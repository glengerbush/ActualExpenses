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
let claimCalls = 0;
const provider = createServer((req, res) => {
  if (req.url === "/uncertain" && req.method === "POST") {
    claimCalls++;
    req.socket.destroy();
    return;
  }
  if (req.url === "/claim" && req.method === "POST") {
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
    token = Buffer.from(origin + "/claim").toString("base64");
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
    "PASS staging/claim/replay/uncertain/cancel/feed/portable-secret boundaries",
  );
} finally {
  server.kill();
  await new Promise((resolve) => server.once("close", resolve));
  await new Promise((resolve) => provider.close(resolve));
  await rm(directory, { recursive: true, force: true });
}
