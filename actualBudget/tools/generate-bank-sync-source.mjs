// Six bounded core bank-sync witnesses. All financial normalization, linking,
// opening balances and matching execute in the genuine pinned engine. The only
// stub is the sync-server transport response, NOT raw SimpleFIN protocol parsing.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { mkdtemp, readFile, writeFile, rm, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
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
  ),
  api = require("@actual-app/api");
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
const directory = await mkdtemp(tmpdir() + "/bank-sync-source-"),
  check = process.argv.includes("--check"),
  captureLinked = process.argv.includes("--capture-linked"),
  exportLinked = process.argv.includes("--export-linked") || captureLinked,
  captureEdges = process.argv.includes("--capture-edges"),
  checkEdges = process.argv.includes("--check-edges"),
  fixture = new URL(
    "../tests/fixtures/bank-sync-source-26.10.json",
    import.meta.url,
  ),
  archives = new URL("../tests/fixtures/bank-sync-source/", import.meta.url),
  hash = (v) => createHash("sha256").update(v).digest("hex"),
  plain = (v) => JSON.parse(JSON.stringify(v));
const transaction = (id, amount, booked = true, extra = {}) => ({
  transactionId: id,
  date: "2026-11-01",
  booked,
  payeeName: "CAFE",
  notes: "Bank #note",
  transactionAmount: { amount, currency: "USD" },
  ...extra,
});
const cases = [
  {
    id: "first-sync-opening",
    seed: false,
    rounds: [
      [transaction("posted", "-12.34"), transaction("pending", "-2.00", false)],
    ],
  },
  {
    id: "manual-pending-booked-same-id",
    manual: true,
    rounds: [
      [transaction("stable-id", "-12.34", false)],
      [transaction("stable-id", "-12.34", true, { date: "2026-11-02" })],
    ],
  },
  {
    id: "custom-payment-deposit-mappings",
    preferences: {
      "custom-sync-mappings": JSON.stringify({
        payment: {
          date: "postedDate",
          payee: "otherPayee",
          notes: "otherNotes",
        },
        deposit: {
          date: "transactedDate",
          payee: "otherPayee",
          notes: "otherNotes",
        },
      }),
    },
    rounds: [
      [
        transaction("payment", "-1.23", true, {
          postedDate: "2026-11-03",
          otherPayee: "MAPPED PAYMENT",
          otherNotes: "Mapped #payment",
        }),
        transaction("deposit", "2.34", true, {
          transactedDate: "2026-11-04",
          otherPayee: "MAPPED DEPOSIT",
          otherNotes: "Mapped #deposit",
        }),
      ],
    ],
  },
  {
    id: "exclude-pending-and-notes",
    preferences: {
      "sync-import-pending": "false",
      "sync-import-notes": "false",
    },
    rounds: [
      [transaction("pending", "-8.00", false), transaction("posted", "-3.00")],
    ],
  },
  {
    id: "update-bank-date",
    preferences: { "sync-update-dates": "true" },
    rounds: [
      [transaction("dated", "-4.00")],
      [transaction("dated", "-4.00", true, { date: "2026-11-05" })],
    ],
  },
  {
    id: "balance-only-preference",
    preferences: { "sync-import-transactions": "false" },
    rounds: [[transaction("unimported", "-6.00")]],
    balance: 25000,
  },
];
const originalCount = cases.length;
if (captureEdges || checkEdges)
  cases.push(
    {
      id: "first-sync-imports-disabled",
      empty: true,
      preferences: { "sync-import-transactions": "false" },
      rounds: [[transaction("disabled-but-source-imports", "-6.00")]],
      balance: 25000,
    },
    {
      id: "first-sync-one-decimal",
      seed: false,
      rounds: [[transaction("one-decimal", "-1.2")]],
    },
    {
      id: "first-sync-renamed-starting-reference",
      empty: true,
      renamedReference: true,
      rounds: [[transaction("renamed-reference", "-6.00")]],
    },
  );
