// Two real provider GETs complete in reverse order; newer cache must win.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { randomUUID } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
let held,
  started,
  hold = false,
  balance = "100.00";
const provider = createServer((req, res) => {
  if (req.method === "POST") {
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  const body = JSON.stringify({
    accounts: [
      {
        id: "account",
        name: "Source account",
        org: { id: "institution", name: "Source institution" },
        currency: "USD",
        balance,
        "balance-date": 1794225600,
      },
    ],
  });
  const send = () => {
    res.setHeader("Content-Type", "application/json");
    res.end(body);
  };
  if (hold) {
    hold = false;
    held = send;
    started();
  } else send();
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, headers = {}, expected = 200) {
  const response = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  assert.equal(response.status, expected, JSON.stringify(value));
  return value;
}
try {
  const budget = (
    await call(
      "/budgets",
      {
        id: randomUUID(),
        name: "Reverse provider delivery",
        budget_type: "envelope",
      },
      { "Idempotency-Key": randomUUID() },
      201,
    )
  ).data.id;
  const root = `/budgets/${budget}`,
    epoch = (await call(root + "/checkpoint")).data.epoch;
  const headers = { "X-Actual-Budget-Epoch": epoch };
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
  const body = { action: "discover", generation: connection.generation };
  await call(path, body, { ...headers, "Idempotency-Key": randomUUID() });
  balance = "111.00";
  hold = true;
  const ready = new Promise((resolve) => (started = resolve)),
    key = randomUUID();
  const old = fetch(api + path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...headers,
      "Idempotency-Key": key,
    },
    body: JSON.stringify(body),
  });
  let timer;
  try {
    await Promise.race([
      ready,
      new Promise(
        (_, reject) =>
          (timer = setTimeout(
            () => reject(Error("Provider GET not reached")),
            5000,
          )),
      ),
    ]);
  } finally {
    clearTimeout(timer);
  }
  balance = "222.00";
  await call(path, body, { ...headers, "Idempotency-Key": randomUUID() });
  const newest = await call(root + "/banking");
  assert.equal(newest.data.accounts[0].balance, "222.00");
  held();
  held = undefined;
  const oldResponse = await old,
    oldBody = await oldResponse.json();
  assert.equal(oldResponse.status, 409, JSON.stringify(oldBody));
  assert.deepEqual(
    await call(root + "/banking"),
    newest,
    "Late capture cannot roll back newer provider cache",
  );
  // Captured stale request is still the same immutable operation; explicit
  // cancellation releases it and prevents any delayed original resurrection.
  assert.equal(
    (
      await call(
        path,
        body,
        {
          ...headers,
          "Idempotency-Key": key,
          "X-Actual-Cancel-If-Uncommitted": "true",
        },
        409,
      )
    ).error.code,
    "operation_canceled",
  );
  await call(path, body, { ...headers, "Idempotency-Key": key }, 409);
  assert.equal((await call(root + "/transactions")).data.length, 0);
  assert.equal(
    (await call(root + "/banking")).data.accounts[0].balance,
    "222.00",
  );
  console.log(
    "PASS reverse GET completion refuses stale capture; exact cancellation prevents resurrection",
  );
} finally {
  held?.();
  await server.stop();
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
