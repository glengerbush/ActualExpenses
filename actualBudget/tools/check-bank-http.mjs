// Public lifecycle tracer. No internal database reads or real provider traffic.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
const directory = await mkdtemp(tmpdir() + "/bank-http-");
const binary =
  process.env.ACTUALBUDGET_BANK_BINARY ??
  fileURLToPath(
    new URL("../server/target/debug/actualbudget-server", import.meta.url),
  );
let claimCalls = 0,
  downloadCalls = 0,
  mode = "normal";
const claimedTokens = new Set();
let heldResponse, requestObserved;
const provider = createServer((req, res) => {
  if (req.url.startsWith("/access/accounts")) {
    downloadCalls++;
    if (mode === "held-large") {
      heldResponse = res;
      requestObserved?.();
      return;
    }
    if (mode === "normal-large") {
      res.setHeader("Content-Type", "application/json");
      res.end(
        JSON.stringify({ accounts: [], padding: "x".repeat(2 * 1024 * 1024) }),
      );
      return;
    }

    if (mode === "rate") {
      res.statusCode = 429;
      res.end("secret " + origin);
      return;
    }
    if (mode === "revoked") {
      res.statusCode = 403;
      res.end("secret " + origin);
      return;
    }
    if (mode === "bad-json") {
      res.end("not JSON secret " + origin);
      return;
    }
    if (mode === "large") {
      res.end("x".repeat(16 * 1024 * 1024 + 1));
      return;
    }
    if (mode === "continuation") {
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify({ accounts: [], next_cursor: "unexpected" }));
      return;
    }
    assert.equal(
      req.headers.authorization,
      "Basic " + Buffer.from("synthetic:credential").toString("base64"),
    );
    const url = new URL(req.url, "http://fixture.invalid");
    assert.equal(url.searchParams.get("balances-only"), "1");
    res.setHeader("Content-Type", "application/json");
    res.end(
      JSON.stringify({
        errlist:
          mode === "partial"
            ? [
                {
                  code: "act.missingdata",
                  msg: "secret " + origin,
                  account_id: "remote-A",
                  conn_id: "institution-A",
                },
              ]
            : [],
        errors: mode === "legacy" ? ["<b>secret " + origin + "</b>"] : [],
        connections: [
          { conn_id: "institution-A", name: "Bank A" },
          { conn_id: "institution-B", name: "Bank B" },
        ],
        accounts: [
          {
            id: "remote-A",
            conn_id: "institution-A",
            name: "Checking",
            currency: "USD",
            balance: "100.50",
            "available-balance": "90.25",
            "balance-date": 1794225600,
          },
          {
            id: "remote-A",
            conn_id: "institution-B",
            name: "Savings",
            currency: "USD",
            balance: "200.00",
            "balance-date": 1794225600,
          },
        ],
      }),
    );
    return;
  }

  if (req.url === "/uncertain" && req.method === "POST") {
    claimCalls++;
    req.socket.destroy();
    return;
  }
  if (req.url.startsWith("/claim/") && req.method === "POST") {
    if (claimedTokens.has(req.url)) {
      res.statusCode = 403;
      res.end("Forbidden");
      return;
    }
    claimedTokens.add(req.url);
    claimCalls++;
    res.end(
      `http://synthetic:credential@127.0.0.1:${provider.address().port}/access`,
    );
  } else {
    res.statusCode = 404;
    res.end();
  }
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${provider.address().port}`;
const options = {
  env: {
    ...process.env,
    ACTUALBUDGET_BANK_TEST_ORIGIN: origin,
    ACTUALBUDGET_DATA_DIR: directory,
    ACTUALBUDGET_HOST: "127.0.0.1",
    ACTUALBUDGET_PORT: "0",
    ACTUALBUDGET_WEB_DIR: directory,
  },
  stdio: ["ignore", "pipe", "pipe"],
};
let server,
  stderr = "";
async function start() {
  server = spawn(binary, [], options);
  server.stderr.on("data", (b) => (stderr += b));
  return await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("Disposable server did not start")),
      15000,
    );
    server.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    server.stdout.on("data", (b) => {
      const match = String(b).match(/LISTENING (127\.0\.0\.1:\d+)/);
      if (match) {
        clearTimeout(timer);
        resolve("http://" + match[1] + "/api/v1");
      }
    });
  });
}
let base = await start();

try {
  const call = async (
    path,
    body,
    key = randomUUID(),
    headers = {},
    status = 200,
  ) => {
    const r = await fetch(base + path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": key,
        ...headers,
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const result = await r.json();
    assert.equal(r.status, status, JSON.stringify({ path, result }));
    return result;
  };
  const budget = randomUUID();
  await call(
    "/budgets",
    { id: budget, name: "Bank lifecycle", budget_type: "tracking" },
    "budget",
    {},
    201,
  );
  const root = "/budgets/" + budget;
  const state = (await call(root + "/checkpoint")).data,
    id = randomUUID(),
    token = Buffer.from(origin + "/claim/initial").toString("base64");
  const checkpoint = state.snapshot;
  const body = {
      id,
      provider: "simplefin",
      name: "Synthetic connection",
      setup_token: token,
    },
    headers = { "X-Actual-Budget-Epoch": state.epoch };
  const created = await call(
    root + "/banking/connections",
    body,
    "create",
    headers,
    201,
  );
  assert.equal(created.data.id, id);
  assert.equal(created.data.state, "claim_pending");
  assert.deepEqual(
    await call(root + "/banking/connections", body, "create", headers, 201),
    created,
  );
  const publicState = await call(root + "/banking");
  assert.equal(publicState.data.connections.length, 1);
  assert.equal(publicState.data.connections[0].id, id);
  for (const forbidden of [token, "NOT-A-REAL-TOKEN", "synthetic.invalid"])
    assert.ok(
      !JSON.stringify({ created, publicState }).includes(forbidden),
      "Public state/receipt may not reveal setup secret",
    );
  const command = { action: "claim", generation: created.data.generation };
  const claimed = await call(
    root + "/banking/connections/" + id + "/commands",
    command,
    "claim",
    headers,
  );
  assert.equal(claimed.data.state, "connected");
  assert.equal(claimCalls, 1);
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + id + "/commands",
      command,
      "claim",
      headers,
    ),
    claimed,
  );
  assert.equal(
    claimCalls,
    1,
    "Committed lost ACK never consumes one-use token twice",
  );
  for (const secret of [token, "synthetic:credential", origin])
    assert.ok(!JSON.stringify(claimed).includes(secret));
  const discover = { action: "discover", generation: claimed.data.generation };
  const discovered = await call(
    root + "/banking/connections/" + id + "/commands",
    discover,
    "discover",
    headers,
  );
  assert.equal(discovered.data.accounts.length, 2);
  assert.equal(
    new Set(discovered.data.accounts.map((a) => a.id)).size,
    2,
    "Remote IDs are scoped to institution connection",
  );
  assert.equal(discovered.data.accounts[0].balance, "100.50");
  assert.equal(discovered.data.accounts[0].available_balance, "90.25");
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + id + "/commands",
      discover,
      "discover",
      headers,
    ),
    discovered,
  );
  assert.equal(downloadCalls, 1);
  const ambiguousId = randomUUID(),
    ambiguousToken = Buffer.from(origin + "/uncertain").toString("base64");
  const ambiguous = await call(
    root + "/banking/connections",
    { ...body, id: ambiguousId, setup_token: ambiguousToken },
    "ambiguous-create",
    headers,
    201,
  );
  const lost = { action: "claim", generation: ambiguous.data.generation };
  const outcome = await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    lost,
    "ambiguous-claim",
    headers,
  );
  assert.equal(outcome.data.state, "claim_failed");
  assert.equal(outcome.data.error_code, "claim_uncertain");
  assert.equal(claimCalls, 2);
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + ambiguousId + "/commands",
      lost,
      "ambiguous-claim",
      headers,
    ),
    outcome,
  );
  assert.equal(claimCalls, 2, "Uncertain one-use claim must not repeat");
  await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    lost,
    "new-claim-key",
    headers,
    409,
  );
  assert.equal(claimCalls, 2);
  await call(
    root + "/banking/connections",
    { ...body, setup_token: ambiguousToken },
    "create",
    headers,
    409,
  );
  const changes = await call(
    root + "/changes?cursor=" + encodeURIComponent(checkpoint),
  );
  assert.ok(changes.data.some((r) => r.entity === "bank_connections"));
  const point = (
    await call(
      root + "/backups",
      { label: "Secret boundary" },
      "backup",
      headers,
      201,
    )
  ).data;
  const download = await fetch(
    base + root + "/backups/" + point.id + "/download",
  );
  assert.equal(download.status, 200);
  const bytes = Buffer.from(await download.arrayBuffer());
  const archive = spawnSync(
    "python3",
    [
      "-c",
      'import sys,io,zipfile; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); print("\\n".join(z.read(n).decode() for n in z.namelist()))',
    ],
    { input: bytes, maxBuffer: 8 * 1024 * 1024 },
  );
  assert.equal(archive.status, 0, archive.stderr.toString());
  for (const secret of [
    token,
    ambiguousToken,
    "synthetic:credential",
    origin,
  ]) {
    assert.ok(
      !archive.stdout.includes(secret),
      "Portable backup excludes credentials",
    );
    assert.ok(
      !JSON.stringify(changes).includes(secret),
      "Change feed excludes credentials",
    );
  }
  const canceledId = randomUUID(),
    canceledBody = { ...body, id: canceledId };
  await call(
    root + "/banking/connections",
    canceledBody,
    "cancel-create",
    { ...headers, "X-Actual-Cancel-If-Uncommitted": "true" },
    409,
  );
  await call(
    root + "/banking/connections",
    canceledBody,
    "cancel-create",
    headers,
    409,
  );
  assert.equal((await call(root + "/banking")).data.connections.length, 2);
  assert.deepEqual(
    (await call(root + "/transactions")).data,
    [],
    "Staging a credential never creates financial entries",
  );
  server.kill();
  await new Promise((resolve) => server.once("close", resolve));
  base = await start();
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + id + "/commands",
      command,
      "claim",
      headers,
    ),
    claimed,
  );
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + ambiguousId + "/commands",
      lost,
      "ambiguous-claim",
      headers,
    ),
    outcome,
  );
  assert.equal(claimCalls, 2, "Restart never reclaims either token");
  for (const secret of [token, ambiguousToken, "synthetic:credential", origin])
    assert.ok(
      !stderr.includes(secret),
      "Logs exclude externally submitted credentials",
    );
  assert.deepEqual(
    await call(
      root + "/banking/connections/" + id + "/commands",
      discover,
      "discover",
      headers,
    ),
    discovered,
  );
  assert.equal(downloadCalls, 1);
  const replacement = {
    action: "replace",
    generation: ambiguous.data.generation,
    setup_token: Buffer.from(origin + "/claim/replacement").toString("base64"),
  };
  const replaced = await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    replacement,
    "replace",
    headers,
  );
  assert.equal(replaced.data.state, "claim_pending");
  assert.notEqual(replaced.data.generation, ambiguous.data.generation);
  await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    lost,
    "stale-generation",
    headers,
    409,
  );
  const reconnected = await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    { action: "claim", generation: replaced.data.generation },
    "claim-replacement",
    headers,
  );
  assert.equal(reconnected.data.state, "connected");
  assert.equal(claimCalls, 3);
  const remove = { action: "remove", generation: reconnected.data.generation };
  const removed = await call(
    root + "/banking/connections/" + ambiguousId + "/commands",
    remove,
    "remove",
    headers,
  );
  assert.equal(removed.data.deleted, true);
  assert.equal((await call(root + "/banking")).data.connections.length, 1);
  for (const [variant, expected] of [
    ["partial", "provider_incomplete"],
    ["legacy", "provider_attention"],
    ["rate", "provider_rate_limited"],
    ["bad-json", "provider_invalid_response"],
    ["large", "provider_response_limit"],
    ["continuation", "provider_pagination_unsupported"],
  ]) {
    mode = variant;
    const key = "transport-" + variant;
    const out = await call(
      root + "/banking/connections/" + id + "/commands",
      discover,
      key,
      headers,
    );
    assert.equal(out.data.errors[0].code, expected);
    assert.ok(
      !JSON.stringify(out).includes(origin),
      "Provider diagnostics never echo secrets",
    );
    const before = downloadCalls;
    assert.deepEqual(
      await call(
        root + "/banking/connections/" + id + "/commands",
        discover,
        key,
        headers,
      ),
      out,
    );
    assert.equal(downloadCalls, before);
    assert.equal(
      (await call(root + "/banking")).data.connections.find((c) => c.id === id)
        .error_code,
      expected,
    );
    if (variant === "partial") {
      assert.equal(
        out.data.accounts[0].diagnostics[0].code,
        "provider_incomplete",
      );
      assert.deepEqual(out.data.accounts[1].diagnostics, []);
    }
  }
  mode = "normal";
  const quotaBase = downloadCalls;
  for (let i = quotaBase; i < 24; i++)
    await call(
      root + "/banking/connections/" + id + "/commands",
      discover,
      "quota-" + i,
      headers,
    );
  assert.equal(downloadCalls, 24);
  await call(
    root + "/banking/connections/" + id + "/commands",
    discover,
    "quota-limit",
    headers,
    429,
  );
  assert.equal(downloadCalls, 24, "Local quota refuses before remote HTTP");
  assert.deepEqual(
    (await call(root + "/transactions")).data,
    [],
    "Provider failures and discovery never fabricate ledger rows",
  );
  const revokedId = randomUUID(),
    revokedToken = Buffer.from(origin + "/claim/revoked").toString("base64");
  const revokedStage = await call(
    root + "/banking/connections",
    { ...body, id: revokedId, setup_token: revokedToken },
    "revoked-stage",
    headers,
    201,
  );
  const revokedConnection = await call(
    root + "/banking/connections/" + revokedId + "/commands",
    { action: "claim", generation: revokedStage.data.generation },
    "revoked-claim",
    headers,
  );
  mode = "revoked";
  const revokedCommand = {
    action: "discover",
    generation: revokedConnection.data.generation,
  };
  const revoked = await call(
    root + "/banking/connections/" + revokedId + "/commands",
    revokedCommand,
    "revoked-fetch",
    headers,
  );
  assert.equal(revoked.data.connection.state, "credentials_missing");
  assert.equal(revoked.data.errors[0].code, "provider_revoked");
  const revokedCalls = downloadCalls;
  await call(
    root + "/banking/connections/" + revokedId + "/commands",
    revokedCommand,
    "revoked-new-key",
    headers,
    409,
  );
  assert.equal(
    downloadCalls,
    revokedCalls,
    "Revoked credentials require explicit replacement",
  );
  mode = "normal";
  const cancelId = randomUUID(),
    cancelToken = Buffer.from(origin + "/claim/cancel-capacity").toString(
      "base64",
    );
  const cancelStage = await call(
    root + "/banking/connections",
    { ...body, id: cancelId, setup_token: cancelToken },
    "cancel-capacity-stage",
    headers,
    201,
  );
  const cancelConnection = await call(
    root + "/banking/connections/" + cancelId + "/commands",
    { action: "claim", generation: cancelStage.data.generation },
    "cancel-capacity-claim",
    headers,
  );
  const cancelCommand = {
    action: "discover",
    generation: cancelConnection.data.generation,
  };
  const cancelPath = root + "/banking/connections/" + cancelId + "/commands";
  const cancelQuotaBase = downloadCalls;
  const beforeCancels = (await call(root + "/checkpoint")).data.snapshot;
  for (let i = 0; i < 4; i++) {
    mode = "held-large";
    const observed = new Promise((resolve) => (requestObserved = resolve));
    const pending = call(cancelPath, cancelCommand, "held-" + i, headers, 409);
    await Promise.race([
      observed,
      new Promise((_, reject) =>
        setTimeout(() => reject(Error("Provider GET did not start")), 5000),
      ),
    ]);
    const canceled = await call(
      cancelPath,
      cancelCommand,
      "held-" + i,
      { ...headers, "X-Actual-Cancel-If-Uncommitted": "true" },
      409,
    );
    assert.equal(canceled.error.code, "operation_canceled");
    heldResponse.setHeader("Content-Type", "application/json");
    heldResponse.end(
      JSON.stringify({
        accounts: [],
        padding: "x".repeat(16 * 1024 * 1024 - 1024),
      }),
    );
    assert.deepEqual(
      await pending,
      canceled,
      "Delayed original must return the durable canceled outcome",
    );
    assert.deepEqual(
      await call(cancelPath, cancelCommand, "held-" + i, headers, 409),
      canceled,
    );
  }
  assert.equal(
    (await call(root + "/checkpoint")).data.snapshot,
    beforeCancels,
    "Canceled late responses never change metadata",
  );
  mode = "normal-large";
  const afterCancels = await call(
    cancelPath,
    cancelCommand,
    "after-held-cancels",
    headers,
  );
  assert.deepEqual(
    afterCancels.data.errors,
    [],
    "Canceled captures cannot permanently occupy recovery capacity",
  );
  assert.deepEqual((await call(root + "/transactions")).data, []);
  mode = "normal";
  for (let i = 5; i < 24; i++)
    await call(cancelPath, cancelCommand, "cancel-quota-" + i, headers);
  assert.equal(
    downloadCalls - cancelQuotaBase,
    24,
    "Canceled remote requests still count toward provider quota",
  );
  await call(cancelPath, cancelCommand, "cancel-quota-over", headers, 429);
  assert.equal(downloadCalls - cancelQuotaBase, 24);
  const contract = await (await fetch(base + "/openapi.json")).json();
  const schemaProof = spawnSync(
    "uv",
    [
      "run",
      "--with",
      "jsonschema==4.23.0",
      "python",
      "-c",
      `import sys,json
from jsonschema import Draft202012Validator,FormatChecker
p=json.load(sys.stdin); c=p['contract']
try:
 for name,value in p['values']:
  schema={**c['components']['schemas'][name],'components':c['components']}
  Draft202012Validator.check_schema(schema)
  Draft202012Validator(schema,format_checker=FormatChecker()).validate(value)
except Exception as e:
 print(name,type(e).__name__,list(getattr(e,'path',[])),str(getattr(e,'validator','')));sys.exit(1)
`,
    ],
    {
      input: JSON.stringify({
        contract,
        values: [
          ["BankConnectionCreate", body],
          ["BankConnectionCommand", command],
          ["BankConnectionCommand", replacement],
          ["BankConnectionCommand", discover],
          ["BankDiscovery", discovered.data],
          ["BankConnection", claimed.data],
          ["BankState", (await call(root + "/banking")).data],
        ],
      }),
      maxBuffer: 1024 * 1024,
    },
  );
  assert.equal(
    schemaProof.status,
    0,
    schemaProof.stdout.toString() + schemaProof.stderr.toString(),
  );
  console.log(
    "PASS bank lifecycle/discovery/transport/rate/recovery/contract/secret boundaries",
  );
} finally {
  server.kill();
  await new Promise((resolve) => server.once("close", resolve));
  await new Promise((resolve) => provider.close(resolve));
  await rm(directory, { recursive: true, force: true });
}
