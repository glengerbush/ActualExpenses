// Execute the unchanged pinned server's entire helper tail. Only HTTP and SSRF
// destinations are disposable loopback fixtures; no replacement normalizer.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
process.env.TZ = "UTC";
const sourceUrl = new URL(
  "../reference/actual-26.10.0/packages/sync-server/src/app-simplefin/app-simplefin.js",
  import.meta.url,
);
const source = await readFile(sourceUrl, "utf8"),
  tail = source.slice(source.indexOf("function logAccountError("));
assert.ok(tail.startsWith("function logAccountError("));
const hash = (v) => createHash("sha256").update(v).digest("hex");
const requests = [];
let mode = "normal";
const raw = {
  errors: [],
  accounts: [
    {
      id: "remote-A",
      name: "Synthetic checking",
      currency: "USD",
      balance: "100.50",
      "available-balance": "90.25",
      "balance-date": 1794225600,
      org: { id: "org-A", name: "Synthetic institution" },
      transactions: [
        {
          id: "booked-A",
          posted: 1793664000,
          transacted_at: 1793577600,
          amount: "-12.34",
          payee: "CAFE",
          description: "Bank #note",
        },
        {
          id: "pending-A",
          posted: 0,
          transacted_at: 1793750400,
          pending: true,
          amount: "-2.00",
          payee: "SHOP",
          description: "Pending",
        },
      ],
    },
  ],
};
let secondaryOrigin;
const secondary = createServer((req, res) => {
  requests.push({
    target: "secondary",
    method: req.method,
    path: req.url,
    authorization: req.headers.authorization ?? null,
  });
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(raw));
});
await new Promise((r) => secondary.listen(0, "127.0.0.1", r));
secondaryOrigin = `http://127.0.0.1:${secondary.address().port}`;
const primary = createServer((req, res) => {
  requests.push({
    target: "primary",
    method: req.method,
    path: req.url,
    authorization: req.headers.authorization ?? null,
  });
  if (req.url === "/claim") {
    if (mode === "claim403") {
      res.statusCode = 403;
      res.end("Forbidden");
      return;
    }
    if (mode === "claimRedirect") {
      res.statusCode = 302;
      res.setHeader("Location", secondaryOrigin + "/claim");
      res.end("Redirect refused by source helper");
      return;
    }
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${primary.address().port}/access`,
    );
    return;
  }
  if (mode === "crossOrigin") {
    res.statusCode = 302;
    res.setHeader("Location", secondaryOrigin + "/accounts");
    res.end();
    return;
  }
  const result = structuredClone(raw);
  if (mode === "attention")
    result.errors = ["Connection to Synthetic institution may need attention"];
  if (mode === "postedZero") delete result.accounts[0].transactions[1].pending;
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(result));
});
await new Promise((r) => primary.listen(0, "127.0.0.1", r));
const origin = `http://127.0.0.1:${primary.address().port}`;
const allow = async (raw) => {
  assert.ok(
    [origin, secondaryOrigin].includes(new URL(raw).origin),
    "Source may only contact fixture transports",
  );
};
const functions = new Function(
  "assertUrlAllowed",
  "fetch",
  "console",
  tail +
    "\nreturn {getAccounts,getAccountResponse,claimAccessKey,decodeClaimUrl,isInvalidAccessKey};",
)(allow, fetch, { log() {} });
const fixture = new URL(
    "../tests/fixtures/simplefin-transport-source-26.10.json",
    import.meta.url,
  ),
  observations = [];
try {
  for (const scenario of [
    "normal",
    "postedZero",
    "attention",
    "claim403",
    "claimRedirect",
    "crossOrigin",
  ]) {
    mode = scenario;
    requests.length = 0;
    let result;
    if (mode.startsWith("claim")) {
      try {
        const access = await functions.claimAccessKey(origin + "/claim");
        result = {
          access: access.replaceAll(origin, "PRIMARY"),
          invalid: functions.isInvalidAccessKey(access),
        };
      } catch (error) {
        result = { error: error.message };
      }
    } else {
      const response = await functions.getAccounts(
        `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${primary.address().port}/access`,
        ["remote-A"],
        new Date("2026-11-01T00:00:00Z"),
        new Date("2026-11-10T00:00:00Z"),
      );
      result = {
        raw: structuredClone(response),
        normalized: functions.getAccountResponse(
          response,
          "remote-A",
          new Date("2026-11-01T00:00:00Z"),
        ),
        errors: response.errors,
        hasError: response.hasError,
      };
    }
    observations.push({
      scenario,
      input: structuredClone(raw),
      requests: structuredClone(requests),
      result,
    });
  }
  assert.equal(observations[0].result.normalized.startingBalance, 10050);
  assert.equal(
    observations[0].result.normalized.transactions.booked[0].transactionAmount
      .amount,
    "-12.34",
  );
  assert.equal(
    observations[1].result.normalized.transactions.pending[0].transactionId,
    "pending-A",
  );
  assert.equal(observations[2].result.hasError, true);
  assert.equal(observations[3].result.invalid, true);
  assert.equal(
    observations[4].requests.length,
    1,
    "Claim redirects are never followed",
  );
  assert.equal(
    observations[5].requests[1].authorization,
    null,
    "Cross-origin redirect strips credentials",
  );
  const output = JSON.parse(
    JSON.stringify({
      reference: "Actual 26.10.0 (pinned source tree)",
      source_path: "packages/sync-server/src/app-simplefin/app-simplefin.js",
      source_sha256: hash(source),
      executed_helper_tail_sha256: hash(tail),
      scope:
        "Unchanged server helper tail with loopback-only transport. Does not qualify production TLS, limits, v2 structured errors or live access.",
      normalization:
        "Ordinary JSON serialization only: undefined helper properties are absent on the source HTTP wire; no financial or identity changes.",
      observations,
    }),
  );
  if (process.argv.includes("--check"))
    assert.deepEqual(output, JSON.parse(await readFile(fixture, "utf8")));
  else await writeFile(fixture, JSON.stringify(output, null, 2) + "\n");
  console.log(
    "PASS six pinned raw SimpleFIN transport/normalization witnesses",
  );
} finally {
  await Promise.all([
    new Promise((r) => primary.close(r)),
    new Promise((r) => secondary.close(r)),
  ]);
}
