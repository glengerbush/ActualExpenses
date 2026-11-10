// Financial expectations come only from the already executed pinned bank engine.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-sync-source-26.10.json", import.meta.url),
  ),
);
let current,
  round = 0;
const fetched = [];
const provider = createServer((req, res) => {
  if (req.method === "POST" && req.url.startsWith("/claim/")) {
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  const url = new URL(req.url, "http://fixture.invalid");
  if (url.pathname != "/access/accounts") {
    res.statusCode = 404;
    res.end();
    return;
  }
  fetched.push({
    case: current.id,
    query: Object.fromEntries(url.searchParams),
    authorization: req.headers.authorization,
  });
  const outcome = current.outcomes[round],
    input = outcome.input;
  const transactions = input.map((row) => ({
    id: row.transactionId,
    posted: row.booked
      ? Date.parse((row.postedDate ?? row.date) + "T00:00:00Z") / 1000
      : 0,
    transacted_at:
      Date.parse((row.transactedDate ?? row.date) + "T00:00:00Z") / 1000,
    pending: !row.booked,
    amount: row.transactionAmount.amount,
    payee: row.payeeName,
    description: row.notes,
    ...Object.fromEntries(
      Object.entries(row).filter(([key]) =>
        ["otherPayee", "otherNotes"].includes(key),
      ),
    ),
  }));
  const balance = outcome.transport_response.balances[0].balanceAmount;
  res.setHeader("Content-Type", "application/json");
  res.end(
    JSON.stringify({
      errors: [],
      accounts: [
        {
          id: "synthetic-remote-account",
          name: "Source checking",
          org: { id: "source-institution", name: "Synthetic institution" },
          currency: balance.currency,
          balance: balance.amount,
          "balance-date": Date.parse(fixture.clock) / 1000,
          transactions: url.searchParams.has("balances-only")
            ? []
            : transactions,
        },
        ...(url.searchParams.has("balances-only")
          ? [
              {
                id: "untouched-side-account",
                name: "Not selected",
                org: {
                  id: "source-institution",
                  name: "Synthetic institution",
                },
                currency: "USD",
                balance: "5.00",
                "balance-date": Date.parse(fixture.clock) / 1000,
              },
            ]
          : []),
      ],
    }),
  );
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${provider.address().port}`;
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = origin;
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
const proofs = [];
async function call(path, body, headers = {}, status = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const value = await response.json();
  assert.equal(response.status, status, JSON.stringify({ path, value }));
  if (status >= 400) proofs.push(["Error", value]);
  else if (path.includes("/accounts/") && path.endsWith("/banking/commands")) {
    proofs.push(["BankAccountCommand", body], ["BankAccountResponse", value]);
  } else if (body?.action === "refresh") {
    proofs.push(["BankRefresh", body], ["BankRefreshResponse", value]);
  } else if (
    body === undefined &&
    path.includes("/accounts/") &&
    path.endsWith("/banking")
  )
    proofs.push(["BankAccount", value.data]);
  else if (path.includes("/changes?")) proofs.push(["ChangePage", value]);

  return value;
}
function financial(rows, payees, categories, source) {
  const names = new Map(payees.map((p) => [p.id, p.name])),
    cats = new Map(categories.map((c) => [c.id, c.name]));
  return rows
    .map((t) => ({
      amount: t.amount,
      date: t.date,
      notes: t.notes ?? "",
      cleared: !!t.cleared,
      reconciled: !!t.reconciled,
      starting: !!t[source ? "starting_balance_flag" : "starting_balance"],
      payee: names.get(t[source ? "payee" : "payee_id"]) ?? null,
      category: cats.get(t[source ? "category" : "category_id"]) ?? null,
      category_id: t[source ? "category" : "category_id"] ?? null,
      retained_payee_id: current.before.payees.some(
        (p) => p.id === t[source ? "payee" : "payee_id"],
      )
        ? t[source ? "payee" : "payee_id"]
        : null,
      imported_id: t.imported_id ?? null,
      imported_payee: t.imported_payee ?? null,
      parent: t.parent_id ?? null,
      transfer: t.transfer_id ?? null,
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
try {
  for (const item of fixture.cases) {
    current = item;
    round = 0;
    const bytes = await readFile(
      new URL(
        "../tests/fixtures/bank-sync-source/" + item.archive.filename,
        import.meta.url,
      ),
    );
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      item.archive.sha256,
    );
    const upload = await fetch(api + "/imports/actual", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: bytes,
    });
    assert.equal(upload.status, 201, await upload.clone().text());
    const staged = (await upload.json()).data;
    const activated = await call(
        "/imports/" + staged.id + "/activate",
        {},
        {},
        201,
      ),
      budget =
        activated.data.id ??
        activated.data.budget_id ??
        activated.data.budget?.id;
    assert.ok(budget);
    const root = "/budgets/" + budget,
      account = item.ids.account;
    if (!item.before.accounts.some((a) => a.id === account))
      await call(
        root + "/accounts",
        {
          id: account,
          name: "Source checking",
          off_budget: false,
          opening_balance: 0,
          opening_date: "2026-11-01",
        },
        {},
        201,
      );
    const initialCheckpoint = (await call(root + "/checkpoint")).data;
    const epoch = initialCheckpoint.epoch,
      headers = {
        "X-Actual-Budget-Epoch": epoch,
        "X-Actual-History-Owner": randomUUID(),
      };
    const id = randomUUID(),
      stagedConnection = await call(
        root + "/banking/connections",
        {
          id,
          provider: "simplefin",
          name: "Source fixture",
          setup_token: Buffer.from(origin + "/claim/" + id).toString("base64"),
        },
        headers,
        201,
      );
    const claimed = await call(
      root + "/banking/connections/" + id + "/commands",
      { action: "claim", generation: stagedConnection.data.generation },
      headers,
    );
    const discovered = await call(
      root + "/banking/connections/" + id + "/commands",
      { action: "discover", generation: claimed.data.generation },
      headers,
    );
    const cp = (await call(root + "/checkpoint")).data;
    const linked = await call(
      root + "/accounts/" + account + "/banking/commands",
      {
        action: "link",
        snapshot: cp.snapshot,
        connection_id: id,
        generation: claimed.data.generation,
        remote_account_id: discovered.data.accounts[0].id,
        opening_balance_mode: "auto",
      },
      headers,
    );
    let firstIds, originalCommand, originalHeaders, originalReceipt;
    for (round = 0; round < item.outcomes.length; round++) {
      const expected = item.outcomes[round].after,
        cp = (await call(root + "/checkpoint")).data;
      const command = {
        action: "refresh",
        generation: claimed.data.generation,
        snapshot: cp.snapshot,
        start_date: "2026-11-01",
        end_date: "2026-11-10",
        accounts: [{ account_id: account, revision: linked.data.revision }],
      };
      const requestHeaders = { ...headers, "Idempotency-Key": randomUUID() };
      const result = await call(
        root + "/banking/connections/" + id + "/commands",
        command,
        requestHeaders,
      );
      if (round === 0) {
        originalCommand = command;
        originalHeaders = requestHeaders;
        originalReceipt = result;
      }
      assert.equal(
        (await call(root + "/banking")).data.accounts.find(
          (a) => a.remote_id === "untouched-side-account",
        ).deleted,
        false,
        "Scoped refresh never tombstones an unrequested provider account",
      );
      const rows = (await call(root + "/transactions")).data,
        payees = (await call(root + "/payees")).data,
        categories = (await call(root + "/categories")).data;
      assert.deepEqual(
        financial(rows, payees, categories, false),
        financial(
          expected.transactions,
          expected.payees,
          expected.categories,
          true,
        ),
        item.id + " round " + round,
      );
      assert.deepEqual(
        payees.map((p) => p.name).sort(),
        expected.payees
          .filter((p) => !p.transfer_acct)
          .map((p) => p.name)
          .sort(),
        "No unused or duplicate provisional payees persist",
      );
      const bankAccount = (await call(root + "/banking")).data.accounts.find(
        (a) => a.remote_id === "synthetic-remote-account",
      );
      assert.equal(
        bankAccount.balance,
        item.outcomes[round].transport_response.balances[0].balanceAmount
          .amount,
      );
      assert.equal(
        bankAccount.currency,
        item.outcomes[round].transport_response.balances[0].balanceAmount
          .currency,
      );
      assert.ok(
        rows.every((r) => r.account_id === account),
        "Bank rows retain their captured local account",
      );
      const ids = rows.map((r) => r.id).sort();
      if (round === 0) firstIds = ids;
      for (const prior of item.before.transactions) {
        if (expected.transactions.some((t) => t.id === prior.id))
          assert.ok(
            ids.includes(prior.id),
            "Original manual identity survives bank matching",
          );
      }
      const count = fetched.length;
      assert.deepEqual(
        await call(
          root + "/banking/connections/" + id + "/commands",
          command,
          requestHeaders,
        ),
        result,
      );
      assert.equal(fetched.length, count);
      const repeat = {
        ...command,
        snapshot: (await call(root + "/checkpoint")).data.snapshot,
      };
      await call(
        root + "/banking/connections/" + id + "/commands",
        repeat,
        headers,
      );
      assert.deepEqual(
        (await call(root + "/transactions")).data.map((r) => r.id).sort(),
        ids,
        "Repeat preserves exact financial identities",
      );
      if (round > 0)
        assert.ok(
          firstIds.every((id) => ids.includes(id)),
          "Pending→booked never replaces an existing source identity",
        );
    }
    if (item.id === "first-sync-opening") {
      const history = (await call(root + "/history", undefined, headers)).data;
      assert.ok(history.undo?.id);
      await call(
        root + "/history/undo",
        { action_id: history.undo.id },
        headers,
      );
      assert.deepEqual(
        (await call(root + "/transactions")).data,
        [],
        "Owned bank undo includes opening, expenses and provider metadata",
      );
      const cp = (await call(root + "/checkpoint")).data;
      assert.deepEqual(
        await call(
          root + "/banking/connections/" + id + "/commands",
          originalCommand,
          originalHeaders,
        ),
        originalReceipt,
        "Old receipt cannot reapply an undone bank import",
      );
      assert.deepEqual((await call(root + "/checkpoint")).data, cp);
      const redo = (await call(root + "/history", undefined, headers)).data
        .redo;
      assert.ok(redo?.id);
      await call(root + "/history/redo", { action_id: redo.id }, headers);
      assert.deepEqual(
        (await call(root + "/transactions")).data.map((r) => r.id).sort(),
        firstIds,
        "Redo restores exact bank financial identities",
      );
      const prior = (await call(root + "/accounts/" + account + "/banking"))
        .data;
      const newerClock =
          new Date(Date.now() + 1000).toISOString() + "-0000-0000000000000028",
        olderClock = new Date().toISOString() + "-0000-0000000000000028";
      const newerHeaders = {
        ...headers,
        "Idempotency-Key": randomUUID(),
        "X-Actual-Edit-Clock": newerClock,
      };
      const setting = {
        action: "settings",
        snapshot: prior.snapshot,
        revision: prior.link.revision,
        changes: { import_notes: false },
      };
      const saved = await call(
        root + "/accounts/" + account + "/banking/commands",
        setting,
        newerHeaders,
      );
      const protectedSettings = (
        await call(root + "/accounts/" + account + "/banking")
      ).data;
      await call(
        root + "/accounts/" + account + "/banking/commands",
        {
          ...setting,
          snapshot: protectedSettings.snapshot,
          revision: saved.data.revision,
          changes: { import_notes: true },
        },
        { ...headers, "X-Actual-Edit-Clock": olderClock },
        409,
      );
      assert.deepEqual(
        (await call(root + "/accounts/" + account + "/banking")).data,
        protectedSettings,
        "Older clock cannot replace a newer preference even with a fresh snapshot",
      );
      assert.deepEqual(
        await call(
          root + "/accounts/" + account + "/banking/commands",
          setting,
          newerHeaders,
        ),
        saved,
      );
    }
    await call(
      root +
        "/changes?cursor=" +
        encodeURIComponent(initialCheckpoint.snapshot),
    );
    console.log("PASS source bank financial", item.id);
  }
  const contract = await (await fetch(api + "/openapi.json")).json();
  const validated = spawnSync(
    "uv",
    [
      "run",
      "--with",
      "jsonschema==4.23.0",
      "python",
      "-c",
      `import sys,json
from jsonschema import Draft202012Validator,FormatChecker
p=json.load(sys.stdin);c=p['contract']
try:
 for name,value in p['proofs']:
  schema={**c['components']['schemas'][name],'components':c['components']}
  Draft202012Validator.check_schema(schema);Draft202012Validator(schema,format_checker=FormatChecker()).validate(value)
except Exception as e:
 print(name,type(e).__name__,list(getattr(e,'path',[])));sys.exit(1)
`,
    ],
    { input: JSON.stringify({ contract, proofs }), maxBuffer: 1024 * 1024 },
  );
  assert.equal(
    validated.status,
    0,
    validated.stdout.toString() + validated.stderr.toString(),
  );
  console.log(
    "PASS served bank financial/settings/feed contract",
    proofs.length,
    "observations",
  );
} finally {
  await server.stop();
  await new Promise((resolve) => provider.close(resolve));
}
