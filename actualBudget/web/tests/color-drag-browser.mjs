import { chromium, webkit, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const url = server.url.replace("actualbudget.test", "127.0.0.1");
const api = url + "/api/v1";
async function call(path, body, method = body ? "POST" : "GET") {
  const r = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  expect(r.ok).toBe(true);
  return (await r.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    group = randomUUID(),
    category = randomUUID(),
    root = `/budgets/${budget}`;
  await call("/budgets", { id: budget, name: "Color drag" });
  await call(root + "/category-groups", {
    id: group,
    name: "Everyday",
    color: "#397c8c",
  });
  await call(root + "/categories", {
    id: category,
    group_id: group,
    name: "Groceries",
  });
  browser =
    process.env.PWA_BROWSER === "webkit"
      ? await webkit.launch()
      : await chromium.launch({
          executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
          args: ["--no-sandbox"],
        });
  const page = await browser.newPage();
  await page.goto(url);
  await page.getByRole("button", { name: "Budget", exact: true }).click();
  await page.getByLabel("Months displayed").selectOption("1");
  for (const [kind, id, name] of [
    ["categories", category, "Groceries"],
    ["category-groups", group, "Everyday"],
  ]) {
    let release, started;
    const held = new Promise((r) => (release = r));
    const requestStarted = new Promise((r) => (started = r));
    const path = `**/api/v1${root}/${kind}/${id}`;
    let writes = 0;
    await page.route(path, async (route) => {
      if (route.request().method() !== "PATCH") return route.continue();
      if (++writes === 1) {
        started();
        await held;
      }
      await route.continue();
    });
    await page
      .getByRole("button", { name: `Choose color for ${name}`, exact: true })
      .click();
    const input = page.getByLabel(`Color for ${name}`, { exact: true });
    await input.fill("#112233");
    await requestStarted;
    try {
      // Safari emits change events while dragging. Disabling the native input closes its picker.
      await expect(input).toBeEnabled();
      await input.fill("#445566");
      await input.fill("#778899");
      await expect(input).toHaveValue("#778899");
    } finally {
      release();
    }
    await expect
      .poll(async () => (await call(`${root}/${kind}/${id}`)).color)
      .toBe("#778899");
    await expect(input).toBeEnabled();
    await expect(input).toHaveValue("#778899");
    await page.unroute(path);
    await page.keyboard.press("Escape");
  }
  await page
    .getByRole("button", { name: "Choose color for Groceries", exact: true })
    .click();
  const palette = page.getByRole("dialog", {
    name: "Color palette for Groceries",
    exact: true,
  });
  await palette.getByRole("button", { name: "Teal", exact: true }).click();
  await expect
    .poll(async () => (await call(`${root}/categories/${category}`)).color)
    .toBe("#169d98");
  await expect(palette).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(palette).not.toBeVisible();
  await page
    .getByRole("button", { name: "Choose color for Groceries", exact: true })
    .click();
  const input = page.getByLabel("Color for Groceries", { exact: true });
  const colorPath = `${root}/categories/${category}`;
  for (const outcome of ["conflict", "cancel"]) {
    let release, started;
    const held = new Promise((r) => (release = r)),
      requestStarted = new Promise((r) => (started = r));
    const writes = [];
    await page.route(`**/api/v1${colorPath}`, async (route) => {
      if (route.request().method() !== "PATCH") return route.continue();
      writes.push(route.request().postDataJSON());
      if (writes.length === 1) {
        started();
        await held;
        if (outcome === "cancel") return route.abort();
      }
      return route.continue();
    });
    await input.fill("#123456");
    await requestStarted;
    await input.fill("#654321");
    if (outcome === "conflict")
      await call(colorPath, { color: "#abcdef" }, "PATCH");
    release();
    if (outcome === "conflict") {
      await expect(
        page.getByRole("alert").filter({ hasText: "A newer color was kept" }),
      ).toBeVisible();
    } else {
      await expect(
        page.getByRole("button", { name: "Retry color", exact: true }),
      ).toBeEnabled();
      await expect(palette).not.toBeVisible();
      await page
        .getByRole("button", { name: "Cancel color change", exact: true })
        .click();
      await expect(
        page.getByRole("button", { name: "Retry color", exact: true }),
      ).toHaveCount(0);
    }
    await expect(input).toHaveValue("#abcdef");
    expect((await call(colorPath)).color).toBe("#abcdef");
    expect(writes.some((body) => body?.color === "#654321")).toBe(false);
    await page.unroute(`**/api/v1${colorPath}`);
  }
  console.log(
    "PASS color controls stay enabled through drag events and persist the final shade",
  );
} finally {
  await browser?.close();
  await server.stop();
}
