// Release-container history stress through public HTTP; no private database writes.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { EventEmitter } from "node:events";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { resolve } from "node:path";
import { measure, percentile } from "./resource-metrics.mjs";

const [container, archiveArgument, outputArgument, countArgument = "1000"] =
  process.argv.slice(2);
assert.ok(
  container && archiveArgument && outputArgument,
  "Usage: node check-resource-history.mjs actualbudget-q51-CONTAINER ARCHIVE NEW_OUTPUT_DIRECTORY [CYCLES=1000]",
);
assert.match(container, /^actualbudget-q51-[a-z0-9-]+$/);
const count = Number(countArgument);
assert.ok(Number.isInteger(count) && count >= 20 && count <= 10000);
const docker = (...args) => execFileSync("docker", args, { encoding: "utf8" });
const [info] = JSON.parse(docker("inspect", container));
assert.ok(info.State.Running);
const bindings = info.NetworkSettings.Ports["5006/tcp"];
assert.equal(bindings.length, 1);
assert.equal(bindings[0].HostIp, "127.0.0.1");
assert.equal(info.Mounts.length, 1);
assert.equal(info.Mounts[0].Type, "volume");
assert.equal(info.Mounts[0].Destination, "/data");
const cgroup = (await readFile(`/proc/${info.State.Pid}/cgroup`, "utf8"))
  .trim()
  .split("\n")
  .find((line) => line.startsWith("0::"))
  ?.slice(3);
assert.ok(cgroup?.includes(info.Id));
const api = `http://127.0.0.1:${bindings[0].HostPort}/api/v1`,
  output = resolve(outputArgument),
  bytes = await readFile(resolve(archiveArgument));
await mkdir(output); // Never repeat mutations against an existing evidence directory.
let headers = {};
async function call(path, body, status = 200, extra = {}) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
      ...extra,
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, status, path + ": " + text.slice(0, 1000));
  return JSON.parse(text);
}
const result = {
  image: info.Image,
  archive_sha256: createHash("sha256").update(bytes).digest("hex"),
  count,
  protocol:
    "Public HTTP; production container; one 10048-row imported budget; 40 owned note edits, repeated undo/redo of newest action, then all20 retained actions; counters include HTTP request/body only; storage snapshots outside timing; caller process is Node, not a browser",
  edits: [],
  undo: [],
  redo: [],
  storage: [],
};
function storage(label) {
  const files = docker(
    "exec",
    container,
    "find",
    "/data",
    "-type",
    "f",
    "-printf",
    "%P\t%s\n",
  )
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [path, bytes] = line.split("\t");
      return { path, bytes: Number(bytes) };
    });
  result.storage.push({
    label,
    files,
    total_bytes: files.reduce((sum, file) => sum + file.bytes, 0),
  });
}
const measured = async (action) =>
  measure({
    browserPid: process.pid,
    serverCgroup: "/sys/fs/cgroup" + cgroup,
    context: new EventEmitter(),
    action: async () => {
      const start = performance.now();
      await action();
      return { http_ms: performance.now() - start };
    },
  });
const quantiles = (rows) => {
  const q = (values) => ({
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
  });
  return {
    samples: rows.length,
    http_ms: q(rows.map((row) => row.value.http_ms)),
    server_cpu_ms: q(rows.map((row) => row.server.cpu_ms)),
    server_peak_memory_bytes: q(
      rows.map((row) => row.server.peak_memory_bytes),
    ),
    server_write_bytes_by_device: Object.fromEntries(
      [...new Set(rows.flatMap((row) => Object.keys(row.server.io)))].map(
        (device) => [
          device,
          q(rows.map((row) => row.server.io[device]?.wbytes ?? 0)),
        ],
      ),
    ),
  };
};
try {
  assert.deepEqual(
    (await call("/budgets")).data,
    [],
    "Refusing a populated server",
  );
  const stage = (await call("/imports/actual", bytes, 201)).data;
  const budget = (await call(`/imports/${stage.id}/activate`, {}, 201)).data,
    root = `/budgets/${budget.id}`;
  result.budget_id = budget.id;
  headers = {
    "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
    "X-Actual-History-Owner": randomUUID(),
  };
  const accountRows = async () =>
    (await call(root + "/accounts")).data
      .map(({ id, balance }) => ({ id, balance }))
      .sort((a, b) => a.id.localeCompare(b.id));
  const beforeAccounts = await accountRows();
  const transaction = "50500000-0000-4000-8000-000000000000";
  const selected = async () =>
    (await call(root + "/transactions/" + transaction)).data;
  const original = await selected();
  assert.equal(original.reconciled, false);
  storage("after_import");
  for (let index = 1; index <= 40; index++) {
    const snapshot = (await call(root + "/checkpoint")).data.snapshot;
    result.edits.push(
      await measured(() =>
        call(root + "/transactions/batch", {
          snapshot,
          operations: [
            {
              action: "update",
              id: transaction,
              fields: { notes: "Resource history " + index },
            },
          ],
          max_graph_members: 1,
        }),
      ),
    );
  }
  storage("after40_edits");
  for (let index = 1; index <= count; index++) {
    for (const [phase, note] of [
      ["undo", "Resource history 39"],
      ["redo", "Resource history 40"],
    ]) {
      const state = (await call(root + "/history")).data;
      assert.ok(state[phase]?.id);
      result[phase].push(
        await measured(() =>
          call(root + "/history/" + phase, { action_id: state[phase].id }),
        ),
      );
      assert.equal((await selected()).notes, note);
    }
    if (index % 100 === 0 || index === count) {
      storage("after" + index + "_cycles");
      await writeFile(
        output + "/observations.json",
        JSON.stringify(result) + "\n",
      );
      console.log("PASS undo/redo cycles", index, "/", count);
    }
  }
  // Aliases must still preserve sequential ownership at the bounded stack limit.
  for (const phase of ["undo", "redo"]) {
    for (let index = 0; index < 20; index++) {
      const state = (await call(root + "/history")).data;
      assert.ok(state[phase]?.id);
      await call(root + "/history/" + phase, { action_id: state[phase].id });
    }
    assert.equal((await call(root + "/history")).data[phase], null);
    assert.equal(
      (await selected()).notes,
      "Resource history " + (phase === "undo" ? 20 : 40),
    );
  }
  assert.deepEqual(
    await accountRows(),
    beforeAccounts,
    "Undo/redo note stress cannot change balances",
  );
  result.financial = {
    accounts: beforeAccounts,
    transaction,
    amount: original.amount,
    date: original.date,
    final_notes: (await selected()).notes,
    retained_actions: 20,
    sequential_undo_redo: true,
  };
  storage("after_sequential20");
  result.summary = Object.fromEntries(
    ["edits", "undo", "redo"].map((phase) => [
      phase,
      {
        all: quantiles(result[phase]),
        first20: quantiles(result[phase].slice(0, 20)),
        last20: quantiles(result[phase].slice(-20)),
      },
    ]),
  );
} catch (error) {
  result.error = String(error);
  throw error;
} finally {
  await writeFile(
    output + "/observations.json",
    JSON.stringify(result, null, 2) + "\n",
  );
}
console.log(
  "PASS repeated and sequential history, unchanged financial balances",
);
