import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(
  path,
  body,
  method = body === undefined ? "GET" : "POST",
  key = randomUUID(),
) {
  const r = await fetch(api + path, {
    method,
    headers: { "Content-Type": "application/json", "Idempotency-Key": key },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
async function stage(name) {
  const bytes = await readFile(
    new URL("../../tests/fixtures/" + name, import.meta.url),
  );
  const r = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const target = randomUUID();
  await call("/budgets", { id: target, name: "Household import" });
  const ledger = await call(`/budgets/${target}/ledger`, {});
  const existing = randomUUID();
  await call(`/budgets/${target}/transactions`, {
    id: existing,
    account_id: ledger.id,
    date: "2026-11-01",
    amount: -1234,
  });
  const prefs = await call(`/budgets/${target}/preferences`);
  const epoch = (await call(`/budgets/${target}/checkpoint`)).epoch;
  for (const file of [
    "import-26.10.0.zip",
    "import-splits-26.10.0.zip",
    "import-transfers-26.10.0.zip",
    "split-transfers-26.10.0.zip",
  ]) {
    const source = await stage(file);
    const activated = await call(`/imports/${source.id}/activate`, {});
    const before = await call(`/budgets/${target}/transactions?limit=1000`);
    const sourceRows = await call(
      `/budgets/${activated.id}/transactions?limit=1000`,
    );
    const key = randomUUID(),
      body = { budget_id: target, epoch };
    const first = await call(`/imports/${source.id}/merge`, body, "POST", key);
    const replay = await call(`/imports/${source.id}/merge`, body, "POST", key);
    expect(replay).toEqual(first);
    const again = await call(`/imports/${source.id}/merge`, body);
    expect(again.import_result.added).toBe(0);
    const after = await call(`/budgets/${target}/transactions?limit=1000`);
    // Different fixtures can share source IDs; only genuinely new rows are additive.
    expect(after.find((t) => t.id === existing).amount).toBe(-1234);
    expect(after.length).toBeGreaterThanOrEqual(before.length);
    if (file === "import-26.10.0.zip") {
      expect(after.length).toBe(before.length + sourceRows.length);
      expect(after.reduce((n, t) => n + t.amount, 0)).toBe(
        before.reduce((n, t) => n + t.amount, 0) +
          sourceRows.reduce((n, t) => n + t.amount, 0),
      );
    }
    const mergedTx = new Map(after.map((t) => [t.id, t]));
    for (const t of after.filter((t) => t.transfer_id && !t.parent_id)) {
      const peer = mergedTx.get(t.transfer_id);
      if (peer) {
        expect(peer.transfer_id).toBe(t.id);
        expect(peer.amount).toBe(-t.amount);
      }
    }
    // Importing into the full import of the same source also preserves its existing rows.
    const ownEpoch = (await call(`/budgets/${activated.id}/checkpoint`)).epoch;
    expect(
      (
        await call(`/imports/${source.id}/merge`, {
          budget_id: activated.id,
          epoch: ownEpoch,
        })
      ).import_result.added,
    ).toBe(0);
  }
  expect(await call(`/budgets/${target}/preferences`)).toEqual(prefs);
  // Reference matches and merge receipts survive renaming and native backup/restore.
  const refs = await call(`/budgets/${target}/categories`);
  if (refs.length)
    await call(
      `/budgets/${target}/categories/${refs[0].id}`,
      { name: "Renamed after merge" },
      "PATCH",
    );
  const point = await call(`/budgets/${target}/backups`, {
    label: "After archive merges",
  });
  await call(`/budgets/${target}/backups/${point.id}/restore`, {
    confirm_name: "Household import",
  });
  const restoredEpoch = (await call(`/budgets/${target}/checkpoint`)).epoch;
  const restoredSource = await stage("import-26.10.0.zip");
  expect(
    (
      await call(`/imports/${restoredSource.id}/merge`, {
        budget_id: target,
        epoch: restoredEpoch,
      })
    ).import_result.added,
  ).toBe(0);
  expect((await call(`/budgets/${target}/categories`)).length).toBe(
    refs.length,
  );

  const source = await stage("import-26.10.0.zip");
  const bad = await fetch(api + `/imports/${source.id}/merge`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify({ budget_id: target, epoch: "0".repeat(32) }),
  });
  expect(bad.status).toBe(409);
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const p = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  p.on("pageerror", (e) => errors.push(e.message));
  await p.goto(server.url);
  await p.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Budgets", exact: true }),
  ).toBeVisible();
  await expect(
    p.getByText("Experimental features", { exact: true }),
  ).toHaveCount(0);
  await expect(p.getByLabel("Schedule for all budgets")).toBeEnabled();
  await p.getByLabel("Schedule for all budgets").selectOption("0");
  await expect
    .poll(async () => (await call("/backup-policy")).interval_seconds)
    .toBe(0);
  await p.getByLabel("Schedule for all budgets").selectOption("604800");
  await expect
    .poll(async () => (await call("/backup-policy")).interval_seconds)
    .toBe(604800);
  await p.getByRole("button", { name: "Budgets", exact: true }).click();
  await p.getByText("Household import", { exact: true }).click();
  const sidebar = p.getByRole("complementary", { name: "Budget navigation" });
  await expect(
    sidebar.getByRole("button", { name: "Transaction files", exact: true }),
  ).toHaveCount(0);
  await expect(
    sidebar.getByRole("button", { name: "Recovery tools", exact: true }),
  ).toHaveCount(0);
  await sidebar.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = p.getByRole("navigation", { name: "Settings sections" });
  await settings.getByRole("button", { name: "Recovery", exact: true }).click();
  await expect(
    p.getByRole("heading", { name: "Recovery tools", exact: true }),
  ).toBeVisible();
  await settings.getByRole("button", { name: "Import", exact: true }).click();
  await p
    .getByRole("button", { name: "Transaction file", exact: true })
    .click();
  await p.getByLabel("An existing budget", { exact: true }).check();
  await p.getByLabel("Budget", { exact: true }).selectOption(target);
  await p
    .getByRole("button", {
      name: "Continue to transaction import",
      exact: true,
    })
    .click();
  async function importCsv(name, payee) {
    await p.getByLabel("Transaction file", { exact: true }).setInputFiles({
      name,
      mimeType: "text/csv",
      buffer: Buffer.from(`Date,Payee,Amount\n2026-11-10,${payee},-7.25\n`),
    });
    await p.getByRole("button", { name: "Read file", exact: true }).click();
    for (const field of ["Date", "Payee", "Amount"])
      await p
        .getByLabel(`${field} column`, { exact: false })
        .selectOption(field);
    await p
      .getByRole("button", { name: "Preview selected import", exact: true })
      .click();
    await p
      .getByRole("button", { name: "Import reviewed records", exact: true })
      .click();
    await expect(p.getByText(/Import saved:/)).toBeVisible();
  }
  await importCsv("existing.csv", "CSV Existing");
  expect(
    (await call(`/budgets/${target}/transactions?limit=1000`)).some(
      (t) => t.payee_name?.toLowerCase() === "csv existing",
    ),
  ).toBe(true);
  await settings.getByRole("button", { name: "Import", exact: true }).click();
  await p
    .getByRole("button", { name: "Transaction file", exact: true })
    .click();
  await p.getByLabel("A new budget", { exact: true }).check();
  await p.getByRole("button", { name: "Name new budget", exact: true }).click();
  await p.getByLabel("Budget name", { exact: true }).fill("CSV Separate");
  await p.getByRole("button", { name: "Create budget", exact: true }).click();
  await importCsv("separate.csv", "CSV New");
  const created = (await call("/budgets")).find(
    (b) => b.name === "CSV Separate",
  );
  expect(created).toBeTruthy();
  expect(
    (await call(`/budgets/${created.id}/transactions`)).some(
      (t) => t.payee_name?.toLowerCase() === "csv new",
    ),
  ).toBe(true);
  await settings.getByRole("button", { name: "Import", exact: true }).click();
  await p
    .getByLabel("Actual ZIP export", { exact: true })
    .setInputFiles(
      new URL("../../tests/fixtures/import-26.10.0.zip", import.meta.url)
        .pathname,
    );
  await p.getByRole("button", { name: "Review archive", exact: true }).click();
  await p.getByLabel("An existing budget", { exact: true }).check();
  await p.getByLabel("Budget", { exact: true }).selectOption(created.id);
  await p.screenshot({
    path: "../../.scratch/settings-import-archive.png",
    fullPage: true,
  });
  // The acknowledged merge is retried after reload using its saved target and operation identity.
  let key;
  await p.route("**/api/v1/imports/*/merge", async (route) => {
    key = route.request().headers()["idempotency-key"];
    const response = await route.fetch({
      url: route.request().url().replace("actualbudget.test", "127.0.0.1"),
      headers: {
        ...route.request().headers(),
        host: new URL(route.request().url()).host,
      },
    });
    expect(response.ok(), await response.text()).toBe(true);
    await route.abort("failed");
  });
  await p
    .getByRole("button", { name: "Merge into selected budget", exact: true })
    .click();
  await expect(
    p.getByRole("button", { name: "Retry saved import", exact: true }),
  ).toBeEnabled();
  const count = (await call(`/budgets/${created.id}/transactions?limit=1000`))
    .length;
  await p.unroute("**/api/v1/imports/*/merge");
  await p.reload();
  let retried;
  p.on("request", (r) => {
    if (r.url().endsWith("/merge")) retried = r.headers()["idempotency-key"];
  });
  await p
    .getByRole("button", { name: "Retry saved import", exact: true })
    .click();
  await expect(
    sidebar.getByRole("button", { name: "Transaction history", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  expect(retried).toBe(key);
  expect(
    (await call(`/budgets/${created.id}/transactions?limit=1000`)).length,
  ).toBe(count);
  await sidebar.getByRole("button", { name: "Settings", exact: true }).click();
  await settings.getByRole("button", { name: "Import", exact: true }).click();
  await p.setViewportSize({ width: 390, height: 844 });
  await p.screenshot({
    path: "../../.scratch/settings-import-mobile.png",
    fullPage: true,
  });
  expect(
    await p.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS archive graph merges, repeat/replay/epoch protection, settings navigation, CSV new/existing destinations, archive retry and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
}
