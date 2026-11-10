// Explicit relink of a genuine linked source export, through registered handlers.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createServer } from "node:http";
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
const hash = (v) => createHash("sha256").update(v).digest("hex"),
  plain = (v) => JSON.parse(JSON.stringify(v));
const prior = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/gocardless-financial-source-26.10.json",
      import.meta.url,
    ),
    "utf8",
  ),
).cases[0];
const bytes = await readFile(
  new URL(
    "../tests/fixtures/gocardless-financial-source/" + prior.linked.filename,
    import.meta.url,
  ),
);
assert.equal(hash(bytes), prior.linked.sha256);
const requests = [];
const http = createServer(async (req, res) => {
  assert.equal(req.url, "/gocardless/transactions");
  assert.equal(req.method, "POST");
  assert.equal(req.headers["x-actual-token"], "synthetic-bank-source-token");
  let body = "";
  for await (const part of req) body += part;
  requests.push({ path: req.url, body: JSON.parse(body) });
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify({ status: "ok", data: prior.transport_response }));
});
await new Promise((resolve) => http.listen(0, "127.0.0.1", resolve));
const directory = await mkdtemp(tmpdir() + "/gocardless-renewal-source-");
try {
  const engine = await api.init({ dataDir: directory });
  await api.importBudget(bytes, { filename: prior.linked.filename });
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
    account = before.accounts.find(
      (a) => a.account_sync_source === "goCardless",
    );
  assert.ok(account && before.transactions.length > 0);
  await engine.send("set-server-url", {
    url: `http://127.0.0.1:${http.address().port}`,
    validate: false,
  });
  await engine.send("subscribe-set-token", {
    token: "synthetic-bank-source-token",
  });
  const input = {
    requisitionId: "renewed-requisition-B",
    account: {
      account_id: "renewed-remote-B",
      institution: {
        id: prior.transport_response.institutionId,
        name: "Synthetic institution",
      },
      name: account.name,
      mask: "1234",
      official_name: "Synthetic checking",
    },
    upgradingId: account.id,
    startingDate: "2026-11-01",
  };
  const result = await engine.send("gocardless-accounts-link", input);
  assert.equal(result, "ok");
  const after = await snapshot();
  assert.deepEqual(
    after.transactions,
    before.transactions,
    "Explicit renewed provider IDs never recreate or alter existing source purchases",
  );
  assert.equal(
    after.accounts.find((a) => a.id === account.id).account_id,
    "renewed-remote-B",
  );
  const repeated = await engine.send("accounts-bank-sync", {
    ids: [account.id],
  });
  assert.deepEqual(repeated.errors, []);
  const repeat = await snapshot();
  assert.deepEqual(repeat.transactions, before.transactions);
  assert.equal(requests.length, 2);
  for (const request of requests) {
    assert.equal(request.body.requisitionId, input.requisitionId);
    assert.equal(request.body.accountId, input.account.account_id);
    assert.equal(request.body.includeBalance, false);
  }
  const output = {
    sourceVersion: "26.10.0",
    sourceCommit: "2bebdbaae0b701120a8cad9d56aafd64fa04c577",
    clock,
    apiBundleSha256: hash(await readFile(require.resolve("@actual-app/api"))),
    archive: {
      path: "gocardless-financial-source/" + prior.linked.filename,
      sha256: hash(bytes),
    },
    scope:
      "Whole unchanged registered explicit GoCardless relink plus repeat using an immutable linked export. Every pre-existing source UUID and financial field remains literal; only genuinely new metadata UUIDs normalize for independent check.",
    before,
    input,
    transport_response: prior.transport_response,
    requests,
    result,
    after,
    repeated,
    repeat,
  };
  const original = new Set();
  const collect = (v) => {
    if (typeof v === "string")
      for (const m of v.matchAll(
        /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,
      ))
        original.add(m[0]);
    else if (Array.isArray(v)) v.forEach(collect);
    else if (v && typeof v === "object")
      for (const [k, x] of Object.entries(v)) {
        collect(k);
        collect(x);
      }
  };
  collect(before);
  collect(input);
  collect(prior.transport_response);
  const stable = (value) => {
    const ids = new Map();
    const id = (v) => {
      if (original.has(v)) return v;
      if (!ids.has(v)) ids.set(v, `new-metadata-${ids.size}`);
      return ids.get(v);
    };
    const walk = (v) =>
      typeof v === "string"
        ? v.replace(
            /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,
            id,
          )
        : Array.isArray(v)
          ? v.map(walk)
          : v && typeof v === "object"
            ? Object.fromEntries(
                Object.entries(v).map(([k, x]) => [k, walk(x)]),
              )
            : v;
    return walk(value);
  };
  const destination = new URL(
    "../tests/fixtures/gocardless-renewal-source-26.10.json",
    import.meta.url,
  );
  if (process.argv.includes("--check"))
    assert.deepEqual(
      stable(output),
      stable(JSON.parse(await readFile(destination, "utf8"))),
    );
  else
    await writeFile(destination, JSON.stringify(output, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine explicit GoCardless renewed account/requisition IDs: all original financial rows/IDs unchanged, repeat exact",
  );
} finally {
  await api.shutdown();
  http.closeAllConnections();
  await new Promise((resolve) => http.close(resolve));
  await rm(directory, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}
