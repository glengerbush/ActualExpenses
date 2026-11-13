import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, method = body === undefined ? "GET" : "POST") {
  const response = await fetch(api + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return (await response.json()).data;
}
let browser;
try {
  const budget = randomUUID(),
    root = `/budgets/${budget}`,
    groups = [randomUUID(), randomUUID(), randomUUID()],
    cats = [randomUUID(), randomUUID(), randomUUID()];
  await call("/budgets", { id: budget, name: "Ordering budget" });
  await call(root + "/accounts", {
    id: randomUUID(),
    name: "History",
    opening_balance: 0,
    opening_date: "2026-11-01",
  });
  for (const [i, name] of ["Everyday", "Home", "Travel"].entries())
    await call(root + "/category-groups", {
      id: groups[i],
      name,
      sort_order: i,
    });
  for (const [i, name] of ["Apples", "Bread", "Coffee"].entries())
    await call(root + "/categories", {
      id: cats[i],
      name,
      group_id: groups[0],
      sort_order: i,
    });
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH,
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const p = await browser.newPage({ viewport: { width: 1600, height: 1100 } });
  const errors = [];
  p.on("pageerror", (e) => errors.push(e.message));
  await p.goto(server.url);
  await p.getByText("Ordering budget", { exact: true }).click();
  const nav = p.getByRole("complementary", { name: "Budget navigation" });
  await nav.getByRole("button", { name: "Budget", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Close budget view", exact: true }),
  ).toHaveCount(0);
  const table = () => p.locator(".month-grid table").first();
  // Keyboard moves use the same persisted path as pointer drops.
  await expect(
    p.getByRole("button", { name: "Move group Home", exact: true }).first(),
  ).toBeEnabled();
  await p
    .getByRole("button", { name: "Move group Home", exact: true })
    .first()
    .press("ArrowUp");
  await expect
    .poll(async () => (await call(root + "/category-groups")).map((g) => g.id))
    .toEqual([groups[1], groups[0], groups[2]]);
  await expect(
    p.getByRole("button", { name: "Move group Travel", exact: true }).first(),
  ).toBeEnabled();
  const source = await p
    .getByRole("button", { name: "Move group Travel", exact: true })
    .first()
    .boundingBox();
  const destination = await p
    .locator(`[data-budget-kind="category-groups"][data-id="${groups[1]}"]`)
    .first()
    .boundingBox();
  await p.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
  await p.mouse.down();
  await p.mouse.move(destination.x + 65, destination.y + 5, { steps: 12 });
  await p.mouse.up();
  await expect
    .poll(async () => (await call(root + "/category-groups")).map((g) => g.id))
    .toEqual([groups[2], groups[1], groups[0]]);
  // Categories can move into another matching group.
  await expect(
    p
      .getByRole("button", { name: "Move category Coffee", exact: true })
      .first(),
  ).toBeEnabled();
  const csource = await p
    .getByRole("button", { name: "Move category Coffee", exact: true })
    .first()
    .boundingBox();
  const cdestination = await p
    .locator(`[data-budget-kind="category-groups"][data-id="${groups[1]}"]`)
    .first()
    .boundingBox();
  await p.mouse.move(csource.x + 14, csource.y + 20);
  await p.mouse.down();
  await p.mouse.move(cdestination.x + 65, cdestination.y + 8, { steps: 12 });
  await p.mouse.up();
  await expect
    .poll(async () => (await call(root + "/categories/" + cats[2])).group_id)
    .toBe(groups[1]);
  await expect(
    p.getByRole("button", { name: "Move category Bread", exact: true }).first(),
  ).toBeEnabled();
  await p
    .getByRole("button", { name: "Move category Bread", exact: true })
    .first()
    .press("ArrowUp");
  await expect
    .poll(async () =>
      (await call(root + "/categories"))
        .filter((c) => c.group_id === groups[0])
        .map((c) => c.id),
    )
    .toEqual([cats[1], cats[0]]);
  // Rename inline, and persist color independently.
  const name = p
    .getByRole("textbox", { name: "Group name Home", exact: true })
    .first();
  await expect(name).toBeEnabled();
  await name.fill("Household");
  await name.press("Enter");
  await expect
    .poll(async () => (await call(root + "/category-groups/" + groups[1])).name)
    .toBe("Household");
  await expect(
    p.getByLabel("Color for Household", { exact: true }).first(),
  ).toBeEnabled();
  await p
    .getByLabel("Color for Household", { exact: true })
    .first()
    .fill("#ad274e");
  await expect
    .poll(
      async () => (await call(root + "/category-groups/" + groups[1])).color,
    )
    .toBe("#ad274e");
  await p.reload();
  await expect(
    p
      .getByRole("textbox", { name: "Group name Household", exact: true })
      .first(),
  ).toBeVisible();
  await p.screenshot({
    path: "../../.scratch/budget-drag-inline.png",
    fullPage: true,
  });
  // Same tab does not add duplicate history; reload keeps its destination.
  await nav.getByRole("button", { name: "Payees", exact: true }).click();
  await nav
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await nav
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
  await p.goBack();
  await expect(
    nav.getByRole("button", { name: "Payees", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await p.goBack();
  await expect(
    nav.getByRole("button", { name: "Budget", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await p.goForward();
  await expect(
    nav.getByRole("button", { name: "Payees", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await p.reload();
  await expect(
    nav.getByRole("button", { name: "Payees", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await nav
    .getByRole("button", { name: "Dashboards and reports", exact: true })
    .click();
  await p.getByRole("button", { name: "Custom reports", exact: true }).click();
  await p.getByRole("button", { name: "Dashboards", exact: true }).click();
  await p.goBack();
  await expect(
    p.getByRole("button", { name: "Custom reports", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  // Unknown response: preserve the exact rename request across reload.
  await nav.getByRole("button", { name: "Budget", exact: true }).click();
  let originalKey;
  await p.route(
    "**/api/v1" + root + "/category-groups/" + groups[1],
    async (route) => {
      if (route.request().method() !== "PATCH") return route.continue();
      originalKey = route.request().headers()["idempotency-key"];
      const response = await route.fetch({
        url: route.request().url().replace("actualbudget.test", "127.0.0.1"),
        headers: {
          ...route.request().headers(),
          host: new URL(route.request().url()).host,
        },
      });
      expect(response.ok(), await response.text()).toBe(true);
      await route.abort("failed");
    },
  );
  const rename = p
    .getByRole("textbox", { name: "Group name Household", exact: true })
    .first();
  await rename.fill("House");
  await rename.press("Enter");
  await expect(
    p.getByRole("button", {
      name: "Retry saved group/category edit",
      exact: true,
    }),
  ).toBeEnabled();
  await p.unroute("**/api/v1" + root + "/category-groups/" + groups[1]);
  await p.reload();
  let retryKey;
  p.on("request", (r) => {
    if (r.method() === "PATCH" && r.url().endsWith(groups[1]))
      retryKey = r.headers()["idempotency-key"];
  });
  await p
    .getByRole("button", {
      name: "Retry saved group/category edit",
      exact: true,
    })
    .click();
  await expect(
    p.getByRole("textbox", { name: "Group name House", exact: true }).first(),
  ).toBeEnabled();
  expect(retryKey).toBe(originalKey);
  // Imported groups may share order numbers: rebalance without losing rows.
  for (const id of groups)
    await call(root + "/category-groups/" + id, { sort_order: 0 }, "PATCH");
  await p.reload();
  await expect(
    p.getByRole("button", { name: "Move group Travel", exact: true }).first(),
  ).toBeEnabled();
  await p
    .getByRole("button", { name: "Move group Travel", exact: true })
    .first()
    .press("ArrowUp");
  await expect
    .poll(async () => (await call(root + "/category-groups")).map((g) => g.id))
    .toEqual([groups[0], groups[2], groups[1]]);
  // Real touch events exercise pointer capture, including the scrollable mobile table.
  await p.setViewportSize({ width: 390, height: 844 });
  const touchHandle = p
    .getByRole("button", { name: "Move group Travel", exact: true })
    .first();
  await expect(touchHandle).toBeEnabled();
  await touchHandle.scrollIntoViewIfNeeded();
  const touchStart = await touchHandle.boundingBox();
  const touchEnd = await p
    .locator(`[data-budget-kind="category-groups"][data-id="${groups[1]}"]`)
    .first()
    .boundingBox();
  const cdp = await p.context().newCDPSession(p);
  await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: true });
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: touchStart.x + 14, y: touchStart.y + 22 }],
  });
  for (let i = 1; i <= 8; i++)
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [
        {
          x: touchStart.x + 14,
          y:
            touchStart.y +
            22 +
            ((touchEnd.y + touchEnd.height - 5 - touchStart.y - 22) * i) / 8,
        },
      ],
    });
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await expect
    .poll(async () => (await call(root + "/category-groups")).map((g) => g.id))
    .toEqual([groups[0], groups[1], groups[2]]);
  await cdp.detach();

  await p.screenshot({
    path: "../../.scratch/budget-drag-mobile.png",
    fullPage: true,
  });
  expect(
    await p.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBe(true);
  expect(errors).toEqual([]);
  console.log(
    "PASS persisted group/category ordering, inline name/color, lost-response recovery, browser history and mobile layout",
  );
} finally {
  await browser?.close();
  await server.stop();
}
