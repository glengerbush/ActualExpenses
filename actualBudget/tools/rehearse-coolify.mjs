// Disposable, loopback-only image/data replacement and rollback rehearsal.
// Requires Docker, the pinned Actual API dependencies and a built native image.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const webRequire = createRequire(
  new URL("../web/package.json", import.meta.url),
);
const { chromium, expect } = webRequire("@playwright/test");
const [image, outputArg] = process.argv.slice(2);
assert.ok(
  image && outputArg,
  "Usage: node rehearse-coolify.mjs LOCAL_IMAGE NEW_OUTPUT_DIRECTORY",
);
const output = resolve(outputArg);
await mkdir(output, { mode: 0o700 }); // Never reuse another rehearsal's evidence.
const prefix = `actualbudget-cutover-${randomUUID()}`;
const oldImage =
  "actualbudget/actual-server@sha256:24645e971da6bb1a1f953d6859e307a0b29be1e8b4130a35b3220c209b5b860c";
const helperImage =
  "debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc";
const url = "http://127.0.0.1:5006";
const password = randomBytes(24).toString("hex");
const volumes = [],
  containers = [];
const evidence = {
  image,
  oldImage,
  port: "127.0.0.1:5006:5006",
  mount: "/data",
  checks: [],
};
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const docker = (...args) =>
  execFileSync("docker", args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  }).trim();
