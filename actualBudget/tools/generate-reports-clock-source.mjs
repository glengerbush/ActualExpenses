// Genuine synchronized source mutations; never manufacture SQLite state or clocks.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:net";
import { randomBytes, createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

process.env.TZ = "UTC";
const NativeDate = Date;
let clock = "2026-11-09T12:00:00.000Z";
globalThis.Date = class extends NativeDate {
  constructor(...args) {
    super(...(args.length ? args : [clock]));
  }
  static now() {
    return NativeDate.parse(clock);
  }
};
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const fixtures = new URL("../tests/fixtures/", import.meta.url);
const metadataPath = new URL("reports-clock-source-26.10.json", fixtures);
const check = process.argv.includes("--check");
const existing = await readFile(metadataPath, "utf8").catch((error) => {
  if (error.code === "ENOENT") return null;
  throw error;
});
assert.ok(
  check ? existing : !existing,
  check
    ? "Missing committed generation metadata"
    : "Refusing to overwrite source fixtures",
);
const root = await mkdtemp(join(tmpdir(), "reports-clock-source-"));
const existingResult = existing ? JSON.parse(existing) : null;
let child,
  initialized = false,
  logs = "";
const result = {
  oracle: "@actual-app/api@26.10.0 + @actual-app/sync-server@26.10.0",
  source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
};
const inspect = (path) =>
  JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `
import json,pathlib,sqlite3,tempfile,zipfile
with zipfile.ZipFile(__import__('sys').argv[1]) as z,tempfile.TemporaryDirectory() as tmp:
 p=pathlib.Path(tmp)/'source.sqlite';p.write_bytes(z.read('db.sqlite'))
 db=sqlite3.connect(p.as_uri()+'?mode=ro&immutable=1',uri=True);db.row_factory=sqlite3.Row
 assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
 clocks=[dict(r) for r in db.execute("SELECT dataset,row,column,timestamp FROM messages_crdt WHERE dataset IN ('dashboard','dashboard_pages','custom_reports') ORDER BY dataset,row,column,timestamp")]
 assert len(clocks)>0
 print(json.dumps({'clocks':clocks}))
`,
        path,
      ],
      { encoding: "utf8" },
    ),
  );
