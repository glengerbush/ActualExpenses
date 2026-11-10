// Missing-data scope is tied to retained provider identities, never diagnostic text.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
let partial = false;
const provider = createServer((req, res) => {
  if (req.method === "POST") {
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  const balancesOnly = new URL(
    req.url,
    "http://fixture.invalid",
  ).searchParams.has("balances-only");
  res.setHeader("Content-Type", "application/json");
  res.end(
    JSON.stringify({
      errlist: partial
        ? [
            {
              code: "act.missingdata",
              account_id: "account-1",
              conn_id: "institution",
              msg: "PRIVATE DIAGNOSTIC NEVER PUBLISH",
            },
          ]
        : [],
      accounts: [1, 2]
        .filter((n) => !partial || n === 2)
        .map((n) => ({
          id: `account-${n}`,
          conn_id: "institution",
          org: { name: "Controlled institution" },
          name: `Source ${n}`,
          currency: "USD",
          balance: `${n}.00`,
          "balance-date": 1794225600,
          transactions: balancesOnly
            ? []
            : [
                {
                  id: `purchase-${n}`,
                  posted: 1793491200,
                  amount: `-${n}.00`,
                  payee: "Source shop",
                  description: "Bank note",
                },
              ],
        })),
    }),
  );
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, headers = {}, status = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await response.json();
  assert.equal(response.status, status, JSON.stringify(result));
  assert.ok(!JSON.stringify(result).includes("PRIVATE DIAGNOSTIC"));
  return result;
}
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`;
  await call(
    "/budgets",
    { id: budget, name: "Partial provider isolation", budget_type: "envelope" },
    { "Idempotency-Key": randomUUID() },
    201,
  );
  const epoch = (await call(root + "/checkpoint")).data.epoch,
    headers = { "X-Actual-Budget-Epoch": epoch };
  let connection = (
    await call(
      root + "/banking/connections",
      {
        id: randomUUID(),
        provider: "simplefin",
        name: "Controlled provider",
        setup_token: Buffer.from(
          process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim",
        ).toString("base64"),
      },
      { ...headers, "Idempotency-Key": randomUUID() },
      201,
    )
  ).data;
  const path = root + `/banking/connections/${connection.id}/commands`;
  connection = (
    await call(
      path,
      { action: "claim", generation: connection.generation },
      { ...headers, "Idempotency-Key": randomUUID() },
    )
  ).data;
  await call(
    path,
    { action: "discover", generation: connection.generation },
    { ...headers, "Idempotency-Key": randomUUID() },
  );
  const ids = [];
  for (let n = 1; n <= 2; n++) {
    const account = randomUUID();
    ids.push(account);
    await call(
      root + "/accounts",
      {
        id: account,
        name: `Local ${n}`,
        off_budget: false,
        opening_balance: 0,
        opening_date: "2026-11-01",
      },
      { ...headers, "Idempotency-Key": randomUUID() },
      201,
    );
    const state = (await call(root + "/banking")).data;
    await call(
      root + `/accounts/${account}/banking/commands`,
      {
        action: "link",
        snapshot: state.snapshot,
        connection_id: connection.id,
        generation: connection.generation,
        remote_account_id: state.accounts.find(
          (r) => r.remote_id === `account-${n}`,
        ).id,
        opening_balance_mode: "none",
      },
      { ...headers, "Idempotency-Key": randomUUID() },
    );
  }
  partial = true;
  const discovery = await call(
    path,
    { action: "discover", generation: connection.generation },
    { ...headers, "Idempotency-Key": randomUUID() },
  );
  assert.equal(
    discovery.data.errors[0].remote_id,
    "account-1",
    "Missing raw account retains its known scoped error identity",
  );
  assert.deepEqual(
    discovery.data.accounts[0].diagnostics,
    [],
    "Unrelated returned account remains usable",
  );
  async function refresh() {
    const state = (await call(root + "/banking")).data;
    return call(
      path,
      {
        action: "refresh",
        generation: connection.generation,
        snapshot: state.snapshot,
        start_date: "2026-11-01",
        end_date: "2026-11-10",
        accounts: ids.map((id) => ({
          account_id: id,
          revision: state.links.find((l) => l.id === id).revision,
        })),
      },
      {
        ...headers,
        "Idempotency-Key": randomUUID(),
        "X-Actual-History-Owner": randomUUID(),
      },
    );
  }
  await refresh();
  const after = (await call(root + "/transactions")).data;
  assert.equal(after.length, 1);
  assert.equal(after[0].account_id, ids[1]);
  assert.equal(after[0].amount, -200);
  let state = (await call(root + "/banking")).data;
  assert.equal(
    state.accounts.find((r) => r.remote_id === "account-1").deleted,
    false,
    "Incomplete financial response cannot tombstone cached account",
  );
  assert.equal(
    state.links.find((l) => l.id === ids[0]).last_sync_end,
    null,
    "Failed scope cannot advance success window",
  );
  assert.equal(
    state.links.find((l) => l.id === ids[1]).last_sync_end,
    "2026-11-10",
  );
  partial = false;
  await refresh();
  const final = (await call(root + "/transactions")).data;
  assert.equal(final.length, 2);
  assert.ok(final.some((t) => t.id === after[0].id));
  assert.deepEqual(
    final.map((t) => t.amount).sort((a, b) => a - b),
    [-200, -100],
  );
  const deleted = await fetch(api + root + `/accounts/${ids[0]}`, {
    method: "DELETE",
    headers: {
      "Content-Type": "application/json",
      ...headers,
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify({ confirm_name: "Local 1" }),
  });
  assert.equal(deleted.status, 200, await deleted.text());
  const replacement = randomUUID();
  await call(
    root + "/accounts",
    {
      id: replacement,
      name: "Replacement local",
      off_budget: false,
      opening_balance: 0,
      opening_date: "2026-11-01",
    },
    { ...headers, "Idempotency-Key": randomUUID() },
    201,
  );
  const captured = (await call(root + "/banking")).data,
    oldLink = captured.links.find((l) => l.id === ids[0]),
    remote = captured.accounts.find((r) => r.id === oldLink.remote_account_id);
  const ledger = await call(root + "/transactions");
  await call(
    root + `/accounts/${replacement}/banking/commands`,
    {
      action: "link",
      snapshot: captured.snapshot,
      connection_id: connection.id,
      generation: connection.generation,
      remote_account_id: remote.id,
      opening_balance_mode: "none",
    },
    { ...headers, "Idempotency-Key": randomUUID() },
    409,
  );
  const unlink = {
      action: "unlink",
      snapshot: captured.snapshot,
      revision: oldLink.revision,
    },
    unlinkHeaders = { ...headers, "Idempotency-Key": randomUUID() };
  const unlinked = await call(
    root + `/accounts/${ids[0]}/banking/commands`,
    unlink,
    unlinkHeaders,
  );
  assert.equal(unlinked.data.deleted, true);
  const afterUnlink = await call(root + "/transactions");
  delete ledger.snapshot;
  delete afterUnlink.snapshot;
  assert.deepEqual(
    afterUnlink,
    ledger,
    "Unlinking unavailable metadata cannot alter any financial/reference/version field",
  );
  const changes = await call(
    root + "/changes?cursor=" + encodeURIComponent(captured.snapshot),
  );
  assert.ok(changes.data.length > 0);
  assert.ok(changes.data.every((c) => c.entity === "bank_links"));
  assert.deepEqual(
    await call(
      root + `/accounts/${ids[0]}/banking/commands`,
      unlink,
      unlinkHeaders,
    ),
    unlinked,
  );
  const current = (await call(root + "/banking")).data;
  await call(
    root + `/accounts/${replacement}/banking/commands`,
    {
      action: "link",
      snapshot: current.snapshot,
      connection_id: connection.id,
      generation: connection.generation,
      remote_account_id: remote.id,
      opening_balance_mode: "none",
    },
    { ...headers, "Idempotency-Key": randomUUID() },
  );
  assert.equal(
    (await call(root + "/banking")).data.links.find((l) => l.id === replacement)
      .deleted,
    false,
  );
  console.log(
    "PASS omitted failed account retains cache/window; unaffected scope imports once and recovery preserves IDs; deleted local mapping can be explicitly unlinked/reused",
  );
} finally {
  await server.stop();
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