const edgeManifest = new URL(
  "../tests/fixtures/bank-sync-edges-source-26.10.json",
  import.meta.url,
);
let priorEdges;
if (captureEdges || checkEdges) {
  try {
    priorEdges = JSON.parse(await readFile(edgeManifest, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
}
let response,
  requests = [];
const http = createServer(async (req, res) => {
  let body = "";
  for await (const chunk of req) body += chunk;
  assert.equal(
    req.url,
    "/simplefin/transactions",
    "No production provider or general sync-server emulation",
  );
  assert.equal(req.method, "POST");
  assert.equal(req.headers["x-actual-token"], "synthetic-bank-source-token");
  requests.push({ path: req.url, body: JSON.parse(body) });
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify({ status: "ok", data: response }));
});
await new Promise((resolve) => http.listen(0, "127.0.0.1", resolve));
try {
  const engine = await api.init({ dataDir: directory });
  await mkdir(archives, { recursive: true });
  const observations = [],
    linkedRecords = [];
  for (const [index, item] of cases.entries()) {
    await engine.send("set-server-url", { url: null });
    requests = [];
    const ids = {
      anchor: `28280000-0000-4000-8000-${String(index).padStart(12, "0")}`,
    };
    await api.runImport(item.id, async () => {
      if (item.seed !== false) {
        ids.account = await api.createAccount({ name: "Source checking" }, 0);
        if (!item.empty) {
          ids.manualPayee = await api.createPayee({ name: "Manual cafe" });
          await api.addTransactions(ids.account, [
            {
              id: ids.anchor,
              date: "2026-11-01",
              amount: item.manual ? -1234 : -1,
              payee: ids.manualPayee,
              notes: "Keep manual note",
              cleared: false,
            },
          ]);
        }
      }
      if (item.renamedReference) {
        ids.startingPayee = await api.createPayee({ name: "Starting Balance" });
        await api.updatePayee(ids.startingPayee, { name: "STARTING BALANCE" });
        ids.startingCategory = (await api.getCategories()).find(
          (c) => c.is_income && c.name === "Starting Balances",
        ).id;
        await api.updateCategory(ids.startingCategory, {
          name: "starting balances",
        });
      }
      for (const [key, value] of Object.entries(item.preferences ?? {}))
        await api.setPreference(`${key}-${ids.account}`, value);
    });
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
    const before = await snapshot(),
      filename = item.id + "-26.10.zip",
      createArchive =
        !check ||
        (captureEdges &&
          index >= originalCount &&
          !priorEdges?.cases.some((c) => c.id === item.id)),
      archive = createArchive
        ? await api.exportBudget()
        : await readFile(new URL(filename, archives));
    if (createArchive)
      await writeFile(new URL(filename, archives), archive, { flag: "wx" });
    await engine.send("set-server-url", {
      url: `http://127.0.0.1:${http.address().port}`,
      validate: false,
    });
    await engine.send("subscribe-set-token", {
      token: "synthetic-bank-source-token",
    });
    const outcomes = [];
    for (const [round, input] of item.rounds.entries()) {
      const current = item.balance ?? 10000;
      response = {
        startingBalance: current,
        balances: [
          {
            balanceAmount: { amount: String(current / 100), currency: "USD" },
            balanceType: "expected",
            referenceDate: "2026-11-09",
          },
        ],
        transactions: {
          all: plain(input),
          booked: input.filter((t) => t.booked),
          pending: input.filter((t) => !t.booked),
        },
      };
      const result =
        round === 0
          ? await engine.send("simplefin-accounts-link", {
              externalAccount: {
                account_id: "synthetic-remote-account",
                name: "Source checking",
                institution: "Synthetic institution",
                orgDomain: "bank.invalid",
              },
              ...(ids.account ? { upgradingId: ids.account } : {}),
              startingDate: "2026-11-01",
            })
          : await engine.send("accounts-bank-sync", { ids: [ids.account] });
      assert.ok(
        round === 0 ? result === "ok" : result.errors.length === 0,
        "Bank handler must complete successfully",
      );
      let after = await snapshot();
      ids.account ??= after.accounts.find(
        (a) => a.name === "Source checking",
      ).id;
      const firstIds = after.transactions.map((t) => t.id).sort();
      const repeat = await engine.send("accounts-bank-sync", {
        ids: [ids.account],
      });
      const repeated = await snapshot();
      assert.deepEqual(repeat.errors, [], "Repeated refresh must succeed");
      assert.deepEqual(
        repeated.transactions.map((t) => t.id).sort(),
        firstIds,
        "Repeated remote input never recreates purchases",
      );
      assert.deepEqual(
        repeated.transactions,
        after.transactions,
        "Repeat preserves complete financial graph",
      );
      if (item.manual)
        assert.ok(
          after.transactions.some(
            (t) => t.id === ids.anchor && t.notes === "Keep manual note",
          ),
        );
      const sourceTotals =
        index >= originalCount
          ? {
              source_ledger_total:
                (
                  await api.aqlQuery(
                    api
                      .q("transactions")
                      .filter({ account: ids.account })
                      .calculate({ $sum: "$amount" }),
                  )
                ).data ?? 0,
            }
          : {};
      outcomes.push({
        ...sourceTotals,
        input: plain(input),
        transport_response: plain(response),
        result,
        after,
        repeat,
        repeated,
      });
    }
    assert.equal(
      requests.length,
      item.rounds.length * 2,
      "Every link/refresh and repeat reaches the transport seam",
    );
    await engine.send("set-server-url", { url: null });
    if (exportLinked && item.id === "first-sync-opening") {
      const linkedFilename = "linked-first-sync-opening-26.10.zip";
      const linkedBytes = captureLinked
        ? await api.exportBudget()
        : await readFile(new URL(linkedFilename, archives));
      if (captureLinked)
        await writeFile(new URL(linkedFilename, archives), linkedBytes, {
          flag: "wx",
        });
      linkedRecords.push({
        id: item.id,
        input: plain(item),
        snapshot: await snapshot(),
        archive: { filename: linkedFilename, sha256: hash(linkedBytes) },
      });
    }
    observations.push({
      id: item.id,
      input: item,
      ids,
      before,
      archive: { filename, sha256: hash(archive) },
      requests: plain(requests),
      outcomes,
    });
  }
  const paths = [
    "packages/loot-core/src/server/accounts/sync.ts",
    "packages/loot-core/src/server/accounts/app.ts",
    "packages/loot-core/src/server/accounts/link.ts",
    "packages/loot-core/src/server/util/custom-sync-mapping.ts",
  ];
  const source_files = Object.fromEntries(
    await Promise.all(
      paths.map(async (p) => [
        p,
        hash(
          await readFile(
            new URL("../reference/actual-26.10.0/" + p, import.meta.url),
          ),
        ),
      ]),
    ),
  );
  const output = {
    oracle:
      "Unchanged @actual-app/api26.10.0 registered bank handlers; loopback sync-server response stub only",
    source_commit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    scope:
      "Core bank normalization/writer, not raw SimpleFIN adapter/claim/HTTP protocol",
    api_bundle_sha256: hash(await readFile(require.resolve("@actual-app/api"))),
    source_files,
    cases: observations.slice(0, originalCount),
  };
  function stable(value) {
    const ids = new Map();
    const id = (v) => {
      if (v.startsWith("28280000-")) return v;
      if (!ids.has(v)) ids.set(v, `generated-${ids.size}`);
      return ids.get(v);
    };
    function tieRows(rows) {
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
      const result = [...rows],
        groups = new Map();
      for (const [index, row] of rows.entries()) {
        const key = JSON.stringify([
          row.date,
          row.sort_order,
          !!row.starting_balance_flag,
        ]);
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(index);
      }
      const signature = (r) =>
        JSON.stringify([
          r.imported_id ?? "",
          r.id.startsWith("28280000-") ? r.id : "",
          r.amount,
          r.notes ?? null,
          r.imported_payee ?? null,
          !!r.cleared,
          !!r.reconciled,
        ]);
      for (const indexes of groups.values()) {
        const tied = indexes
          .map((i) => rows[i])
          .sort((a, b) => signature(a).localeCompare(signature(b)));
        for (const [i, index] of indexes.entries()) result[index] = tied[i];
      }
      return result;
    }
    const walk = (v) =>
      typeof v === "string"
        ? v.replace(
            /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,
            id,
          )
        : Array.isArray(v)
          ? tieRows(v).map(walk)
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
      stable(JSON.parse(await readFile(fixture, "utf8"))),
    );
  else
    await writeFile(fixture, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  if (exportLinked) {
    const manifest = new URL(
      "../tests/fixtures/bank-linked-source-26.10.json",
      import.meta.url,
    );
    const linked = {
      oracle: output.oracle,
      source_commit: output.source_commit,
      clock,
      api_bundle_sha256: output.api_bundle_sha256,
      source_files,
      scope:
        "Fresh genuine linked SimpleFIN export after the unchanged registered handler and repeated source refresh. Official export clears CRDT; values/identities only, no source field-clock claim.",
      cases: linkedRecords,
    };
    if (!captureLinked)
      assert.deepEqual(
        stable(linked),
        stable(JSON.parse(await readFile(manifest, "utf8"))),
      );
    else
      await writeFile(manifest, JSON.stringify(linked, null, 2) + "\n", {
        flag: "wx",
      });
    console.log(
      "PASS genuine linked SimpleFIN export and retained original identities",
    );
  }
  if (captureEdges || checkEdges) {
    const manifest = new URL(
      "../tests/fixtures/bank-sync-edges-source-26.10.json",
      import.meta.url,
    );
    let edgeCases = observations.slice(originalCount);
    for (const prior of priorEdges?.cases ?? []) {
      const current = edgeCases.find((c) => c.id === prior.id);
      assert.deepEqual(
        stable(current),
        stable(prior),
        "Previously captured edge observation must remain unchanged",
      );
    }
    if (captureEdges && priorEdges)
      edgeCases = edgeCases.map(
        (c) => priorEdges.cases.find((old) => old.id === c.id) ?? c,
      );
    assert.equal(edgeCases[0].outcomes[0].source_ledger_total, 25000);
    assert.ok(
      edgeCases[0].outcomes[0].after.transactions.some(
        (t) => t.imported_id === "disabled-but-source-imports",
      ),
      "Source first sync overrides disabled import preference",
    );
    assert.equal(
      edgeCases[1].outcomes[0].source_ledger_total,
      9892,
      "Source one-decimal opening differs from its provider balance",
    );
    const renamed = edgeCases.find(
      (c) => c.id === "first-sync-renamed-starting-reference",
    );
    assert.equal(
      renamed.outcomes[0].after.transactions.find(
        (t) => t.starting_balance_flag,
      ).payee,
      renamed.ids.startingPayee,
      "Source reuses original case-insensitive starting payee identity",
    );
    assert.equal(
      renamed.outcomes[0].after.transactions.find(
        (t) => t.starting_balance_flag,
      ).category,
      renamed.ids.startingCategory,
      "Source prefers lowercased starting balances category over other income",
    );
    const payeePath = "packages/loot-core/src/server/accounts/payees.ts";
    const edges = {
      oracle: output.oracle,
      source_commit: output.source_commit,
      clock,
      api_bundle_sha256: output.api_bundle_sha256,
      source_files: {
        ...source_files,
        [payeePath]: hash(
          await readFile(
            new URL(
              "../reference/actual-26.10.0/" + payeePath,
              import.meta.url,
            ),
          ),
        ),
      },
      scope:
        "Bounded whole-source first-link edge observations; original six observations unchanged. Source divergences and original renamed reference identities are retained.",
      cases: edgeCases,
    };
    if (!captureEdges)
      assert.deepEqual(
        stable(edges),
        stable(JSON.parse(await readFile(manifest, "utf8"))),
      );
    else
      await writeFile(manifest, JSON.stringify(edges, null, 2) + "\n", {
        flag: priorEdges ? "w" : "wx",
      });
    console.log(
      "PASS three genuine first-link edge observations with source AQL totals and original references",
    );
  }
  console.log(
    "PASS six genuine core bank-sync workflows with repeat identities and original input",
  );
} finally {
  await api.shutdown();
  await new Promise((resolve) => http.close(resolve));
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}
