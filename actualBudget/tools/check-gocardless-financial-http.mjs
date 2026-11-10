// Genuine source financial graph → public provider/link/refresh seam.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Controlled provider proof owns disposable server",
);
const financial = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/gocardless-financial-source-26.10.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
const adapter = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/gocardless-source-26.10.json", import.meta.url),
    "utf8",
  ),
);
let current;
const provider = createServer((req, res) => {
  const row = adapter.observations.find(
      (c) => c.input.id === current.input.source,
    ),
    institution = current.transport_response.institutionId;
  const raw = row.requests.find((r) => r.path.includes("/transactions/"))
    .response.body;
  const data = (() => {
    if (req.url === "/api/v2/token/new/")
      return {
        access: "SYNTHETIC-ACCESS",
        refresh: "SYNTHETIC-REFRESH",
        access_expires: 86400,
        refresh_expires: 2592000,
      };
    assert.equal(req.headers.authorization, "Bearer SYNTHETIC-ACCESS");
    if (req.url === `/api/v2/institutions/${institution}/`)
      return { ...row.input.institution, id: institution };
    if (req.url === "/api/v2/agreements/enduser/") return { id: "agreement-A" };
    if (req.url === "/api/v2/requisitions/" && req.method === "POST")
      return {
        id: "requisition-A",
        link: "https://ob.gocardless.com/psd2/start/SYNTHETIC",
      };
    if (req.url === "/api/v2/requisitions/requisition-A/")
      return {
        id: "requisition-A",
        status: "LN",
        institution_id: institution,
        accounts: ["remote-A"],
      };
    if (req.url === "/api/v2/accounts/remote-A/details/")
      return {
        account: {
          name: "GoCardless source checking",
          iban: "DE0000000000001234",
          currency: "EUR",
          product: "Synthetic checking",
        },
      };
    if (req.url === "/api/v2/accounts/remote-A/")
      return {
        id: "remote-A",
        institution_id: institution,
        status: "READY",
        iban: "DE0000000000001234",
      };
    if (req.url === "/api/v2/accounts/remote-A/balances/")
      return { balances: row.input.balances };
    if (req.url.startsWith("/api/v2/accounts/remote-A/transactions/"))
      return raw;
    assert.fail("Unexpected provider path " + req.url);
  })();
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify(data));
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
process.env.ACTUALBUDGET_GOCARDLESS_TEST_ORIGIN =
  process.env.ACTUALBUDGET_BANK_TEST_ORIGIN;
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
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json();
  assert.equal(
    response.status,
    status,
    `${current.id} ${path}: ${JSON.stringify(data)}`,
  );
  if (path.endsWith("/banking/connections"))
    proofs.push(["BankConnectionCreate", body], ["BankConnection", data.data]);
  else if (path.includes("/accounts/") && path.endsWith("/banking/commands"))
    proofs.push(["BankAccountCommand", body], ["BankAccountResponse", data]);
  else if (
    path.includes("/banking/connections/") &&
    path.endsWith("/commands")
  ) {
    proofs.push(["BankConnectionCommand", body]);
    if (body.action === "refresh") proofs.push(["BankRefreshResponse", data]);
    else if (body.action === "discover")
      proofs.push(["BankDiscovery", data.data]);
    else proofs.push(["BankConnection", data.data]);
  }
  return data;
}
function nextClock(clock) {
  const count = parseInt(clock.slice(25, 29), 16) + 1;
  return `${new Date(Date.parse(clock.slice(0, 24)) + (count > 65535 ? 1 : 0)).toISOString()}-${(count > 65535 ? 0 : count).toString(16).toUpperCase().padStart(4, "0")}-2929292929292929`;
}
function projection(rows, payees, categories, source, before) {
  const names = new Map(payees.map((p) => [p.id, p.name])),
    originalPayees = new Set(before.payees.map((p) => p.id)),
    originalCategories = new Set(before.categories.map((c) => c.id)),
    originalRows = new Set(before.transactions.map((t) => t.id));
  return rows
    .map((t) => {
      const payee = t[source ? "payee" : "payee_id"],
        category = t[source ? "category" : "category_id"];
      const raw = t.raw_synced_data ? JSON.parse(t.raw_synced_data) : null;
      if (raw?.payee)
        raw.payee = originalPayees.has(raw.payee)
          ? raw.payee
          : (names.get(raw.payee) ??
            (/^[0-9a-f-]{36}$/.test(raw.payee)
              ? "unretained-generated-payee"
              : raw.payee));
      return {
        identity: originalRows.has(t.id)
          ? t.id
          : t[source ? "starting_balance_flag" : "starting_balance"]
            ? "starting"
            : t.imported_id,
        account: t[source ? "account" : "account_id"],
        amount: t.amount,
        date: t.date,
        notes: t.notes ?? "",
        cleared: !!t.cleared,
        reconciled: !!t.reconciled,
        parent: t.parent_id ?? null,
        transfer: t.transfer_id ?? null,
        is_parent: !!t.is_parent,
        is_child: source ? !!t.is_child : t.parent_id != null,
        starting: !!t[source ? "starting_balance_flag" : "starting_balance"],
        payee: originalPayees.has(payee)
          ? payee
          : (names.get(payee) ?? payee ?? null),
        category: originalCategories.has(category)
          ? category
          : (categories.find((c) => c.id === category)?.name ??
            category ??
            null),
        imported_id: t.imported_id ?? null,
        imported_payee: t.imported_payee ?? null,
        raw,
      };
    })
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
try {
  for (current of financial.cases) {
    const bytes = await readFile(
      new URL(
        "../tests/fixtures/gocardless-financial-source/" +
          current.archive.filename,
        import.meta.url,
      ),
    );
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      current.archive.sha256,
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
    const stage = (await upload.json()).data;
    assert.equal(stage.status, "ready");
    const active = (await call(`/imports/${stage.id}/activate`, {}, {}, 201))
      .data;
    const budget = await cloneImportedTemplate(api, active.id),
      root = `/budgets/${budget}`;
    const epoch = (await call(root + "/checkpoint")).data.epoch,
      headers = { "X-Actual-Budget-Epoch": epoch },
      owner = randomUUID();
    let account = (await call(root + "/accounts")).data[0]?.id;
    if (!account) {
      account = current.ids.account;
      await call(
        root + "/accounts",
        {
          id: account,
          name: "GoCardless source checking",
          opening_balance: 0,
          opening_date: "2026-11-01",
        },
        headers,
        201,
      );
    }
    assert.equal(account, current.ids.account);
    let connection = (
      await call(
        root + "/banking/connections",
        {
          id: randomUUID(),
          provider: "gocardless",
          name: "Synthetic institution",
          secret_id: "SYNTHETIC-ID",
          secret_key: "SYNTHETIC-KEY",
        },
        headers,
        201,
      )
    ).data;
    const path = root + `/banking/connections/${connection.id}/commands`;
    connection = (
      await call(
        path,
        {
          action: "authorize",
          generation: connection.generation,
          institution_id: current.transport_response.institutionId,
          country: "GB",
          redirect_origin: "http://actualbudget.test:45000",
        },
        headers,
      )
    ).data;
    connection = (
      await call(
        path,
        { action: "check_authorization", generation: connection.generation },
        headers,
      )
    ).data;
    assert.equal(connection.state, "connected");
    const discovered = await call(
      path,
      { action: "discover", generation: connection.generation },
      headers,
    );
    assert.equal(discovered.data.errors.length, 0, JSON.stringify(discovered));
    let bank = (await call(root + "/banking")).data;
    assert.equal(bank.accounts.length, 1);
    await call(
      root + `/accounts/${account}/banking/commands`,
      {
        action: "link",
        snapshot: bank.snapshot,
        connection_id: connection.id,
        generation: connection.generation,
        remote_account_id: bank.accounts[0].id,
        opening_balance_mode:
          current.input.opening === undefined ? "auto" : "custom",
        opening_date: "2026-11-01",
        ...(current.input.opening === undefined
          ? {}
          : { opening_amount: current.input.opening }),
      },
      headers,
    );
    bank = (await call(root + "/banking")).data;
    const body = {
        action: "refresh",
        generation: connection.generation,
        snapshot: bank.snapshot,
        start_date: "2026-11-01",
        end_date: "2026-11-10",
        accounts: [{ account_id: account, revision: bank.links[0].revision }],
      },
      key = randomUUID();
    const wire = {
      ...headers,
      "Idempotency-Key": key,
      "X-Actual-History-Owner": owner,
      "X-Actual-Edit-Clock": nextClock(bank.clock),
    };
    const result = await call(path, body, wire);
    assert.equal(result.data.status, "complete", JSON.stringify(result));
    const rows = (await call(root + "/transactions")).data,
      payees = (await call(root + "/payees")).data,
      categories = (await call(root + "/categories")).data;
    assert.deepEqual(
      projection(rows, payees, categories, false, current.before),
      projection(
        current.after.transactions,
        current.after.payees,
        current.after.categories,
        true,
        current.before,
      ),
      current.id + " complete source financial/reference/status/raw graph",
    );
    const firstIds = rows.map((t) => t.id).sort();
    assert.deepEqual(
      await call(path, body, { ...wire, "X-Actual-Receipt-Only": "true" }),
      result,
    );
    bank = (await call(root + "/banking")).data;
    const repeated = await call(
      path,
      {
        ...body,
        snapshot: bank.snapshot,
        accounts: [{ account_id: account, revision: bank.links[0].revision }],
      },
      {
        ...headers,
        "X-Actual-History-Owner": owner,
        "X-Actual-Edit-Clock": nextClock(bank.clock),
      },
    );
    assert.equal(repeated.data.status, "complete");
    assert.deepEqual(
      (await call(root + "/transactions")).data.map((t) => t.id).sort(),
      firstIds,
      "every original import identity survives repeat",
    );
    assert.deepEqual(
      projection(
        (await call(root + "/transactions")).data,
        (await call(root + "/payees")).data,
        categories,
        false,
        current.before,
      ),
      projection(
        current.repeat.transactions,
        current.repeat.payees,
        current.repeat.categories,
        true,
        current.before,
      ),
    );
    console.log("PASS GoCardless source financial " + current.id);
  }
  const contract = await (await fetch(api + "/openapi.json")).json();
  const verified = spawnSync(
    "uv",
    [
      "run",
      "--with",
      "jsonschema==4.23.0",
      "python",
      "-c",
      `import json,sys
from jsonschema import Draft202012Validator,FormatChecker
p=json.load(sys.stdin)
for name,value in p['proofs']:
 schema={**p['contract']['components']['schemas'][name],'components':p['contract']['components']}
 Draft202012Validator.check_schema(schema)
 Draft202012Validator(schema,format_checker=FormatChecker()).validate(value)
`,
    ],
    { input: JSON.stringify({ contract, proofs }), maxBuffer: 1024 * 1024 },
  );
  assert.equal(
    verified.status,
    0,
    verified.stdout.toString() + verified.stderr.toString(),
  );
  console.log(
    "PASS " +
      proofs.length +
      " served GoCardless financial contract observations",
  );
} finally {
  provider.closeAllConnections();
  await server.stop();
  await new Promise((resolve) => provider.close(resolve));
}