async function json(url, body) {
  const response = await fetch(url, {
    ...(body === undefined
      ? {}
      : {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        }),
    signal: AbortSignal.timeout(5000),
  });
  assert.equal(response.status, 200, await response.clone().text());
  const value = await response.json();
  assert.equal(value.status, "ok");
  return value.data;
}
try {
  for (const name of ["api", "sync-server"])
    assert.equal(
      JSON.parse(
        await readFile(
          new URL(
            `../../actualExpenses/tools/compatibility/node_modules/@actual-app/${name}/package.json`,
            import.meta.url,
          ),
          "utf8",
        ),
      ).version,
      "26.10.0",
    );
  const reservation = createServer();
  await new Promise((resolve, reject) => {
    reservation.once("error", reject);
    reservation.listen(0, "127.0.0.1", resolve);
  });
  const port = reservation.address().port;
  await new Promise((resolve) => reservation.close(resolve));
  const serverDir = join(root, "server");
  await mkdir(serverDir);
  child = spawn(
    process.execPath,
    [
      fileURLToPath(
        new URL(
          "../../actualExpenses/tools/compatibility/node_modules/@actual-app/sync-server/build/bin/actual-server.js",
          import.meta.url,
        ),
      ),
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        NODE_ENV: "production",
        ACTUAL_DATA_DIR: serverDir,
        ACTUAL_HOSTNAME: "127.0.0.1",
        ACTUAL_PORT: String(port),
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (data) => {
      logs = (logs + data).slice(-16384);
    });
  const url = `http://127.0.0.1:${port}`,
    deadline = performance.now() + 30000;
  for (;;) {
    assert.equal(child.exitCode, null, logs);
    try {
      if (
        (
          await fetch(url + "/account/needs-bootstrap", {
            signal: AbortSignal.timeout(1000),
          })
        ).ok
      )
        break;
    } catch {}
    assert.ok(performance.now() < deadline, logs);
    await delay(100);
  }
  const password = randomBytes(24).toString("hex");
  await json(url + "/account/bootstrap", { password });
  const dataDir = join(root, "client");
  await mkdir(dataDir);
  const engine = await api.init({ dataDir, serverURL: url, password });
  initialized = true;
  let account;
  await api.runImport("Schedule generation source", async () => {
    account = await api.createAccount({ name: "Spending" }, 0);
  });
  await api.sync();
  clock = "2026-11-09T12:00:01.000Z";
  result.page = await engine.send("dashboard-create", {
    name: "Source field ordering",
  });
  result.report = await engine.send("report/create", {
    name: "Ordered source report",
    startDate: "2026-10",
    endDate: "2026-11",
    isDateStatic: true,
    dateRange: "Last 6 months",
    mode: "total",
    groupBy: "Category",
    interval: "Monthly",
    balanceType: "Payment",
    sortBy: "desc",
    showEmpty: false,
    showOffBudget: false,
    showHiddenCategories: true,
    showUncategorized: true,
    includeCurrentInterval: true,
    trimIntervals: false,
    showTrendLines: false,
    graphType: "BarGraph",
    conditions: [],
    conditionsOp: "and",
  });
  await engine.send("dashboard-add-widget", {
    dashboard_page_id: result.page,
    type: "summary-card",
    x: 0,
    y: 0,
    width: 6,
    height: 3,
    meta: {
      name: "Source summary",
      content: '{"type":"sum"}',
      timeFrame: { start: "2026-10", end: "2026-11", mode: "static" },
    },
  });
  await api.sync();
  result.widget = (
    await api.aqlQuery(
      api.q("dashboard").filter({ dashboard_page_id: result.page }).select("*"),
    )
  ).data[0].id;
  clock = "2026-11-09T12:00:02.000Z";
  await engine.send("dashboard-rename", {
    id: result.page,
    name: "Newer source field ordering",
  });
  await api.sync();
  const bytes = await api.exportBudget(),
    archive = "reports-clock-source-26.10.zip",
    local = join(root, archive);
  await writeFile(local, bytes);
  Object.assign(result, inspect(local));
  result.documents = {
    dashboard_pages: (await api.aqlQuery(api.q("dashboard_pages").select("*")))
      .data,
    dashboard: (await api.aqlQuery(api.q("dashboard").select("*"))).data,
    custom_reports: (await api.aqlQuery(api.q("custom_reports").select("*")))
      .data,
  };
  result.archive = archive;
  result.archive_sha256 = createHash("sha256").update(bytes).digest("hex");
  if (!check) {
    await writeFile(new URL(archive, fixtures), bytes, { flag: "wx" });
    await writeFile(metadataPath, JSON.stringify(result, null, 2) + "\n", {
      flag: "wx",
    });
  } else {
    // IDs and HLC node IDs are source-generated. Compare field/value coverage and
    // source ordering within each field; original bytes/hash stay immutable.
    const coverage = (r) =>
      r.clocks.map((c) => c.dataset + ":" + c.column).sort();
    assert.deepEqual(coverage(result), coverage(existingResult));
    assert.equal(
      existingResult.archive_sha256,
      createHash("sha256")
        .update(await readFile(new URL(archive, fixtures)))
        .digest("hex"),
    );
  }
  console.log(
    "PASS genuine pinned synchronized dashboard/report field clocks:" +
      result.clocks.length,
  );
} finally {
  if (initialized) await api.shutdown();
  if (child && child.exitCode === null) {
    const closed = new Promise((resolve) => child.once("close", resolve));
    child.kill("SIGTERM");
    await closed;
  }
  globalThis.Date = NativeDate;
  await rm(root, { recursive: true, force: true });
}
