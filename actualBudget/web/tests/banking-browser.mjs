// External-server-compatible custody/recovery UI. Never claims a real token.
import { chromium, expect } from "@playwright/test";
import { randomUUID, createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1",
  budget = randomUUID();
async function call(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.status, await r.clone().text()).toBe(status);
  return r.json();
}
await call(
  "/budgets",
  { id: budget, name: "Bank recovery browser", budget_type: "tracking" },
  201,
);
const directory = new URL("../test-results/banking/", import.meta.url);
let browser;
try {
  await mkdir(directory, { recursive: true });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
      viewport: { width: 1440, height: 1000 },
    }),
    page = await context.newPage(),
    errors = [];
  await context.addInitScript(() =>
    Object.defineProperty(crypto, "subtle", { value: undefined }),
  );
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.evaluate(
    (b) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: b, accountId: "" }),
      ),
    budget,
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Bank connections", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Bank connections", exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => typeof crypto.subtle)).toBe("undefined");
  const token = Buffer.from(
      "https://synthetic.invalid/claim/NOT-A-REAL-TOKEN",
    ).toString("base64"),
    hash = createHash("sha256").update(token).digest("hex"),
    captures = [];
  await page
    .getByLabel("Connection name", { exact: true })
    .fill("Household bank");
  await page.getByLabel("SimpleFIN setup token", { exact: true }).fill(token);
  const routePattern = new RegExp(
    `/api/v1/budgets/${budget}/banking/connections$`,
  );
  await page.route(
    routePattern,
    async (route) => {
      const request = route.request(),
        headers = request.headers();
      captures.push({
        body: request.postData(),
        key: headers["idempotency-key"],
        epoch: headers["x-actual-budget-epoch"],
      });
      // The exact browser operation is committed through the public native seam;
      // Origin is omitted rather than forwarding a browser origin to an IP host.
      const forwarded = await fetch(
        api + new URL(request.url()).pathname.replace("/api/v1", ""),
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": headers["idempotency-key"],
            "X-Actual-Budget-Epoch": headers["x-actual-budget-epoch"],
          },
          body: request.postData(),
        },
      );
      expect(forwarded.status, await forwarded.clone().text()).toBe(201);
      expect(
        (await call(`/budgets/${budget}/banking`)).data.connections,
      ).toHaveLength(1);
      await route.abort("failed");
    },
    { times: 1 },
  );
  await page
    .getByRole("button", { name: "Stage connection", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  const saved = await page.evaluate(
    (b) =>
      new Promise((resolve, reject) => {
        const open = indexedDB.open("actualBudget.banking.v1", 1);
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result,
            r = db.transaction("drafts").objectStore("drafts").get(b);
          r.onsuccess = () => {
            resolve(r.result);
            db.close();
          };
          r.onerror = () => reject(r.error);
        };
      }),
    budget,
  );
  expect(JSON.stringify(saved)).not.toContain(token);
  expect(saved.pending.body.setup_token_sha256).toBe(hash);
  expect(saved.pending.key).toBe(captures[0].key);
  expect(saved.pending.epoch).toBe(captures[0].epoch);
  expect(JSON.parse(captures[0].body).setup_token).toBe(token);
  await page.screenshot({
    path: new URL("recovery-desktop.png", directory).pathname,
    fullPage: true,
  });
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("SimpleFIN setup token", { exact: true }),
  ).toHaveValue("");
  await page
    .getByRole("button", { name: "Check original result", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toHaveCount(0);
  expect(
    (await call(`/budgets/${budget}/banking`)).data.connections,
  ).toHaveLength(1);
  const readSaved = () =>
    page.evaluate(
      (b) =>
        new Promise((resolve, reject) => {
          const open = indexedDB.open("actualBudget.banking.v1", 1);
          open.onerror = () => reject(open.error);
          open.onsuccess = () => {
            const db = open.result,
              r = db.transaction("drafts").objectStore("drafts").get(b);
            r.onsuccess = () => {
              resolve(r.result);
              db.close();
            };
            r.onerror = () => reject(r.error);
          };
        }),
      budget,
    );
  async function failNextSave() {
    await page.evaluate(() => {
      const put = IDBObjectStore.prototype.put;
      let fail = true;
      IDBObjectStore.prototype.put = function (...args) {
        if (fail && this.transaction.db.name === "actualBudget.banking.v1") {
          fail = false;
          throw new DOMException(
            "Synthetic storage limit",
            "QuotaExceededError",
          );
        }
        return put.apply(this, args);
      };
    });
  }
  await page.getByLabel("Connection", { exact: true }).selectOption("");
  await expect(
    page.getByRole("button", { name: "Discard unsent draft", exact: true }),
  ).toBeEnabled();
  await failNextSave();
  await page.getByLabel("Connection name", { exact: true }).fill("Quota draft");
  await expect(page.getByRole("alert")).toContainText(
    "Draft could not be saved",
  );
  const token2 = Buffer.from(
    "https://synthetic.invalid/claim/SECOND-FAKE-TOKEN",
  ).toString("base64");
  await page.getByLabel("SimpleFIN setup token", { exact: true }).fill(token2);
  await expect(
    page.getByRole("button", { name: "Stage connection", exact: true }),
  ).toBeDisabled();
  await page.screenshot({
    path: new URL("quota-failure.png", directory).pathname,
    fullPage: true,
  });
  expect(
    (await call(`/budgets/${budget}/banking`)).data.connections,
  ).toHaveLength(1);
  await page
    .getByRole("button", { name: "Save draft again", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Stage connection", exact: true }),
  ).toBeEnabled();
  let abandoned,
    cancelAttempts = 0;
  const cancellationWires = [];
  await page.route(routePattern, async (route) => {
    const r = route.request(),
      h = r.headers();
    if (h["x-actual-cancel-if-uncommitted"]) {
      cancellationWires.push({
        body: r.postData(),
        key: h["idempotency-key"],
        epoch: h["x-actual-budget-epoch"],
      });
      cancelAttempts++;
      if (cancelAttempts === 1) await route.abort("failed");
      else await route.continue();
    } else {
      abandoned = {
        body: r.postData(),
        key: h["idempotency-key"],
        epoch: h["x-actual-budget-epoch"],
      };
      await route.abort("failed");
    }
  });
  await page
    .getByRole("button", { name: "Stage connection", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  expect(JSON.parse(abandoned.body).name).toBe("Quota draft");
  await page
    .getByRole("button", { name: "Cancel original request", exact: true })
    .click();
  await expect.poll(async () => cancelAttempts).toBe(1);
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  expect((await readSaved()).pending.cancel).toBe(true);
  await page.reload();
  expect((await readSaved()).pending.cancel).toBe(true);
  await page.screenshot({
    path: new URL("cancellation-desktop.png", directory).pathname,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Continue cancellation", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toHaveCount(0);
  expect(cancellationWires).toHaveLength(2);
  expect(cancellationWires[1]).toEqual(cancellationWires[0]);
  expect(cancellationWires[0].key).toBe(abandoned.key);
  expect(cancellationWires[0].epoch).toBe(abandoned.epoch);
  expect(JSON.parse(cancellationWires[0].body).setup_token_sha256).toBe(
    createHash("sha256").update(token2).digest("hex"),
  );
  const delayed = await fetch(api + `/budgets/${budget}/banking/connections`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": abandoned.key,
      "X-Actual-Budget-Epoch": abandoned.epoch,
    },
    body: abandoned.body,
  });
  expect(delayed.status, await delayed.clone().text()).toBe(409);
  expect((await delayed.json()).error.code).toBe("operation_canceled");
  await page.unroute(routePattern);
  expect(
    (await call(`/budgets/${budget}/banking`)).data.connections,
  ).toHaveLength(1);
  await failNextSave();
  await page
    .getByLabel("Connection name", { exact: true })
    .fill("Discard this unsaved name");
  await expect(page.getByRole("alert")).toContainText(
    "Draft could not be saved",
  );
  await page
    .getByRole("button", { name: "Discard unsent draft", exact: true })
    .click();
  await expect(page.getByLabel("Connection name", { exact: true })).toHaveValue(
    "SimpleFIN",
  );
  await expect(
    page.getByLabel("SimpleFIN setup token", { exact: true }),
  ).toHaveValue("");
  await page
    .getByLabel("Connection name", { exact: true })
    .fill("New after discard");
  await page.getByLabel("SimpleFIN setup token", { exact: true }).fill(token2);
  await page
    .getByRole("button", { name: "Stage connection", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(`/budgets/${budget}/banking`)).data.connections.length,
    )
    .toBe(2);
  await expect(
    page.getByRole("button", {
      name: "Refresh connection details",
      exact: true,
    }),
  ).toBeEnabled();
  expect((await readSaved()).pending).toBeNull();
  expect(JSON.stringify(await readSaved())).not.toContain(token2);
  expect(
    await page.evaluate(() =>
      JSON.stringify(Object.fromEntries(Object.entries(localStorage))),
    ),
  ).not.toContain(token2);
  const bankRead = new RegExp(`/api/v1/budgets/${budget}/banking$`);
  await page.route(bankRead, (route) => route.abort("failed"));
  await page.reload();
  await expect(
    page.getByText("Offline review only.", { exact: false }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Claim setup token", exact: true }),
  ).toBeDisabled();
  await page.screenshot({
    path: new URL("offline-desktop.png", directory).pathname,
    fullPage: true,
  });
  await page.unroute(bankRead);
  await page
    .getByRole("button", { name: "Refresh connection details", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Claim setup token", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.getByRole("heading", { name: "New after discard", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");

  await page.screenshot({
    path: new URL("desktop.png", directory).pathname,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: new URL("mobile.png", directory).pathname,
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <=
        document.documentElement.clientWidth,
    ),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS served bank custody UI on LAN HTTP; committed lost ACK, quota fence/discard, durable cancellation, offline cache and privacy without token persistence",
  );
} finally {
  await browser?.close();
  await server.stop();
}
