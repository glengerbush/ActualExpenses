// Genuine pinned registered GoCardless link/refresh handlers. The loopback
// transport serves already executed, unchanged server-adapter observations.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { readFile, writeFile, mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { existsSync } from "node:fs";
import { createHash } from "node:crypto";
const NativeDate = Date,
  clock = "2026-11-09T12:00:00Z";
process.env.TZ = "UTC";
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
const api = require("@actual-app/api"),
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  plain = (v) => JSON.parse(JSON.stringify(v));
assert.equal(
  JSON.parse(
    await readFile(
      require
        .resolve("@actual-app/api")
        .replace(/dist\/index\.js$/, "package.json"),
      "utf8",
    ),
  ).version,
  "26.10.0",
);
const inputBytes = await readFile(
  new URL("../tests/fixtures/gocardless-source-26.10.json", import.meta.url),
);
const source = JSON.parse(inputBytes);
assert.equal(source.sourceVersion, "26.10.0");
const cases = [
  { id: "default-first-link", source: "booked-pending-balance-priority" },
  { id: "ing-first-link", source: "ing-bank-specific-notes-sort-balance" },
  {
    id: "default-manual-match",
    source: "booked-pending-balance-priority",
    manual: true,
  },
  {
    id: "default-custom-opening",
    source: "booked-pending-balance-priority",
    opening: 555,
  },
  {
    id: "default-no-pending",
    source: "booked-pending-balance-priority",
    pending: false,
  },
];
const directory = await mkdtemp(tmpdir() + "/gocardless-financial-source-"),
  check = process.argv.includes("--check");
const archives = new URL(
    "../tests/fixtures/gocardless-financial-source/",
    import.meta.url,
  ),
  manifest = new URL(
    "../tests/fixtures/gocardless-financial-source-26.10.json",
    import.meta.url,
  );
let response,
  requests = [];
