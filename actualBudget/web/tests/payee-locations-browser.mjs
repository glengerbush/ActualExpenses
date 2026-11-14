import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer(),
  url = server.url.replace("actualbudget.test", "localhost"),
  api = url + "/api/v1";
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../tests/fixtures/payee-locations-import.json",
      import.meta.url,
    ),
  ),
);
const evidence = new URL(
  "../../../.scratch/evidence/locations46/",
  import.meta.url,
);
await mkdir(evidence, { recursive: true });
async function write(path, body, method = "POST") {
  const r = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
async function read(path) {
  const r = await fetch(api + path);
  expect(r.ok).toBe(true);
  return (await r.json()).data;
}
const budget = randomUUID(),
  account = randomUUID(),
  root = `/budgets/${budget}`;
let browser;
try {
  await write("/budgets", { id: budget, name: "Location fixture" });
  await write(root + "/accounts", {
    id: account,
    name: "Daily",
    opening_balance: 0,
    opening_date: "2026-01-01",
  });
  for (const [i, id] of fixture.payees.entries())
    await write(root + "/payees", {
      id,
      name: `Shop ${String(i).padStart(2, "0")}`,
    });
  for (const row of fixture.locations)
    await write(root + "/payee-locations", row);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    permissions: ["geolocation"],
    geolocation: { latitude: 40, longitude: -74 },
  });
  await context.addInitScript(() => {
    window.geoCalls = 0;
    const original = navigator.geolocation.getCurrentPosition.bind(
      navigator.geolocation,
    );
    navigator.geolocation.getCurrentPosition = (...args) => {
      window.geoCalls++;
      return original(...args);
    };
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log("PAGEERROR", e.message));
  const external = [];
  page.on("request", (r) => {
    if (!r.url().startsWith(url) && !r.url().startsWith("data:"))
      external.push(r.url());
  });
  await page.goto(url);

  // The inline composer does not expose location capture. Retained metadata and
  // released-client work must survive ordinary editing without implicit writes.
  const savedLocations = await read(root + "/payee-locations");
  const epoch = (await read(root + "/checkpoint")).epoch;
  const work = {
    version: 1,
    budget,
    revision: randomUUID(),
    pending: {
      path: root + "/payee-locations",
      method: "POST",
      key: randomUUID(),
      clock: new Date().toISOString() + "-0000-0000000000000001",
      epoch,
      body: {
        id: randomUUID(),
        payee_id: fixture.payees[0],
        latitude: 41,
        longitude: -75,
      },
    },
  };
  const retained = JSON.stringify(work);
  await page.evaluate(
    ({ budget, retained }) =>
      localStorage.setItem(`actualBudget.locations.v1.${budget}`, retained),
    { budget, retained },
  );
  let locationRequests = 0;
  page.on("request", (request) => {
    if (
      request.url().includes("/payee-locations") &&
      request.method() !== "GET"
    )
      locationRequests++;
  });
  for (const [amount, offline] of [
    ["1.23", false],
    ["2.34", true],
  ]) {
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await expect(
      page.getByLabel("Payee locations", { exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Request location", exact: true }),
    ).toHaveCount(0);
    await page.getByLabel("Amount", { exact: true }).fill(amount);
    if (offline) await context.setOffline(true);
    await page.getByLabel("Amount", { exact: true }).press("Enter");
    await expect(page.locator(".inline-editor")).toHaveCount(0);
    await expect(
      page.locator("td.amount").filter({ hasText: "-" + amount }),
    ).toBeVisible();
    await context.setOffline(false);
    await expect
      .poll(async () =>
        (await read(root + "/transactions")).some(
          (row) => row.amount === -Math.round(Number(amount) * 100),
        ),
      )
      .toBe(true);
  }
  await page.reload();
  for (const width of [390, 1280]) {
    await page.setViewportSize({ width, height: 900 });
    await expect(page.getByTestId("register-total")).toHaveText("-3.57");
    await page.screenshot({
      path: new URL(`entry-${width}.png`, evidence).pathname,
      fullPage: true,
    });
  }
  expect(await page.evaluate(() => window.geoCalls)).toBe(0);
  expect(
    await page.evaluate(
      (budget) => localStorage.getItem(`actualBudget.locations.v1.${budget}`),
      budget,
    ),
  ).toBe(retained);
  expect(await read(root + "/payee-locations")).toEqual(savedLocations);
  expect(locationRequests).toBe(0);
  expect(external).toEqual([]);
  console.log(
    "PASS location metadata/journal preservation, no implicit capture, online/offline financial entry, mobile/desktop",
  );
} finally {
  await browser?.close();
  await server.stop();
}
