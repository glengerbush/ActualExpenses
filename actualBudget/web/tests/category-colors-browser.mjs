import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile, readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { startServer } from "./server.mjs";
const directory = await mkdtemp(tmpdir() + "/category-colors-");
let server = await startServer({ dataDirectory: directory }),
  browser;
const endpoint = () =>
  server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function req(path, body, method = "POST") {
  return fetch(endpoint() + path, {
    method: body === undefined ? "GET" : method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
async function call(path, body, method) {
  const r = await req(path, body, method);
  expect(r.ok, await r.clone().text()).toBe(true);
  return (await r.json()).data;
}
try {
  const budget = randomUUID(),
    account = randomUUID(),
    group = randomUUID(),
    other = randomUUID();
  const root = "/budgets/" + budget,
    today = new Date().toISOString().slice(0, 10);
  const food = "10000000-0000-4000-8000-000000000001",
    dining = "10000000-0000-4000-8000-000000000002";
  await call("/budgets", { id: budget, name: "Color groups" });
  await call(root + "/accounts", {
    id: account,
    name: "History",
    opening_balance: 0,
    opening_date: today,
  });
  await call(root + "/category-groups", {
    id: group,
    name: "Everyday",
    color: "#397C8C",
    sort_order: 0,
  });
  await call(root + "/category-groups", {
    id: other,
    name: "Home",
    color: "#A66B42",
    sort_order: 1,
  });
  await call(root + "/categories", {
    id: food,
    name: "Groceries",
    group_id: group,
    sort_order: 0,
  });
  await call(root + "/categories", {
    id: dining,
    name: "Dining",
    group_id: group,
    sort_order: 1,
  });
  const rent = randomUUID();
  await call(root + "/categories", { id: rent, name: "Rent", group_id: other });
  for (const [category, amount, notes] of [
    [food, -2500, "Market #oldtag"],
    [dining, -1800, "Lunch"],
    [rent, -95000, "November rent"],
  ])
    await call(root + "/transactions", {
      id: randomUUID(),
      account_id: account,
      date: today,
      category_id: category,
      amount,
      notes,
    });
  const auto = (await call(root + "/categories/" + food)).resolved_color;
  expect(auto).not.toBe(
    (await call(root + "/categories/" + dining)).resolved_color,
  );
  await call(root + "/categories/" + food, { color: "#C44071" }, "PATCH");
  await call(root + "/category-groups/" + group, { color: "#4466AA" }, "PATCH");
  expect((await call(root + "/categories/" + food)).resolved_color).toBe(
    "#C44071",
  );
  expect(
    (await call(root + "/categories/" + dining)).category_group_color,
  ).toBe("#4466AA");
  expect(
    (await req(root + "/category-groups/" + group, { color: "red" }, "PATCH"))
      .status,
  ).toBe(400);
  expect((await req(root + "/tags")).status).toBe(404);
  expect(
    (await req(root + "/tags", { id: randomUUID(), tag: "test" })).status,
  ).toBe(404);
  expect(
    (await call(root + "/transactions")).every((t) => t.notes === ""),
  ).toBe(true);
  const backup = await call(root + "/backups", { label: "Color backup" });
  const zip = Buffer.from(
    await (
      await fetch(endpoint() + root + "/backups/" + backup.id + "/download")
    ).arrayBuffer(),
  );
  await writeFile(directory + "/colors.zip", zip);
  // Older archives gain nullable color fields without dropping any categories.
  execFileSync("python3", [
    "-c",
    `import json,zipfile,hashlib,sys
p=sys.argv[1]
with zipfile.ZipFile(p+'/colors.zip') as z: files={n:z.read(n) for n in z.namelist()}
m=json.loads(files['manifest.json']);b=json.loads(files['budget.json'])
for name in ['categories','category_groups']:
 t=b['tables'][name];i=t['columns'].index('color');t['columns'].pop(i)
 for row in t['rows']:row.pop(i)
files['budget.json']=json.dumps(b).encode();m['schema_version']=28;m['entries']['budget.json']={'sha256':hashlib.sha256(files['budget.json']).hexdigest(),'bytes':len(files['budget.json'])};files['manifest.json']=json.dumps(m).encode()
with zipfile.ZipFile(p+'/old.zip','w',zipfile.ZIP_DEFLATED) as z:
 for name,data in files.items():z.writestr(name,data)`,
    directory,
  ]);
  for (const file of ["colors.zip", "old.zip"]) {
    const r = await fetch(endpoint() + "/restores", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": randomUUID(),
      },
      body: await readFile(directory + "/" + file),
    });
    expect(r.ok, await r.clone().text()).toBe(true);
  }
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1100 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  const row = page
    .locator(".register-table tbody tr")
    .filter({ hasText: "Groceries" });
  await expect(row).toHaveCSS("--category-color", "#C44071");
  await mkdir("test-results/category-colors", { recursive: true });
  await page.screenshot({
    path: "test-results/category-colors/transactions.png",
  });
  await page.getByRole("button", { name: "Payees", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Tags", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Months displayed", { exact: true }).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await expect(
    page.locator(`.budget-group[data-id="${group}"]`).first(),
  ).toBeVisible();
  await expect(
    page.locator(`.budget-category[data-id="${food}"]`).first(),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Toggle Everyday", exact: true })
    .click();
  await expect(
    page.locator(`.budget-category[data-id="${food}"]`).first(),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Toggle Everyday", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Choose color for Everyday", exact: true })
    .click();
  await page.getByLabel("Color for Everyday", { exact: true }).fill("#558844");
  await expect
    .poll(async () => (await call(root + "/category-groups/" + group)).color)
    .toBe("#558844");
  await expect(
    page.getByLabel("Color for Everyday", { exact: true }),
  ).toBeEnabled();

  await expect(
    page.getByLabel("Use automatic shade for Groceries", { exact: true }),
  ).toHaveCount(0);
  // A committed color whose response is lost keeps the exact operation for retry.
  let attempted;
  const colorPath = `**/api/v1${root}/categories/${food}`;
  await page.route(colorPath, async (route) => {
    if (route.request().method() !== "PATCH") return route.continue();
    attempted = {
      body: route.request().postData(),
      key: route.request().headers()["idempotency-key"],
    };
    const forwarded = new URL(route.request().url());
    forwarded.hostname = "127.0.0.1";
    await route.fetch({
      url: forwarded.href,
      headers: {
        ...route.request().headers(),
        origin: forwarded.origin,
        host: forwarded.host,
      },
    });
    await route.abort();
  });
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", { name: "Choose color for Groceries", exact: true })
    .click();
  await page.getByLabel("Color for Groceries", { exact: true }).fill("#337755");
  await expect(
    page.getByRole("button", { name: "Retry color", exact: true }),
  ).toBeEnabled();
  await page.unroute(colorPath);
  let retried;
  page.on("request", (r) => {
    if (
      r.method() === "PATCH" &&
      r.url().endsWith(root + "/categories/" + food)
    )
      retried = { body: r.postData(), key: r.headers()["idempotency-key"] };
  });
  await page.getByRole("button", { name: "Retry color", exact: true }).click();
  await expect(
    page.getByLabel("Color for Groceries", { exact: true }),
  ).toBeEnabled();
  expect(retried).toEqual(attempted);
  expect((await call(root + "/categories/" + food)).color).toBe("#337755");

  await expect(
    page.getByLabel("Use automatic shade for Groceries", { exact: true }),
  ).toHaveCount(0);
  await page.emulateMedia({ colorScheme: "dark" });
  await page.screenshot({
    path: "test-results/category-colors/dark.png",
    fullPage: true,
  });
  await page.emulateMedia({ colorScheme: "light" });
  await page.screenshot({
    path: "test-results/category-colors/budget.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/category-colors/mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.reload();
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Months displayed", { exact: true }).selectOption("1");
  await expect(page.locator(".budget-month")).toHaveCount(1);
  await expect(
    page.getByLabel("Color for Everyday", { exact: true }),
  ).toHaveValue("#558844");
  expect(errors).toEqual([]);
  // Simulate a pre-color database, restart through the migration, and verify retention.
  await browser.close();
  browser = null;
  await server.stop();
  execFileSync("python3", [
    "tests/historical-schema.py",
    directory + "/actualbudget.sqlite3",
    "28",
  ]);
  server = await startServer({ dataDirectory: directory });
  expect((await call(root + "/categories/" + food)).resolved_color).toMatch(
    /^#[0-9A-F]{6}$/i,
  );
  expect(
    (await call(root + "/transactions")).find((t) => t.category_id === food)
      .amount,
  ).toBe(-2500);
  console.log(
    "PASS category shades, group changes, overrides/reset, grouped budget, tag retirement, archives, migration and mobile",
  );
} finally {
  await browser?.close();
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}
