// Expense acknowledgements and one matched cash-flow dashboard. Mutates only
// explicitly prepared disposable Workload budgets; never resume a partial save.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { resolve } from "node:path";
import os from "node:os";
import {
  browserProcess,
  measureBrowser,
  summarize,
} from "./resource-browser.mjs";
const require = createRequire(new URL("../web/package.json", import.meta.url));
const { chromium, expect } = require("playwright/test");
const [
  directoryArgument,
  variant,
  outputArgument,
  countArgument = "20",
  onlyCase,
] = process.argv.slice(2);
assert.ok(
  directoryArgument &&
    outputArgument &&
    ["baseline", "native"].includes(variant),
  "Usage: node measure-browser-workloads.mjs LAB_DIRECTORY baseline|native NEW_OUTPUT_JSON [SAMPLES=20] [empty|typical|large]",
);
const directory = resolve(directoryArgument),
  output = resolve(outputArgument),
  count = Number(countArgument);
assert.ok(Number.isInteger(count) && count >= 1 && count <= 100);
assert.ok(
  onlyCase === undefined || ["empty", "typical", "large"].includes(onlyCase),
);
const config = JSON.parse(await readFile(directory + "/" + variant + ".json"));
assert.equal(new URL(config.server).hostname, "127.0.0.1");
assert.ok(config.cgroup?.startsWith("/sys/fs/cgroup/"));
await writeFile(output, "{}\n", { flag: "wx" });
const document = {
  variant,
  count,
  image: config.image,
  browser: null,
  hardware: {
    hostname: os.hostname(),
    cpu: os.cpus()[0].model,
    cpus: os.cpus().length,
    memory: os.totalmem(),
    kernel: os.release(),
  },
  profile:
    "persistent disk profile; warm account; form entry excluded; save click through changed account balance and server acknowledgement; report navigation to one static Oct–Nov2026 cash-flow widget after each new expense; no browser restart between samples; explicit source sync included; DNS loopback only",
  cases: [],
};
const escaped = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
async function account(page, test) {
  if (variant === "baseline") {
    await page.locator(`a[href="/accounts/${test.account}"]`).click();
    await expect(
      page.getByRole("button", { name: "Add New", exact: true }),
    ).toBeVisible();
  } else {
    const close = page.getByRole("button", {
      name: "Close reports",
      exact: true,
    });
    if (await close.isVisible()) await close.click();
    const name = test.accounts.find((item) => item.id === test.account).name;
    await page
      .getByRole("button", { name: new RegExp("^" + escaped(name) + " ") })
      .first()
      .click();
    await expect(
      page.getByRole("button", { name: "Refresh transactions", exact: true }),
    ).toBeEnabled();
  }
}
async function fillExpense(page, note) {
  if (variant === "baseline") {
    await page.getByRole("button", { name: "Add New", exact: true }).click();
    const row = page.getByTestId("new-transaction").getByTestId("row");
    const date = row.getByTestId("date").getByRole("textbox");
    await date.fill("2026-11-10");
    await date.press("Tab");
    const debit = row.getByTestId("debit");
    await debit.click();
    await debit.getByRole("textbox").fill("12.34");
    await page.keyboard.press("Tab");
    const notes = row.getByTestId("notes");
    await notes.click();
    await notes.getByRole("combobox").fill(note);
    await page.keyboard.press("Tab");
  } else {
    await page
      .getByRole("button", { name: "Add transaction", exact: true })
      .click();
    await page.getByLabel("Amount", { exact: true }).fill("12.34");
    await page.getByLabel("Date", { exact: true }).fill("2026-11-10");
    await page.getByLabel("Notes", { exact: true }).fill(note);
    await page.getByLabel("Cleared", { exact: true }).uncheck();
  }
}
const accountBalance = (page) =>
  page.getByTestId(
    variant === "baseline" ? "account-balance" : "register-total",
  );
