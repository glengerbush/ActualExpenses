// Existing SimpleFIN claim/download public identity seam; no private DB reads.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
async function startServer() {
  const directory = await mkdtemp(tmpdir() + "/bank-intent-");
  const binary =
    process.env.ACTUALBUDGET_BANK_BINARY ??
    fileURLToPath(
      new URL("../server/target/debug/actualbudget-server", import.meta.url),
    );
  const child = spawn(binary, [], {
    env: {
      ...process.env,
      ACTUALBUDGET_DATA_DIR: directory,
      ACTUALBUDGET_HOST: "127.0.0.1",
      ACTUALBUDGET_PORT: "0",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let logs = "";
  child.stderr.on("data", (c) => (logs = (logs + c).slice(-16000)));
  const closed = new Promise((resolve) => child.once("close", resolve));
  const stop = async () => {
    if (child.exitCode === null) child.kill();
    await closed;
    await rm(directory, { recursive: true, force: true });
  };
  try {
    const url = await new Promise((resolve, reject) => {
      let output = "";
      const timer = setTimeout(() => reject(Error("Startup: " + logs)), 10000);
      child.once("error", reject);
      child.once("exit", (code) => {
        clearTimeout(timer);
        reject(Error("Server exited " + code + ": " + logs));
      });
      child.stdout.on("data", (c) => {
        output += c;
        const m = /LISTENING 127\.0\.0\.1:(\d+)/.exec(output);
        if (m) {
          clearTimeout(timer);
          resolve(`http://127.0.0.1:${m[1]}`);
        }
      });
    });
    return { url, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}
assert.ok(
  !process.env.ACTUALBUDGET_TEST_URL,
  "Held-provider proof requires its disposable server",
);
const claim = process.argv.includes("--claim");
let held,
  seen,
  calls = 0;
const provider = createServer((req, res) => {
  calls++;
  if (
    (claim && req.url === "/claim") ||
    (!claim && req.url.startsWith("/access/accounts"))
  ) {
    held = res;
    seen?.();
    return;
  }
  if (req.url === "/claim") res.end(origin + "/access");
  else {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify({ accounts: [], errlist: [], connections: [] }));
  }
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
const origin = `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}`;
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, headers = {}, status = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json();
  assert.equal(response.status, status, JSON.stringify(data));
  return data;
}
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`;
  await call(
    "/budgets",
    { id: budget, name: "Pending bank identity", budget_type: "tracking" },
    { "Idempotency-Key": randomUUID() },
    201,
  );
  const headers = {
    "X-Actual-Budget-Epoch": (await call(root + "/checkpoint")).data.epoch,
  };
  const token = Buffer.from(
    process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim",
  ).toString("base64");
  let connection = (
    await call(
      root + "/banking/connections",
      {
        id: randomUUID(),
        provider: "simplefin",
        name: "Synthetic",
        setup_token: token,
      },
      { ...headers, "Idempotency-Key": randomUUID() },
      201,
    )
  ).data;
  const path = root + `/banking/connections/${connection.id}/commands`;
  if (!claim)
    connection = (
      await call(
        path,
        { action: "claim", generation: connection.generation },
        { ...headers, "Idempotency-Key": randomUUID() },
      )
    ).data;
  const body = {
      action: claim ? "claim" : "discover",
      generation: connection.generation,
    },
    key = randomUUID();
  const observed = new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error("Provider request not observed")),
      5000,
    );
    seen = () => {
      clearTimeout(timer);
      resolve();
    };
  });
  const delayed = call(path, body, { ...headers, "Idempotency-Key": key }, 409);
  await observed;
  const before = (await call(root + "/banking")).data;
  if (!claim) {
    const refreshCancel = {
      action: "refresh",
      generation: connection.generation,
      snapshot: before.snapshot,
      start_date: "2026-11-01",
      end_date: "2026-11-10",
      accounts: [{ account_id: randomUUID(), revision: 1 }],
    };
    const refused = await call(
      path,
      refreshCancel,
      {
        ...headers,
        "Idempotency-Key": key,
        "X-Actual-Cancel-If-Uncommitted": "true",
        "X-Actual-Edit-Clock": before.clock,
        "X-Actual-History-Owner": randomUUID(),
      },
      409,
    );
    assert.notEqual(
      refused.error.code,
      "operation_canceled",
      "Owned refresh cancellation erased a different captured request",
    );
    assert.deepEqual((await call(root + "/banking")).data, before);
    await call(
      path,
      body,
      { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
      404,
    );
  }
  for (const [changed, changedHeaders] of [
    [{ ...body, generation: randomUUID() }, {}],
    [body, { "X-Actual-Budget-Epoch": "0".repeat(32) }],
  ]) {
    const refused = await call(
      path,
      changed,
      {
        ...headers,
        "Idempotency-Key": key,
        ...changedHeaders,
        "X-Actual-Cancel-If-Uncommitted": "true",
      },
      409,
    );
    assert.notEqual(
      refused.error.code,
      "operation_canceled",
      "Altered cancel installed a terminal receipt",
    );
    assert.deepEqual((await call(root + "/banking")).data, before);
    await call(
      path,
      body,
      { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
      404,
    );
  }
  for (const command of [
    { action: "remove", generation: connection.generation },
    {
      action: "replace",
      generation: connection.generation,
      setup_token: token,
    },
  ]) {
    await call(path, command, { ...headers, "Idempotency-Key": key }, 409);
    assert.deepEqual((await call(root + "/banking")).data, before);
    await call(
      path,
      body,
      { ...headers, "Idempotency-Key": key, "X-Actual-Receipt-Only": "true" },
      404,
    );
  }
  const canceled = await call(
    path,
    body,
    {
      ...headers,
      "Idempotency-Key": key,
      "X-Actual-Cancel-If-Uncommitted": "true",
    },
    409,
  );
  assert.equal(canceled.error.code, "operation_canceled");
  if (claim) held.end(origin + "/access");
  else held.end(JSON.stringify({ accounts: [], errlist: [], connections: [] }));
  assert.deepEqual(await delayed, canceled);
  const callsAfter = calls;
  assert.deepEqual(
    await call(path, body, { ...headers, "Idempotency-Key": key }, 409),
    canceled,
  );
  assert.equal(calls, callsAfter);
  assert.deepEqual((await call(root + "/transactions")).data, []);
  console.log(
    "PASS held SimpleFIN " +
      (claim ? "claim" : "capture") +
      ": altered cancel/remove/replace refuses without state/receipt writes; original cancel/late replay exact",
  );
} finally {
  provider.closeAllConnections();
  await server.stop();
  await new Promise((resolve) => provider.close(resolve));
}