async function save(name, bytes) {
  await writeFile(`${output}/${name}`, bytes, { mode: 0o600, flag: "wx" });
  return hash(bytes);
}
async function call(
  path,
  body,
  method = body === undefined ? "GET" : "POST",
  expected = body === undefined ? 200 : 201,
) {
  const response = await fetch(url + path, {
    method,
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
    signal: AbortSignal.timeout(30000),
  });
  assert.equal(
    response.status,
    expected,
    `${method} ${path}: ${(await response.clone().text()).slice(0, 1000)}`,
  );
  return response.headers.get("content-type")?.includes("json")
    ? response.json()
    : Buffer.from(await response.arrayBuffer());
}
function volume(label) {
  const name = `${prefix}-actualbudget-data-${label}`;
  docker("volume", "create", name);
  volumes.push(name);
  return name;
}
function copy(from, to) {
  docker(
    "run",
    "--rm",
    "--network",
    "none",
    "--read-only",
    "--mount",
    `type=volume,src=${from},dst=/from,readonly`,
    "--mount",
    `type=volume,src=${to},dst=/to`,
    helperImage,
    "sh",
    "-c",
    "cd /from && tar cf - . | tar xpf - -C /to",
  );
}
function digestVolume(name) {
  return hash(
    execFileSync(
      "docker",
      [
        "run",
        "--rm",
        "--network",
        "none",
        "--read-only",
        "--mount",
        `type=volume,src=${name},dst=/data,readonly`,
        helperImage,
        "tar",
        "--sort=name",
        "--numeric-owner",
        "-cpf",
        "-",
        "-C",
        "/data",
        ".",
      ],
      { maxBuffer: 128 * 1024 * 1024 },
    ),
  );
}
async function start(label, selectedImage, storage, native = false) {
  const name = `${prefix}-${label}`;
  containers.push(name);
  docker(
    "run",
    "--detach",
    "--name",
    name,
    "--publish",
    "127.0.0.1:5006:5006",
    "--mount",
    `type=volume,src=${storage},dst=/data`,
    selectedImage,
  );
  for (let attempt = 0; attempt < 150; attempt++) {
    try {
      if ((await fetch(url + (native ? "/api/v1/info" : "/health"))).ok)
        return name;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw Error("Owned rehearsal container failed readiness: " + name);
}
async function sourceClient(label) {
  const cache = `${output}/${label}`;
  await mkdir(cache);
  return api.init({ dataDir: cache, serverURL: url, password });
}
function projection(row, native = false) {
  return {
    id: row.id,
    account: row[native ? "account_id" : "account"],
    date: String(row.date).replace(/^(\d{4})(\d{2})(\d{2})$/, "$1-$2-$3"),
    amount: row.amount,
    notes: row.notes ?? "",
    cleared: !!row.cleared,
  };
}
const sorted = (rows) => rows.toSorted((a, b) => a.id.localeCompare(b.id));
async function sourceRows() {
  return sorted(
    (
      await api.aqlQuery(
        api.q("transactions").select("*").orderBy({ id: "asc" }),
      )
    ).data.map((row) => projection(row)),
  );
}
let browser;
try {
  evidence.nativeImageId = docker(
    "image",
    "inspect",
    "--format",
    "{{.Id}}",
    image,
  );
  const original = volume("old"),
    backup = volume("backup"),
    replacement = volume("replacement"),
    rollback = volume("rollback");
  const old = await start("old", oldImage, original);
  const bootstrap = await call("/account/needs-bootstrap");
  assert.equal(bootstrap.data.bootstrapped, false);
  await call("/account/bootstrap", { password }, "POST", 200);
  await sourceClient("source-initial");
  await api.importBudget(
    await readFile(
      new URL(
        "../tests/fixtures/complete-migration/empty.zip",
        import.meta.url,
      ),
    ),
    { filename: "empty.zip" },
  );
  const account = await api.createAccount({ name: "Spending" }, 100000);
  const edited = randomUUID(),
    deleted = randomUUID(),
    added = randomUUID();
  await api.addTransactions(account, [
    {
      id: edited,
      date: "2026-11-01",
      amount: -1000,
      notes: "Edit after cutover",
      cleared: true,
    },
    {
      id: deleted,
      date: "2026-11-02",
      amount: -2000,
      notes: "Delete after cutover",
      cleared: true,
    },
  ]);
  await api.sync();
  const remote = (await api.getBudgets()).find(
    (b) => b.cloudFileId && b.groupId,
  );
  assert.ok(remote);
  const before = await sourceRows();
  assert.equal(before.length, 3);
  const exported = Buffer.from(await api.exportBudget());
  evidence.sourceExportSha256 = await save("source-export.zip", exported);
  evidence.before = before;
  await api.shutdown();
  docker("stop", old);
  copy(original, backup);
  copy(backup, replacement);
  evidence.stoppedVolumeSha256 = digestVolume(backup);
  assert.equal(digestVolume(original), evidence.stoppedVolumeSha256);
  assert.equal(digestVolume(replacement), evidence.stoppedVolumeSha256);
  docker(
    "run",
    "--rm",
    "--network",
    "none",
    "--mount",
    `type=volume,src=${replacement},dst=/data`,
    helperImage,
    "chown",
    "-R",
    "10001:10001",
    "/data",
  );
  const native = await start("native", image, replacement, true);
  const inspection = JSON.parse(docker("inspect", native))[0];
  assert.equal(inspection.Config.User, "10001:10001");
  assert.equal(
    inspection.Mounts.find((m) => m.Destination === "/data").Name,
    replacement,
  );
  assert.equal((await call("/api/v1/info")).data.authentication, "none");
  assert.deepEqual((await call("/api/v1/budgets")).data, []); // Image change alone is not migration.
  const stage = (await call("/api/v1/imports/actual", exported)).data;
  assert.equal(stage.status, "ready");
  evidence.conversionReport = stage.report;
  await call(`/api/v1/imports/${stage.id}/activate`, {});
  const root = `/api/v1/budgets/${stage.budget_id}`;
  const nativeRows = async () =>
    sorted(
      (await call(root + `/transactions?account_id=${account}`)).data.map(
        (row) => projection(row, true),
      ),
    );
  assert.deepEqual(await nativeRows(), before);
  docker("restart", native);
  for (let i = 0; i < 100; i++) {
    try {
      await call("/api/v1/info");
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  assert.deepEqual(await nativeRows(), before);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage();
  await page.goto("http://actualbudget.test:5006");
  assert.equal(await page.evaluate(() => isSecureContext), false);
  await page.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget: stage.budget_id, account },
  );
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("970.00");
  await page.screenshot({
    path: `${output}/imported-browser.png`,
    fullPage: true,
  });
  await call(
    root + `/transactions/${edited}`,
    { amount: -1500, notes: "Edited after cutover" },
    "PATCH",
    200,
  );
  await call(root + `/transactions/${deleted}`, {}, "DELETE", 200);
  await call(root + "/transactions", {
    id: added,
    account_id: account,
    date: "2026-11-03",
    amount: -345,
    notes: "Added after cutover",
    cleared: true,
  });
  const after = await nativeRows();
  assert.equal(
    after.reduce((n, r) => n + r.amount, 0),
    98155,
  );
  assert.equal(after.length, 3);
  evidence.after = after;
  const nativeBackup = (
    await call(root + "/backups", { label: "Preserve post-cutover writes" })
  ).data;
  evidence.nativeArchiveSha256 = await save(
    "native-post-cutover.zip",
    await call(root + `/backups/${nativeBackup.id}/download`),
  );
  await page.reload();
  await expect(page.getByTestId("register-total")).toHaveText("981.55");
  await browser.close();
  browser = null;
  docker("stop", native);
  const retainedNative = volume("retained-native");
  copy(replacement, retainedNative);
  evidence.retainedNativeVolumeSha256 = digestVolume(retainedNative);
  assert.equal(digestVolume(replacement), evidence.retainedNativeVolumeSha256);
  copy(backup, rollback);
  assert.equal(digestVolume(rollback), evidence.stoppedVolumeSha256);
  await start("rollback", oldImage, rollback);
  await sourceClient("source-rollback");
  await api.downloadBudget(remote.groupId);
  assert.deepEqual(await sourceRows(), before);
  // Reconcile supported old-app operations; never feed a native archive to Actual.
  await api.updateTransaction(edited, {
    amount: -1500,
    notes: "Edited after cutover",
  });
  await api.deleteTransaction(deleted);
  await api.addTransactions(account, [
    {
      id: added,
      date: "2026-11-03",
      amount: -345,
      notes: "Added after cutover",
      cleared: true,
    },
  ]);
  await api.sync();
  assert.deepEqual(await sourceRows(), after);
  evidence.reconciledExportSha256 = await save(
    "reconciled-source.zip",
    Buffer.from(await api.exportBudget()),
  );
  await api.shutdown();
  await sourceClient("source-independent-readback");
  await api.downloadBudget(remote.groupId);
  assert.deepEqual(await sourceRows(), after);
  await api.shutdown();
  assert.equal(digestVolume(backup), evidence.stoppedVolumeSha256);
  assert.equal(
    digestVolume(retainedNative),
    evidence.retainedNativeVolumeSha256,
  );
  evidence.checks = [
    "matching stopped-volume copies",
    "UID10001 write access",
    "same port and /data mount",
    "explicit staged import",
    "all fixture rows and exact balances",
    "restart persistence",
    "plain-HTTP browser",
    "preserved native archive and volume",
    "matching old image/data rollback",
    "addition/edit/deletion reconciliation",
    "independent old-client readback",
    "unchanged recovery copies",
  ];
  await save("result.json", JSON.stringify(evidence, null, 2) + "\n");
  console.log(
    "PASS copied-volume cutover, restart, old-image/data rollback and reconciled addition/edit/deletion; final balance98155",
  );
} finally {
  await browser?.close();
  await api.shutdown().catch(() => {});
  for (const name of containers) {
    try {
      docker("rm", "-f", name);
    } catch {}
  }
  for (const name of volumes) {
    try {
      docker("volume", "rm", name);
    } catch {}
  }
}
