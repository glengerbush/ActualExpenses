// Linux process/cgroup counters and browser network observations for qualification.
// RSS sums include shared pages. Keep block-device I/O counters separate: stacking
// device-mapper and physical-device bytes would count the same writes twice.
import { readFile, readdir } from "node:fs/promises";
import { execFileSync } from "node:child_process";
const ticks = Number(
  execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" }),
);
const pageSize = Number(
  execFileSync("getconf", ["PAGESIZE"], { encoding: "utf8" }),
);
async function text(path) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    if (["ENOENT", "ESRCH", "EACCES"].includes(error.code)) return null;
    throw error;
  }
}
function pairs(value) {
  return Object.fromEntries(
    (value ?? "")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const [name, count] = line.replace(":", "").trim().split(/\s+/);
        return [name, Number(count)];
      }),
  );
}
export async function processChildren(pid) {
  // Linux records children on the thread which spawned them, not necessarily
  // the process leader. Node and Chromium both spawn from auxiliary threads.
  let threads;
  try {
    threads = await readdir(`/proc/${pid}/task`);
  } catch (error) {
    if (["ENOENT", "ESRCH"].includes(error.code)) return [];
    throw error;
  }
  const children = await Promise.all(
    threads.map((thread) => text(`/proc/${pid}/task/${thread}/children`)),
  );
  return [
    ...new Set(
      children.flatMap(
        (value) => value?.trim().split(/\s+/).filter(Boolean) ?? [],
      ),
    ),
  ];
}
async function processes(root, found = new Map()) {
  const stat = await text(`/proc/${root}/stat`);
  if (!stat) return found;
  const values = stat.slice(stat.lastIndexOf(")") + 2).split(/\s+/);
  const io = pairs(await text(`/proc/${root}/io`));
  const memory = pairs(await text(`/proc/${root}/smaps_rollup`));
  found.set(`${root}:${values[19]}`, {
    cpu_ms: ((Number(values[11]) + Number(values[12])) * 1000) / ticks,
    rss_bytes: Number(values[21]) * pageSize,
    pss_bytes: memory.Pss === undefined ? null : memory.Pss * 1024,
    read_bytes: io.read_bytes ?? null,
    write_bytes: io.write_bytes ?? null,
  });
  for (const child of await processChildren(root))
    await processes(child, found);
  return found;
}
async function cgroup(path) {
  const cpu = pairs(await text(path + "/cpu.stat"));
  const memory = await text(path + "/memory.current");
  if (cpu.usage_usec === undefined || memory === null)
    throw new Error("Missing measured cgroup: " + path);
  const pid = (await text(path + "/cgroup.procs"))?.trim().split(/\s+/)[0];
  const devices = await text(`/proc/${pid}/net/dev`);
  if (!devices) throw new Error("Container network counters unavailable");
  const network = Object.fromEntries(
    devices
      .split("\n")
      .filter((line) => line.includes(":"))
      .map((line) => {
        const [name, data] = line.split(":"),
          values = data.trim().split(/\s+/).map(Number);
        return [name.trim(), { rx_bytes: values[0], tx_bytes: values[8] }];
      }),
  );
  const io = Object.fromEntries(
    ((await text(path + "/io.stat")) ?? "")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const [device, ...fields] = line.split(/\s+/);
        return [
          device,
          Object.fromEntries(
            fields.map((field) => {
              const [key, value] = field.split("=");
              return [key, Number(value)];
            }),
          ),
        ];
      }),
  );
  return {
    cpu_ms: cpu.usage_usec / 1000,
    memory_bytes: Number(memory.trim()),
    io,
    network,
  };
}
function difference(after, before) {
  return Object.fromEntries(
    Object.entries(after).map(([key, value]) => [
      key,
      value - (before?.[key] ?? 0),
    ]),
  );
}
export async function measure({
  browserPid,
  serverCgroup,
  context,
  action,
  settle,
  interval = 250,
}) {
  const network = [],
    pending = new Set(),
    failures = [],
    inflight = new Set();
  const started = (request) => inflight.add(request);
  const finished = (request) => {
    inflight.delete(request);
    const promise = Promise.all([request.sizes(), request.response()])
      .then(([sizes, response]) =>
        network.push({
          method: request.method(),
          origin: new URL(request.url()).origin,
          path: new URL(request.url()).pathname,
          status: response?.status(),
          ...sizes,
        }),
      )
      .catch((error) => failures.push(String(error)));
    pending.add(promise);
    promise.finally(() => pending.delete(promise));
  };
  const failed = (request) => {
    inflight.delete(request);
    const url = new URL(request.url());
    failures.push({
      origin: url.origin,
      path: url.pathname,
      error: request.failure()?.errorText,
    });
  };
  context.on("request", started);
  context.on("requestfinished", finished);
  context.on("requestfailed", failed);
  const initial = await processes(browserPid),
    last = new Map(initial);
  const before = await cgroup(serverCgroup);
  let peakRss = 0,
    peakPss = 0,
    missingPss = 0,
    peakServer = before.memory_bytes,
    sampling = false,
    sampleError;
  async function sample() {
    if (sampling) return;
    sampling = true;
    try {
      const rows = await processes(browserPid);
      peakRss = Math.max(
        peakRss,
        [...rows.values()].reduce((sum, row) => sum + row.rss_bytes, 0),
      );
      peakPss = Math.max(
        peakPss,
        [...rows.values()].reduce((sum, row) => sum + (row.pss_bytes ?? 0), 0),
      );
      missingPss = Math.max(
        missingPss,
        [...rows.values()].filter((row) => row.pss_bytes === null).length,
      );
      for (const [id, row] of rows) last.set(id, row);
      peakServer = Math.max(
        peakServer,
        (await cgroup(serverCgroup)).memory_bytes,
      );
    } catch (error) {
      sampleError = error;
    } finally {
      sampling = false;
    }
  }
  await sample();
  const timer = setInterval(sample, interval);
  const start = performance.now();
  let value,
    elapsed,
    actionCompleted = false;
  try {
    value = await action();
    actionCompleted = true;
    elapsed = performance.now() - start;
  } finally {
    clearInterval(timer);
    while (sampling) await new Promise((resolve) => setTimeout(resolve, 5));
    try {
      // Reading process counters takes time. A deferred application request can
      // begin during that final sample; drain it and resample before closing.
      do {
        await sample();
        await Promise.all([...pending]);
      } while (actionCompleted && settle && (await settle()));
    } finally {
      context.off("request", started);
      context.off("requestfinished", finished);
      context.off("requestfailed", failed);
    }
  }
  const after = await cgroup(serverCgroup);
  if (sampleError) throw sampleError;
  const client = {
    cpu_ms: 0,
    read_bytes: 0,
    write_bytes: 0,
    peak_rss_bytes: peakRss,
    peak_pss_bytes: missingPss ? null : peakPss,
    unavailable_pss_processes: missingPss,
    unavailable_io_processes: 0,
  };
  for (const [id, row] of last) {
    for (const field of ["cpu_ms", "read_bytes", "write_bytes"]) {
      if (row[field] === null) continue;
      client[field] += row[field] - (initial.get(id)?.[field] ?? 0);
    }
    if (row.write_bytes === null) client.unavailable_io_processes++;
  }
  return {
    elapsed_ms: elapsed,
    client,
    server: {
      cpu_ms: after.cpu_ms - before.cpu_ms,
      peak_memory_bytes: peakServer,
      memory_bytes: after.memory_bytes,
      network: Object.fromEntries(
        Object.entries(after.network).map(([device, row]) => [
          device,
          difference(row, before.network[device]),
        ]),
      ),
      io: Object.fromEntries(
        Object.entries(after.io).map(([device, row]) => [
          device,
          difference(row, before.io[device]),
        ]),
      ),
    },
    network,
    inflight: [...inflight].map((request) => new URL(request.url()).pathname),
    failures,
    value,
  };
}

export function percentile(values, probability) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(probability * sorted.length) - 1)];
}
