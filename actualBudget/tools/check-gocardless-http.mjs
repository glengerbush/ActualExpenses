// Local controlled-provider public seam; never a real account/provider call.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Controlled-provider API tracer requires its own disposable server",
);
const directory = await mkdtemp(tmpdir() + "/gocardless-http-");
const binary =
  process.env.ACTUALBUDGET_BANK_BINARY ??
  fileURLToPath(
    new URL("../server/target/debug/actualbudget-server", import.meta.url),
  );
let providerCalls = 0,
  consentStatus = "GC",
  holdConsent = false,
  heldConsent,
  consentObserved,
  holdStatus = false,
  heldStatus,
  statusObserved;
const provider = createServer((req, res) => {
  providerCalls++;
  res.setHeader("Content-Type", "application/json");
  if (req.url === "/api/v2/token/new/")
    res.end(
      JSON.stringify({
        refresh: "SYNTHETIC-REFRESH",
        refresh_expires: 2592000,
      }),
    );
  else if (req.url === "/api/v2/token/refresh/")
    res.end(
      JSON.stringify({ access: "SYNTHETIC-ACCESS", access_expires: 86400 }),
    );
  else if (req.url === "/api/v2/institutions/?country=GB") {
    assert.equal(req.headers.authorization, "Bearer SYNTHETIC-ACCESS");
    res.end(
      JSON.stringify([
        {
          id: "FIXTURE_BANK",
          name: "Synthetic bank",
          countries: ["GB"],
          transaction_total_days: "730",
          max_access_valid_for_days: "180",
          supported_features: ["account_selection"],
        },
      ]),
    );
  } else if (req.url === "/api/v2/institutions/FIXTURE_BANK/")
    res.end(
      JSON.stringify({
        id: "FIXTURE_BANK",
        name: "Synthetic bank",
        countries: ["GB"],
        transaction_total_days: "730",
        max_access_valid_for_days: "180",
        supported_features: [
          "account_selection",
          "separate_continuous_history_consent",
        ],
      }),
    );
  else if (req.url === "/api/v2/agreements/enduser/")
    res.end(JSON.stringify({ id: "agreement-A" }));
  else if (req.url === "/api/v2/requisitions/" && req.method === "POST") {
    if (holdConsent) {
      heldConsent = res;
      consentObserved?.();
      return;
    }
    res.end(
      JSON.stringify({
        id: "requisition-A",
        agreement: "agreement-A",
        link: "https://ob.gocardless.com/psd2/start/SYNTHETIC-HANDOFF",
      }),
    );
  } else if (req.url === "/api/v2/requisitions/requisition-A/") {
    if (holdStatus) {
      heldStatus = res;
      statusObserved?.();
      return;
    }
    res.end(
      JSON.stringify({
        id: "requisition-A",
        status: consentStatus,
        institution_id: "FIXTURE_BANK",
        accounts: ["remote-A"],
      }),
    );
  } else {
    res.statusCode = 404;
    res.end(JSON.stringify({ summary: "Synthetic unknown route" }));
  }
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
const providerOrigin = `http://127.0.0.1:${provider.address().port}`;
const child = spawn(binary, [], {
  env: {
    ...process.env,
    ACTUALBUDGET_DATA_DIR: directory,
    ACTUALBUDGET_HOST: "127.0.0.1",
    ACTUALBUDGET_PORT: "0",
    ACTUALBUDGET_BANK_TEST_ORIGIN: providerOrigin,
    ACTUALBUDGET_GOCARDLESS_TEST_ORIGIN: providerOrigin,
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let logs = "";
child.stderr.on("data", (c) => (logs = (logs + c).slice(-16000)));
const closed = new Promise((resolve) => child.once("close", resolve));
let api;
const proofs = [];
async function call(path, body, headers = {}, status = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json();
  if (path.endsWith("/gocardless/authorization"))
    assert.equal(response.headers.get("cache-control"), "no-store");
  assert.equal(response.status, status, JSON.stringify(data));
  if (path.endsWith("/banking/connections") && body)
    proofs.push(["BankConnectionCreate", body]);
  if (path.endsWith("/commands") && path.includes("/banking/") && body)
    proofs.push(["BankConnectionCommand", body]);
  if (status >= 400) proofs.push(["Error", data]);
  else if (
    path.endsWith("/banking/connections") ||
    (path.endsWith("/commands") && path.includes("/banking/"))
  )
    proofs.push(["BankConnection", data.data]);
  else if (path.includes("/gocardless/institutions"))
    proofs.push(["GoCardlessInstitutions", data]);
  else if (path.endsWith("/gocardless/authorization"))
    proofs.push(["GoCardlessAuthorization", data]);
  else if (path.endsWith("/banking")) proofs.push(["BankState", data.data]);
  for (const secret of [
    "SYNTHETIC-USER-SECRET-ID",
    "SYNTHETIC-USER-SECRET-KEY",
  ])
    assert.ok(!JSON.stringify(data).includes(secret));
  return data;
}
try {
  api = await new Promise((resolve, reject) => {
    let output = "";
    const timer = setTimeout(
      () => reject(Error("Server startup: " + logs)),
      10000,
    );
    child.once("error", reject);
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(Error("Server exited " + code + ": " + logs));
    });
    child.stdout.on("data", (chunk) => {
      output += chunk;
      const m = /LISTENING 127\.0\.0\.1:(\d+)/.exec(output);
      if (m) {
        clearTimeout(timer);
        resolve(`http://127.0.0.1:${m[1]}/api/v1`);
      }
    });
  });
  const id = randomUUID(),
    root = `/budgets/${id}`;
  await call(
    "/budgets",
    { id, name: "Synthetic GoCardless", budget_type: "tracking" },
    { "Idempotency-Key": randomUUID() },
    201,
  );
  const epoch = (await call(root + "/checkpoint")).data.epoch;
  const path = root + "/banking/connections",
    key = randomUUID();
  const body = {
    id: randomUUID(),
    provider: "gocardless",
    name: "Synthetic institution",
    secret_id: "SYNTHETIC-USER-SECRET-ID",
    secret_key: "SYNTHETIC-USER-SECRET-KEY",
  };
  const headers = { "Idempotency-Key": key, "X-Actual-Budget-Epoch": epoch };
  const created = await call(path, body, headers, 201);
  assert.equal(created.data.provider, "gocardless");
  assert.equal(created.data.state, "claim_pending");
  const recovery = {
    id: body.id,
    provider: body.provider,
    name: body.name,
    credentials_sha256: createHash("sha256")
      .update(JSON.stringify([body.secret_id, body.secret_key]))
      .digest("hex"),
  };
  assert.deepEqual(
    await call(
      path,
      recovery,
      { ...headers, "X-Actual-Receipt-Only": "true" },
      201,
    ),
    created,
  );
  await call(
    path,
    { ...recovery, id: randomUUID() },
    { ...headers, "Idempotency-Key": randomUUID() },
    400,
  );
  assert.deepEqual((await call(root + "/banking")).data.connections, [
    created.data,
  ]);
  assert.equal(providerCalls, 0);
  const institutionPath = `${path}/${body.id}/gocardless/institutions?country=GB`;
  const institutions = await call(institutionPath);
  assert.equal(institutions.data[0].id, "FIXTURE_BANK");
  assert.equal(providerCalls, 3); // Refresh-only token new → access exchange → institutions.
  assert.deepEqual(await call(institutionPath), institutions);
  assert.equal(providerCalls, 4); // Stored access token reused, never exposed.
  const commandPath = `${path}/${body.id}/commands`,
    authKey = randomUUID();
  const authorize = {
    action: "authorize",
    generation: created.data.generation,
    institution_id: "FIXTURE_BANK",
    country: "GB",
    redirect_origin: "http://actualbudget.test:45000",
  };
  const authorization = await call(commandPath, authorize, {
    ...headers,
    "Idempotency-Key": authKey,
  });
  assert.notEqual(authorization.data.generation, created.data.generation);
  assert.ok(!JSON.stringify(authorization).includes("SYNTHETIC-HANDOFF"));
  assert.deepEqual(
    await call(commandPath, authorize, {
      ...headers,
      "Idempotency-Key": authKey,
    }),
    authorization,
  );
  const handoffPath = `${path}/${body.id}/gocardless/authorization`;
  const handoff = await call(handoffPath);
  assert.equal(
    handoff.data.url,
    "https://ob.gocardless.com/psd2/start/SYNTHETIC-HANDOFF",
  );
  const backup = (
    await call(
      root + "/backups",
      { label: "Private GoCardless consent excluded" },
      { ...headers, "Idempotency-Key": randomUUID() },
      201,
    )
  ).data;
  const bytes = Buffer.from(
    await (
      await fetch(api + root + `/backups/${backup.id}/download`)
    ).arrayBuffer(),
  );
  const decoded = spawnSync(
    "python3",
    [
      "-c",
      "import io,sys,zipfile; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); sys.stdout.buffer.write(b'\\n'.join(z.read(n) for n in z.namelist()))",
    ],
    { input: bytes, maxBuffer: 8 * 1024 * 1024 },
  );
  assert.equal(decoded.status, 0, decoded.stderr.toString());
  for (const secret of [
    body.secret_id,
    body.secret_key,
    "SYNTHETIC-ACCESS",
    "SYNTHETIC-REFRESH",
    "SYNTHETIC-HANDOFF",
  ])
    assert.ok(
      !decoded.stdout.includes(Buffer.from(secret)),
      "Portable archive exposed " + secret,
    );
  const copyId = randomUUID();
  await call(
    root + "/duplicate",
    { id: copyId, name: "GoCardless portable copy" },
    { ...headers, "Idempotency-Key": randomUUID() },
    201,
  );
  const copyState = (await call(`/budgets/${copyId}/banking`)).data;
  assert.equal(copyState.connections.length, 1);
  assert.equal(copyState.connections[0].id, body.id);
  assert.equal(copyState.connections[0].state, "credentials_missing");
  assert.equal(
    (
      await call(
        `/budgets/${copyId}/banking/connections/${body.id}/gocardless/authorization`,
      )
    ).data.url,
    null,
  );
  const check = {
    action: "check_authorization",
    generation: authorization.data.generation,
  };
  const firstCheckKey = randomUUID();
  const waiting = await call(commandPath, check, {
    ...headers,
    "Idempotency-Key": firstCheckKey,
  });
  assert.equal(waiting.data.state, "claim_pending");
  consentStatus = "LN";
  const linked = await call(commandPath, check, {
    ...headers,
    "Idempotency-Key": randomUUID(),
  });
  assert.equal(linked.data.state, "connected");
  assert.deepEqual(
    await call(commandPath, check, {
      ...headers,
      "Idempotency-Key": firstCheckKey,
    }),
    waiting,
  );
  const currentHandoff = await call(handoffPath);
  holdStatus = true;
  const statusSeen = new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("Held status GET not observed")),
      5000,
    );
    statusObserved = () => {
      clearTimeout(timer);
      resolve();
    };
  });
  const canceledCheckKey = randomUUID();
  const delayedCheck = call(
    commandPath,
    check,
    { ...headers, "Idempotency-Key": canceledCheckKey },
    409,
  );
  await statusSeen;
  for (const [changed, changedHeaders] of [
    [{ ...check, generation: randomUUID() }, {}],
    [check, { "X-Actual-Budget-Epoch": "0".repeat(32) }],
  ]) {
    const refused = await call(
      commandPath,
      changed,
      {
        ...headers,
        "Idempotency-Key": canceledCheckKey,
        ...changedHeaders,
        "X-Actual-Cancel-If-Uncommitted": "true",
      },
      409,
    );
    assert.notEqual(
      refused.error.code,
      "operation_canceled",
      "Altered cancellation must not create a terminal receipt",
    );
    assert.deepEqual(await call(handoffPath), currentHandoff);
    await call(
      commandPath,
      check,
      {
        ...headers,
        "Idempotency-Key": canceledCheckKey,
        "X-Actual-Receipt-Only": "true",
      },
      404,
    );
  }
  await call(
    commandPath,
    { action: "remove", generation: linked.data.generation },
    { ...headers, "Idempotency-Key": canceledCheckKey },
    409,
  );
  assert.deepEqual(await call(handoffPath), currentHandoff);
  await call(
    commandPath,
    check,
    {
      ...headers,
      "Idempotency-Key": canceledCheckKey,
      "X-Actual-Cancel-If-Uncommitted": "true",
    },
    409,
  );
  heldStatus.end(
    JSON.stringify({
      id: "requisition-A",
      status: "EX",
      institution_id: "FIXTURE_BANK",
      accounts: ["remote-A"],
    }),
  );
  await delayedCheck;
  holdStatus = false;
  assert.deepEqual(
    await call(handoffPath),
    currentHandoff,
    "Canceling a status read must retain the existing consent",
  );
  assert.deepEqual(
    (await call(root + "/banking")).data.connections.find(
      (c) => c.id === body.id,
    ),
    linked.data,
  );
  const replaceKey = randomUUID();
  const replacement = {
    action: "replace",
    generation: linked.data.generation,
    secret_id: body.secret_id,
    secret_key: body.secret_key,
  };
  const replaced = await call(commandPath, replacement, {
    ...headers,
    "Idempotency-Key": replaceKey,
  });
  assert.notEqual(replaced.data.generation, linked.data.generation);
  assert.equal(replaced.data.state, "claim_pending");
  assert.deepEqual(
    await call(
      commandPath,
      {
        action: "replace",
        generation: replacement.generation,
        credentials_sha256: recovery.credentials_sha256,
      },
      {
        ...headers,
        "Idempotency-Key": replaceKey,
        "X-Actual-Receipt-Only": "true",
      },
    ),
    replaced,
  );
  assert.equal((await call(handoffPath)).data.url, null);
  const removeKey = randomUUID(),
    remove = { action: "remove", generation: replaced.data.generation };
  const removed = await call(commandPath, remove, {
    ...headers,
    "Idempotency-Key": removeKey,
  });
  assert.equal(removed.data.deleted, true);
  assert.deepEqual(
    await call(commandPath, remove, {
      ...headers,
      "Idempotency-Key": removeKey,
    }),
    removed,
  );
  await call(institutionPath, undefined, {}, 404);
  const pendingBody = { ...body, id: randomUUID(), name: "Canceled consent" };
  const pending = (
    await call(
      path,
      pendingBody,
      { ...headers, "Idempotency-Key": randomUUID() },
      201,
    )
  ).data;
  const pendingPath = `${path}/${pending.id}/commands`,
    pendingKey = randomUUID();
  const pendingAuth = { ...authorize, generation: pending.generation };
  holdConsent = true;
  const observed = new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("Held consent POST not observed")),
      5000,
    );
    consentObserved = () => {
      clearTimeout(timer);
      resolve();
    };
  });
  const delayed = call(
    pendingPath,
    pendingAuth,
    { ...headers, "Idempotency-Key": pendingKey },
    409,
  );
  await observed;
  const alteredAuth = await call(
    pendingPath,
    { ...pendingAuth, country: "DE" },
    {
      ...headers,
      "Idempotency-Key": pendingKey,
      "X-Actual-Cancel-If-Uncommitted": "true",
    },
    409,
  );
  assert.notEqual(alteredAuth.error.code, "operation_canceled");
  await call(
    pendingPath,
    pendingAuth,
    {
      ...headers,
      "Idempotency-Key": pendingKey,
      "X-Actual-Receipt-Only": "true",
    },
    404,
  );
  await call(
    pendingPath,
    pendingAuth,
    {
      ...headers,
      "Idempotency-Key": pendingKey,
      "X-Actual-Cancel-If-Uncommitted": "true",
    },
    409,
  );
  heldConsent.end(
    JSON.stringify({
      id: "requisition-canceled",
      agreement: "agreement-A",
      link: "https://ob.gocardless.com/psd2/start/SYNTHETIC-CANCELED-HANDOFF",
    }),
  );
  await delayed;
  const callsAfterCancel = providerCalls;
  await call(
    pendingPath,
    pendingAuth,
    { ...headers, "Idempotency-Key": pendingKey },
    409,
  );
  assert.equal(providerCalls, callsAfterCancel);
  assert.equal(
    (await call(`${path}/${pending.id}/gocardless/authorization`)).data.url,
    null,
  );
  assert.deepEqual((await call(root + "/transactions")).data, []);
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
    "PASS GoCardless staging/receipt, official token shapes, consent/replacement/removal, private portable clone and held-POST/GET cancellation without resurrection; " +
      proofs.length +
      " served-contract observations",
  );
} finally {
  provider.closeAllConnections();
  if (child.exitCode === null) child.kill();
  await closed;
  await rm(directory, { recursive: true, force: true });
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