const http = createServer(async (req, res) => {
  assert.equal(req.url, "/gocardless/transactions");
  assert.equal(req.method, "POST");
  assert.equal(req.headers["x-actual-token"], "synthetic-bank-source-token");
  let body = "";
  for await (const part of req) body += part;
  requests.push({ path: req.url, body: JSON.parse(body) });
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify({ status: "ok", data: response }));
});
await new Promise((resolve) => http.listen(0, "127.0.0.1", resolve));
try {
  const engine = await api.init({ dataDir: directory });
  await mkdir(archives, { recursive: true });
  const observations = [];
  for (const [index, item] of cases.entries()) {
    await engine.send("set-server-url", { url: null });
    requests = [];
    response = plain(
      source.observations.find((c) => c.input.id === item.source).result.data,
    );
    const ids = {
      anchor: `29290000-0000-4000-8000-${String(index).padStart(12, "0")}`,
    };
    const filename = item.id + "-26.10.zip",
      archivePath = new URL(filename, archives);
    if (existsSync(archivePath)) {
      await api.importBudget(await readFile(archivePath), { filename });
      ids.account = (await engine.send("accounts-get")).find(
        (a) => a.name === "GoCardless source checking",
      )?.id;
      if (item.manual)
        ids.manualPayee = (await api.getPayees()).find(
          (p) => p.name === "Keep manual payee",
        ).id;
    } else {
      assert.ok(!check, "Missing original preapply archive");
      await api.runImport(item.id, async () => {
        if (item.manual || item.pending === false) {
          ids.account = await api.createAccount(
            { name: "GoCardless source checking" },
            0,
          );
          if (item.manual) {
            ids.manualPayee = await api.createPayee({
              name: "Keep manual payee",
            });
            await api.addTransactions(ids.account, [
              {
                id: ids.anchor,
                date: "2026-11-02",
                amount: -1234,
                payee: ids.manualPayee,
                notes: "Keep manual note",
                cleared: false,
              },
            ]);
          }
          if (item.pending === false)
            await api.setPreference(
              `sync-import-pending-${ids.account}`,
              "false",
            );
        }
      });
    }
    const snapshot = async () =>
      plain({
        transactions: (
          await api.aqlQuery(
            api.q("transactions").select("*").options({ splits: "all" }),
          )
        ).data,
        accounts: await engine.send("accounts-get"),
        payees: await api.getPayees(),
        categories: await api.getCategories(),
        preferences: await api.getPreferences(),
      });
    const before = await snapshot();
    const archive = existsSync(archivePath)
      ? await readFile(archivePath)
      : await api.exportBudget();
    if (!existsSync(archivePath))
      await writeFile(archivePath, archive, { flag: "wx" });
    await engine.send("set-server-url", {
      url: `http://127.0.0.1:${http.address().port}`,
      validate: false,
    });
    await engine.send("subscribe-set-token", {
      token: "synthetic-bank-source-token",
    });
    const result = await engine.send("gocardless-accounts-link", {
      requisitionId: "requisition-A",
      account: {
        account_id: "remote-A",
        institution: {
          id: response.institutionId,
          name: "Synthetic institution",
        },
        name: "GoCardless source checking",
        mask: "1234",
        official_name: "Synthetic checking",
      },
      ...(ids.account ? { upgradingId: ids.account } : {}),
      startingDate: "2026-11-01",
      ...(item.opening !== undefined ? { startingBalance: item.opening } : {}),
    });
    assert.equal(result, "ok");
    const after = await snapshot();
    ids.account ??= after.accounts.find(
      (a) => a.name === "GoCardless source checking",
    ).id;
    const firstIds = after.transactions.map((t) => t.id).sort();
    const repeated = await engine.send("accounts-bank-sync", {
      ids: [ids.account],
    });
    assert.deepEqual(repeated.errors, []);
    const repeat = await snapshot();
    assert.deepEqual(
      repeat.transactions.map((t) => t.id).sort(),
      firstIds,
      "Repeated remote rows retain every first identity",
    );
    assert.deepEqual(
      repeat.transactions,
      after.transactions,
      "Repeat preserves complete financial graph",
    );
    if (item.manual)
      assert.ok(
        after.transactions.some(
          (t) => t.id === ids.anchor && t.notes === "Keep manual note",
        ),
      );
    assert.equal(requests.length, 2);
    await engine.send("set-server-url", { url: null });
    let linked;
    if (index === 0) {
      const linkedFilename = "linked-default-first-link-26.10.zip";
      const linkedPath = new URL(linkedFilename, archives);
      const linkedBytes = existsSync(linkedPath)
        ? await readFile(linkedPath)
        : await api.exportBudget();
      if (!existsSync(linkedPath)) {
        assert.ok(!check);
        await writeFile(linkedPath, linkedBytes, { flag: "wx" });
      }
      // Read the immutable linked export through the genuine engine, preserving
      // that archive's own original generated IDs rather than a fresh run's IDs.
      await api.importBudget(linkedBytes, { filename: linkedFilename });
      linked = {
        filename: linkedFilename,
        sha256: hash(linkedBytes),
        snapshot: await snapshot(),
      };
    }
    observations.push({
      id: item.id,
      input: item,
      ids,
      before,
      archive: { filename, sha256: hash(archive) },
      transport_response: plain(response),
      requests: plain(requests),
      result,
      after,
      repeated,
      repeat,
      ...(linked ? { linked } : {}),
    });
  }
  const files = [
    "packages/loot-core/src/server/accounts/sync.ts",
    "packages/loot-core/src/server/accounts/app.ts",
    "packages/loot-core/src/server/accounts/link.ts",
    "packages/loot-core/src/server/util/custom-sync-mapping.ts",
  ];
  const output = {
    sourceVersion: "26.10.0",
    sourceCommit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    apiBundleSha256: hash(await readFile(require.resolve("@actual-app/api"))),
    adapterFixtureSha256: hash(inputBytes),
    sourceHashes: Object.fromEntries(
      await Promise.all(
        files.map(async (file) => [
          file,
          hash(
            await readFile(
              new URL("../reference/actual-26.10.0/" + file, import.meta.url),
            ),
          ),
        ]),
      ),
    ),
    scope:
      "Whole unchanged registered GoCardless first-link and repeat financial graphs, serving exact executed server-adapter output. Official preapply/linked exports clear CRDT; values/identities only. Generated UUID aliases and true date/sort-order ties normalize solely for independent --check; every financial field/edge/multiplicity retained.",
    cases: observations,
  };
  // Same generated-UUID/true-tie normalization as reviewed bank-sync oracle.
  function stable(value) {
    const ids = new Map();
    const id = (v) => {
      if (v.startsWith("29290000-")) return v;
      if (!ids.has(v)) ids.set(v, `generated-${ids.size}`);
      return ids.get(v);
    };
    function ties(rows) {
      if (
        !rows.every(
          (r) =>
            r &&
            typeof r === "object" &&
            typeof r.id === "string" &&
            typeof r.amount === "number" &&
            typeof r.date === "string" &&
            typeof r.sort_order === "number",
        )
      )
        return rows;
      const out = [...rows],
        groups = new Map();
      for (const [i, r] of rows.entries()) {
        const key = JSON.stringify([
          r.date,
          r.sort_order,
          !!r.starting_balance_flag,
        ]);
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(i);
      }
      const signature = (r) =>
        JSON.stringify([
          r.imported_id ?? "",
          r.id.startsWith("29290000-") ? r.id : "",
          r.amount,
          r.notes ?? null,
          r.imported_payee ?? null,
          !!r.cleared,
          !!r.reconciled,
        ]);
      for (const indexes of groups.values()) {
        const sorted = indexes
          .map((i) => rows[i])
          .sort((a, b) => signature(a).localeCompare(signature(b)));
        for (const [i, index] of indexes.entries()) out[index] = sorted[i];
      }
      return out;
    }
    const walk = (v) =>
      typeof v === "string"
        ? v.replace(
            /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,
            id,
          )
        : Array.isArray(v)
          ? ties(v).map(walk)
          : v && typeof v === "object"
            ? Object.fromEntries(
                Object.entries(v).map(([k, x]) => [
                  k.replace(
                    /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,
                    id,
                  ),
                  walk(x),
                ]),
              )
            : v;
    return walk(value);
  }
  if (check)
    assert.deepEqual(
      stable(output),
      stable(JSON.parse(await readFile(manifest, "utf8"))),
    );
  else
    await writeFile(manifest, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS five genuine GoCardless financial link/repeat graphs + original preapply/linked archives",
  );
} finally {
  await api.shutdown();
  http.closeAllConnections();
  await new Promise((resolve) => http.close(resolve));
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}