async function saveExpense(page, beforeBalance) {
  if (variant === "baseline") {
    await page.getByTestId("add-button").click();
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(accountBalance(page)).not.toHaveText(beforeBalance);
    const response = page.waitForResponse(
      (r) =>
        new URL(r.url()).pathname === "/sync/sync" &&
        r.request().method() === "POST",
    );
    await page
      .getByRole("button", { name: "Server Sync", exact: true })
      .click();
    const sync = await response;
    assert.ok(sync.ok());
    await sync.finished();
  } else {
    const response = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        /\/(rules\/)?transactions$/.test(new URL(r.url()).pathname),
    );
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .click();
    const saved = await response;
    assert.ok(saved.ok(), await saved.text());
    await saved.finished();
    await expect(
      page.getByRole("button", { name: "Save transaction", exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Refresh transactions", exact: true }),
    ).toBeEnabled();
    await expect(accountBalance(page)).not.toHaveText(beforeBalance);
  }
}
async function report(page, test) {
  if (variant === "baseline") {
    await page.locator('a[href="/reports"]').click();
    // Workload preparation leaves exactly one active dashboard, so navigation
    // cannot accidentally time the baseline's unrelated default report cards.
    await expect(
      page.getByRole("button", { name: test.reporting.name, exact: true }),
    ).toBeVisible();
    const card = page.getByRole("button", {
      name: new RegExp("^" + escaped(test.reporting.widget_name)),
    });
    await expect(card.getByRole("application")).toBeVisible();
  } else {
    await page
      .getByRole("button", { name: "Dashboards and reports", exact: true })
      .click();
    await page
      .getByRole("combobox", { name: "Dashboard", exact: true })
      .selectOption(test.reporting.page);
    const card = page.getByRole("article", {
      name: test.reporting.widget_name,
      exact: true,
    });
    await expect(card.getByRole("img")).toBeVisible();
    await expect(
      card.getByText("Loading report…", { exact: true }),
    ).toHaveCount(0);
  }
}
for (const test of config.cases.filter(
  (test) => !onlyCase || test.name === onlyCase,
)) {
  assert.ok(test.label.startsWith("Workload ") && test.reporting?.page);
  await writeFile(
    directory + "/" + variant + "-" + test.name + ".workload-started.json",
    JSON.stringify({ output, count, started_at: new Date().toISOString() }) +
      "\n",
    { flag: "wx" },
  );
  const result = {
    name: test.name,
    archive_sha256: test.archive_sha256,
    expense: [],
    report: [],
  };
  document.cases.push(result);
  const profile = await mkdtemp(directory + "/workload-profile-");
  let context, page;
  try {
    context = await chromium.launchPersistentContext(profile, {
      executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
      args: [
        "--no-sandbox",
        "--no-proxy-server",
        "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1, EXCLUDE localhost",
      ],
      viewport: { width: 1440, height: 1000 },
      extraHTTPHeaders: {
        "X-Forwarded-For":
          "198.19.1." + (["empty", "typical", "large"].indexOf(test.name) + 1),
      },
    });
    document.browser = context.browser().version();
    const browserPid = await browserProcess(profile);
    assert.ok(browserPid);
    if (variant === "baseline")
      await context.setStorageState(directory + "/auth.json");
    page = context.pages()[0];
    page.setDefaultTimeout(30000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    result.browser_errors = errors;
    await page.goto(config.server);
    if (variant === "baseline")
      await page
        .getByRole("row", { name: test.label, exact: true })
        .press("Enter");
    else
      await page
        .getByRole("button", {
          name: new RegExp(escaped(test.label) + ".*Open"),
        })
        .click();
    await account(page, test);
    const measured = (action) =>
      measureBrowser(context, action, {
        variant,
        server: config.server,
        serverCgroup: config.cgroup,
        browserPid,
      });
    for (let index = 0; index < count; index++) {
      const note = `Resource acknowledgement ${test.name} ${String(index + 1).padStart(3, "0")}`;
      const beforeBalance = await accountBalance(page).innerText();
      await fillExpense(page, note);
      result.expense.push(
        await measured(() => saveExpense(page, beforeBalance)),
      );
      result.report.push(await measured(() => report(page, test)));
      await account(page, test);
      await writeFile(output, JSON.stringify(document, null, 2) + "\n");
      console.log(variant, test.name, `${index + 1}/${count}`);
    }
    result.summary = {
      expense: summarize(result.expense),
      report: summarize(result.report),
    };
  } catch (error) {
    result.error = String(error);
    if (page)
      await writeFile(
        output.replace(/\.json$/, "-failure.txt"),
        await page.locator("body").ariaSnapshot(),
      );
    throw error;
  } finally {
    await context?.close();
    await rm(profile, { recursive: true, force: true });
    await writeFile(output, JSON.stringify(document, null, 2) + "\n");
  }
}
