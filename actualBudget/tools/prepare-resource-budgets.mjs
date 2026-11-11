// Prepare matching benchmarks through the pinned engine on a new disposable server.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { randomBytes, createHash } from "node:crypto";
import { mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { resolve } from "node:path";
const require = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const api = require("@actual-app/api");
const [url, output, mode] = process.argv.slice(2);
assert.ok(mode === undefined || mode === "--workloads");
assert.ok(
  url && output,
  "Usage: node prepare-resource-budgets.mjs http://127.0.0.1:PORT NEW_OUTPUT_DIRECTORY [--workloads]",
);
const server = new URL(url);
assert.equal(server.protocol, "http:");
assert.equal(server.hostname, "127.0.0.1");
assert.ok(
  server.port &&
    server.pathname === "/" &&
    !server.username &&
    !server.password &&
    !server.search &&
    !server.hash,
);
const directory = resolve(output);
await mkdir(directory); // Refuse reuse of an existing fixture/data directory.
const password = randomBytes(24).toString("hex");
async function request(path, body, token) {
  const response = await fetch(server.origin + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { "X-ACTUAL-TOKEN": token } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  assert.ok(response.ok, path + ": " + (await response.clone().text()));
  const result = await response.json();
  assert.equal(result.status, "ok");
  return result.data;
}
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
try {
  assert.equal(
    (await request("/account/needs-bootstrap")).bootstrapped,
    false,
    "Refusing an existing server",
  );
  await request("/account/bootstrap", { password });
  const { token } = await request("/account/login", { password });
  await writeFile(
    directory + "/bootstrap.json",
    JSON.stringify({ server: server.origin, password, token }) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  await mkdir(directory + "/source-client");
  const engine = await api.init({
    dataDir: directory + "/source-client",
    serverURL: server.origin,
    password,
  });
  const cases = [];
  for (const [name, source] of [
    ["empty", "empty"],
    ["typical", "complete"],
    ["large", "large"],
  ]) {
    const bytes = await readFile(
      new URL(
        `../tests/fixtures/complete-migration/${source}.zip`,
        import.meta.url,
      ),
    );
    const original = await api.importBudget(bytes, {
      filename: source + ".zip",
    });
    const originalRemote = (await api.getBudgets()).find(
      (budget) => budget.id === original.id,
    );
    const label = (mode === "--workloads" ? "Workload " : "Resource ") + name;
    await engine.send("close-budget");
    const local = await engine.send("duplicate-budget", {
      id: original.id,
      newName: label,
      cloudSync: true,
      open: "copy",
    });
    if (name === "empty") await api.createAccount({ name: "Checking" }, 0);
    if (name === "large") {
      // Migration concentrates purchases in one year; qualification also needs
      // multi-year history. Change dates through the pinned engine only.
      await engine.send("transactions-batch-update", {
        updated: Array.from({ length: 10000 }, (_, index) => ({
          id: `50500000-0000-4000-8000-${String(index).padStart(12, "0")}`,
          date: `${2017 + (index % 10)}-${String(1 + (Math.floor(index / 10) % 12)).padStart(2, "0")}-${String(1 + (Math.floor(index / 120) % 28)).padStart(2, "0")}`,
        })),
      });
    }
    let reporting;
    if (mode === "--workloads") {
      const page = await engine.send("dashboard-create", {
        name: "Resource latency",
      });
      const widget = await engine.send("dashboard-add-widget", {
        dashboard_page_id: page,
        type: "cash-flow-card",
        x: 0,
        y: 0,
        width: 12,
        height: 4,
        meta: {
          name: "Resource cash flow",
          timeFrame: { start: "2026-10", end: "2026-11", mode: "static" },
          showBalance: true,
        },
      });
      const pages = (await api.aqlQuery(api.q("dashboard_pages").select("id")))
        .data;
      for (const previous of pages.filter((item) => item.id !== page))
        await engine.send("dashboard-delete", previous.id);
      assert.equal(
        (await api.aqlQuery(api.q("dashboard_pages").select("id"))).data.length,
        1,
      );
      reporting = {
        page,
        widget,
        name: "Resource latency",
        widget_name: "Resource cash flow",
      };
    }
    await api.sync();
    const exported = Buffer.from(await api.exportBudget());
    await writeFile(directory + "/" + name + ".zip", exported, { flag: "wx" });
    const accounts = await api.getAccounts();
    const transactions = (
      await api.aqlQuery(
        api.q("transactions").select("*").orderBy({ id: "asc" }),
      )
    ).data;
    const cloud = (await api.getBudgets()).find(
      (budget) => budget.id === local,
    );
    assert.ok(cloud?.cloudFileId, JSON.stringify(cloud));
    cases.push({
      name,
      ...(reporting ? { reporting } : {}),
      label,
      local_id: local,
      cloud_id: cloud.cloudFileId,
      account:
        accounts.find((account) => account.name.toLowerCase() === "checking")
          ?.id ?? accounts[0].id,
      source_archive_sha256: hash(bytes),
      archive_sha256: hash(exported),
      transactions: transactions.length,
      date_range: transactions.length
        ? [
            transactions.map((row) => row.date).sort()[0],
            transactions
              .map((row) => row.date)
              .sort()
              .at(-1),
          ]
        : null,
      ledger_sha256: hash(JSON.stringify(transactions)),
    });
    await engine.send("close-budget");
    await engine.send("delete-budget", {
      id: original.id,
      cloudFileId: originalRemote.cloudFileId,
    });
  }
  const files = await request("/sync/list-user-files", undefined, token);
  assert.equal(files.length, 3);
  await writeFile(
    directory + "/baseline.json",
    JSON.stringify(
      {
        server: server.origin,
        password,
        token,
        source_version: "26.10.0",
        cases,
      },
      null,
      2,
    ) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  console.log(
    "Prepared three isolated baseline budgets and identical replacement inputs:",
    directory,
  );
} finally {
  await api.shutdown();
  await rm(directory + "/source-client", { recursive: true, force: true });
}
