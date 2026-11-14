import { chromium, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { startServer } from "./server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("./fixtures/date-input-26.10.0.json", import.meta.url),
    "utf8",
  ),
);
const server = await startServer();
const api = server.url.replace("actualbudget.test", "127.0.0.1");
async function write(path, body, method = "POST") {
  const response = await fetch(api + "/api/v1" + path, {
    method,
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    body: JSON.stringify(body),
  });
  expect(response.ok).toBe(true);
  return (await response.json()).data;
}
let browser;
try {
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const errors = [];
  for (const format of [...new Set(fixture.cases.map((c) => c.format))]) {
    const context = await browser.newContext({ timezoneId: "UTC" });
    const page = await context.newPage();
    page.on("pageerror", (e) => errors.push(e.message));
    // Pin only the calendar reference used by shorthand/two-digit date parsing.
    // Date.now and explicit Date(timestamp) remain real for observed server HLCs.
    await page.addInitScript((reference) => {
      const NativeDate = Date;
      const calendar = NativeDate.parse(reference + "T12:00:00Z");
      globalThis.Date = new Proxy(NativeDate, {
        construct(target, args, newTarget) {
          return Reflect.construct(
            target,
            args.length ? args : [calendar],
            newTarget,
          );
        },
      });
    }, fixture.reference);
    const budget = randomUUID(),
      account = randomUUID();
    await write("/budgets", { id: budget, name: "Dates " + format });
    await write(`/budgets/${budget}/accounts`, {
      id: account,
      name: "Date account",
      opening_balance: 0,
      opening_date: "2026-01-01",
    });
    await write(
      `/budgets/${budget}/preferences`,
      { date_format: format },
      "PATCH",
    );
    await page.goto(server.url);
    expect(
      await page.evaluate(() => new Date().toISOString().slice(0, 10)),
    ).toBe(fixture.reference);
    expect(
      Math.abs((await page.evaluate(() => Date.now())) - Date.now()),
    ).toBeLessThan(60_000);
    await page.evaluate(
      ({ budget, account }) =>
        localStorage.setItem(
          "actualBudget.selection",
          JSON.stringify({ budgetId: budget, accountId: account }),
        ),
      { budget, account },
    );
    await page.reload();
    await expect(page.getByLabel("Show through date")).toHaveAttribute(
      "placeholder",
      format.toUpperCase(),
    );
    for (const [index, item] of fixture.cases
      .filter((c) => c.format === format)
      .entries()) {
      await page
        .getByRole("button", { name: "Add transaction", exact: true })
        .click();
      const date = page.getByLabel("Date", { exact: true });
      await date.fill("");
      await date.pressSequentially(item.input);
      // Complete input stays untouched until blur; abbreviations must not interrupt typing.
      if (item.input.length < 10) await expect(date).toHaveValue(item.input);
      await date.press("Tab");
      const expected =
        item.expected ??
        // Canonical ISO input was already supported by the rewrite in every
        // display format; retain that compatibility alongside baseline shortcuts.
        (item.input === "2026-04-05" ? item.input : null);
      if (expected) {
        const [year, month, day] = expected.split("-");
        await expect(date).toHaveValue(
          format.replace("yyyy", year).replace("MM", month).replace("dd", day),
        );
        await page
          .getByLabel("Amount", { exact: true })
          .fill(String(index + 1));
        await page
          .getByRole("button", { name: "Save transaction", exact: true })
          .click();
        await expect(
          page.getByRole("button", { name: "Add transaction", exact: true }),
        ).toBeEnabled();
        const rows = await (
          await fetch(api + `/api/v1/budgets/${budget}/transactions`)
        ).json();
        expect(
          rows.data.find((row) => row.amount === -(index + 1) * 100)?.date,
        ).toBe(expected);
      } else {
        await expect(date).toHaveValue(item.input);
        await page.getByLabel("Amount", { exact: true }).fill("1");
        await page
          .getByRole("button", { name: "Save transaction", exact: true })
          .click();
        await expect(date).toBeVisible();
        await expect(date).toHaveValue(item.input);
        await page.reload();
        await expect(page.getByLabel("Date", { exact: true })).toHaveValue(
          item.input,
        );
        await page.getByRole("button", { name: "Cancel", exact: true }).click();
      }
    }
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("99");
    const enterCase = fixture.cases.find((item) => item.format === format);
    const date = page.getByLabel("Date", { exact: true });
    await date.fill("");
    await date.pressSequentially(enterCase.input);
    await date.press("Enter");
    await expect(
      page.getByRole("button", { name: "Add transaction", exact: true }),
    ).toBeEnabled();
    const rows = await (
      await fetch(api + `/api/v1/budgets/${budget}/transactions`)
    ).json();
    expect(rows.data.find((row) => row.amount === -9900)?.date).toBe(
      enterCase.expected,
    );
    await context.close();
  }
  expect(errors).toEqual([]);
  console.log(
    `PASS ${fixture.cases.length} pinned date inputs: keyboard/blur/save, six formats, invalid draft reload, ISO compatibility`,
  );
} finally {
  await browser?.close();
  await server.stop();
}
