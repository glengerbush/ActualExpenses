// Browser recovery can retain a digest and original nonsecret identity, not tokens.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { spawnSync } from "node:child_process";
import { startServer } from "../web/tests/server.mjs";
let providerCalls = 0;
const provider = createServer((_, res) => {
  providerCalls++;
  res.statusCode = 500;
  res.end();
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
const token = Buffer.from(
    process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim/NOT-A-REAL-SECRET",
  ).toString("base64"),
  digest = createHash("sha256").update(token).digest("hex");
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
const proofs = [];
async function call(path, body, headers = {}, status = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await response.json();
  if (body && path.endsWith("/banking/connections"))
    proofs.push(["BankConnectionCreate", body]);
  if (body && path.endsWith("/commands"))
    proofs.push(["BankConnectionCommand", body]);
  if (path.endsWith("/banking/connections") && status === 201)
    proofs.push(["BankConnection", result.data]);
  if (path.endsWith("/commands") && status === 200)
    proofs.push(["BankConnection", result.data]);
  assert.equal(response.status, status, JSON.stringify(result));
  assert.ok(!JSON.stringify(result).includes(token));
  return result;
}
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`;
  await call(
    "/budgets",
    { id: budget, name: "Digest-only recovery", budget_type: "tracking" },
    { "Idempotency-Key": randomUUID() },
    201,
  );
  const epoch = (await call(root + "/checkpoint")).data.epoch,
    headers = { "X-Actual-Budget-Epoch": epoch };
  const path = root + "/banking/connections",
    body = {
      id: randomUUID(),
      provider: "simplefin",
      name: "Original label",
      setup_token: token,
    },
    key = randomUUID();
  const created = await call(
    path,
    body,
    { ...headers, "Idempotency-Key": key },
    201,
  );
  const recovery = {
    id: body.id,
    provider: body.provider,
    name: body.name,
    setup_token_sha256: digest,
  };
  assert.deepEqual(
    await call(
      path,
      recovery,
      { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
      201,
    ),
    created,
  );
  await call(
    path,
    { ...recovery, name: "Changed label" },
    { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
    409,
  );
  await call(
    path,
    { ...recovery, setup_token_sha256: "0".repeat(64) },
    { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
    409,
  );
  await call(
    path,
    { ...recovery, id: randomUUID() },
    { ...headers, "Idempotency-Key": randomUUID() },
    400,
  );
  const canceledBody = { ...body, id: randomUUID() },
    canceledRecovery = { ...recovery, id: canceledBody.id },
    canceledKey = randomUUID();
  assert.equal(
    (
      await call(
        path,
        canceledRecovery,
        {
          ...headers,
          "Idempotency-Key": canceledKey,
          "X-Actual-Cancel-If-Uncommitted": "true",
        },
        409,
      )
    ).error.code,
    "operation_canceled",
  );
  await call(
    path,
    canceledBody,
    { ...headers, "Idempotency-Key": canceledKey },
    409,
  );
  const commandPath = path + `/${body.id}/commands`,
    replace = {
      action: "replace",
      generation: created.data.generation,
      setup_token: token,
    },
    replaceKey = randomUUID();
  const replaced = await call(commandPath, replace, {
    ...headers,
    "Idempotency-Key": replaceKey,
  });
  const replaceRecovery = {
    action: "replace",
    generation: replace.generation,
    setup_token_sha256: digest,
  };
  assert.deepEqual(
    await call(commandPath, replaceRecovery, {
      ...headers,
      "Idempotency-Key": replaceKey,
      "X-Actual-Receipt-Only": "true",
    }),
    replaced,
  );
  await call(
    commandPath,
    { ...replaceRecovery, generation: replaced.data.generation },
    { ...headers, "Idempotency-Key": randomUUID() },
    400,
  );
  const nextKey = randomUUID(),
    nextRecovery = { ...replaceRecovery, generation: replaced.data.generation };
  await call(
    commandPath,
    nextRecovery,
    {
      ...headers,
      "Idempotency-Key": nextKey,
      "X-Actual-Cancel-If-Uncommitted": "true",
    },
    409,
  );
  await call(
    commandPath,
    {
      action: "replace",
      generation: replaced.data.generation,
      setup_token: token,
    },
    { ...headers, "Idempotency-Key": nextKey },
    409,
  );
  const current = (await call(root + "/banking")).data;
  assert.equal(current.connections.length, 1);
  assert.equal(current.connections[0].generation, replaced.data.generation);
  assert.equal(
    providerCalls,
    0,
    "Digest recovery never claims/replaces remote credentials",
  );
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
    "PASS digest-only original identity receipt/cancel for setup and replacement; no credential creation or provider call",
  );
} finally {
  await server.stop();
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
