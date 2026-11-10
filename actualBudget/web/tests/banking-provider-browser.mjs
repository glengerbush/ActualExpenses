// Local controlled-provider integration, explicitly separate from production-URL UI.
import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";
import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "./server.mjs";
if (process.env.ACTUALBUDGET_TEST_URL)
  throw Error(
    "Controlled-provider browser requires its disposable local server. Use banking-browser.mjs for supplied production URLs.",
  );
const source = JSON.parse(
  await readFile(
    new URL(
      "../../tests/fixtures/bank-sync-source-26.10.json",
      import.meta.url,
    ),
  ),
).cases.find((c) => c.id === "first-sync-opening");
let repeatedWarnings = false;
let downloads = 0,
  claims = 0;
const provider = createServer((req, res) => {
  if (req.method === "POST") {
    claims++;
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  downloads++;
  assert.equal(
    req.headers.authorization,
    "Basic " + Buffer.from("synthetic:NOT-A-REAL-SECRET").toString("base64"),
  );
  const url = new URL(req.url, "http://fixture.invalid"),
    outcome = source.outcomes[0],
    balance = outcome.transport_response.balances[0].balanceAmount;
  res.setHeader("Content-Type", "application/json");
  res.end(
    JSON.stringify({
      errlist: repeatedWarnings
        ? [1, 2].map(() => ({
            code: "act.missingdata",
            account_id: "synthetic-remote-account",
            conn_id: "source-institution",
            msg: "RAW PRIVATE DIAGNOSTIC",
          }))
        : [],
      accounts: [
        {
          id: "synthetic-remote-account",
          org: { id: "source-institution", name: "Synthetic institution" },
          name: "Source checking",
          currency: balance.currency,
          balance: balance.amount,
          "balance-date": 1794225600,
          transactions: url.searchParams.has("balances-only")
            ? []
            : outcome.input.map((t) => ({
                id: t.transactionId,
                posted: t.booked
                  ? Date.parse((t.postedDate ?? t.date) + "T00:00:00Z") / 1000
                  : 0,
                transacted_at:
                  Date.parse((t.transactedDate ?? t.date) + "T00:00:00Z") /
                  1000,
                pending: !t.booked,
                amount: t.transactionAmount.amount,
                payee: t.payeeName,
                description: t.notes,
              })),
        },
        ...(url.searchParams.has("balances-only")
          ? [
              {
                id: "unused-provider-account",
                org: {
                  id: "source-institution",
                  name: "Synthetic institution",
                },
                name: "Unused provider account",
                currency: "USD",
                balance: "0.00",
                "balance-date": 1794225600,
                transactions: [],
              },
            ]
          : []),
      ],
    }),
  );
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
const server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
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
const bytes = await readFile(
  new URL(
    "../../tests/fixtures/bank-sync-source/" + source.archive.filename,
    import.meta.url,
  ),
);
assert.equal(
  createHash("sha256").update(bytes).digest("hex"),
  source.archive.sha256,
);
const directory = new URL("../test-results/banking/", import.meta.url);
let browser, page;
try {
  await mkdir(directory, { recursive: true });
  const uploaded = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  expect(uploaded.status).toBe(201);
  const stage = (await uploaded.json()).data;
  expect(stage.status).toBe("ready");
  await call(`/imports/${stage.id}/activate`, {}, 201);
  const budget = stage.budget_id,
    account = source.ids.account,
    root = `/budgets/${budget}`;
  await call(
    root + "/accounts",
    {
      id: account,
      name: "Source checking",
      off_budget: false,
      opening_balance: 0,
      opening_date: "2026-11-01",
    },
    201,
  );
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1050 },
  });
  await context.addInitScript(() =>
    Object.defineProperty(crypto, "subtle", { value: undefined }),
  );
  page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(server.url);
  await page.evaluate(
    ({ budget, account }) =>
      localStorage.setItem(
        "actualBudget.selection",
        JSON.stringify({ budgetId: budget, accountId: account }),
      ),
    { budget, account },
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Bank connections", exact: true })
    .click();
  const token = Buffer.from(
    process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim/FAKE-ONE-USE-TOKEN",
  ).toString("base64");
  await page.getByLabel("SimpleFIN setup token", { exact: true }).fill(token);
  await page
    .getByRole("button", { name: "Stage connection", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Claim setup token", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Claim setup token", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Discover provider accounts",
      exact: true,
    }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Discover provider accounts", exact: true })
    .click();
  await expect
    .poll(async () => (await call(root + "/banking")).data.accounts.length)
    .toBe(2);
  await expect(
    page.getByRole("button", {
      name: "Refresh connection details",
      exact: true,
    }),
  ).toBeEnabled();
  await page.getByLabel("Local account", { exact: true }).selectOption(account);
  await expect(
    page.getByRole("button", { name: "Save refresh preferences", exact: true }),
  ).toBeEnabled();
  const remote = (await call(root + "/banking")).data.accounts.find(
    (r) => r.remote_id === "synthetic-remote-account",
  );
  await page
    .getByLabel("Provider account", { exact: true })
    .selectOption(remote.id);
  await page
    .getByLabel("Initial opening balance", { exact: true })
    .selectOption("custom");
  await page.getByLabel("Opening amount", { exact: true }).fill("-");
  await expect(
    page.getByRole("button", { name: "Discard unsent draft", exact: true }),
  ).toBeEnabled();
  await page.reload();
  await expect(page.getByLabel("Opening amount", { exact: true })).toHaveValue(
    "-",
  );
  await page
    .getByLabel("Initial opening balance", { exact: true })
    .selectOption("auto");
  await page
    .getByRole("button", { name: "Save account mapping", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/banking")).data.links.filter((l) => !l.deleted)
          .length,
    )
    .toBe(1);
  await expect(
    page.getByRole("button", { name: "Save refresh preferences", exact: true }),
  ).toBeEnabled();
  await page.getByLabel("Import bank notes", { exact: true }).uncheck();
  await page
    .getByRole("button", { name: "Save refresh preferences", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + `/accounts/${account}/banking`)).data.preferences
          .settings.import_notes,
    )
    .toBe(false);
  await expect(
    page.getByRole("button", { name: "Save refresh preferences", exact: true }),
  ).toBeEnabled();
  await page.getByLabel("Import bank notes", { exact: true }).check();
  await page
    .getByRole("button", { name: "Save refresh preferences", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + `/accounts/${account}/banking`)).data.preferences
          .settings.import_notes,
    )
    .toBe(true);
  await expect(
    page.getByRole("button", { name: "Save refresh preferences", exact: true }),
  ).toBeEnabled();
  await page
    .getByLabel("Start date (included)", { exact: true })
    .fill("2026-11-01");
  await page
    .getByLabel("End date (excluded)", { exact: true })
    .fill("2026-11-10");
  const wires = [],
    commandPattern = new RegExp(
      `/api/v1/budgets/${budget}/banking/connections/[^/]+/commands$`,
    );
  let dropped = false;
  await page.route(commandPattern, async (route) => {
    const r = route.request(),
      body = JSON.parse(r.postData()),
      h = r.headers();
    if (body.action !== "refresh") {
      await route.continue();
      return;
    }
    wires.push({
      body: r.postData(),
      key: h["idempotency-key"],
      epoch: h["x-actual-budget-epoch"],
      clock: h["x-actual-edit-clock"],
      owner: h["x-actual-history-owner"],
    });
    if (dropped) {
      await route.continue();
      return;
    }
    dropped = true;
    const result = await fetch(
      api + new URL(r.url()).pathname.replace("/api/v1", ""),
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": h["idempotency-key"],
          "X-Actual-Budget-Epoch": h["x-actual-budget-epoch"],
          "X-Actual-Edit-Clock": h["x-actual-edit-clock"],
          "X-Actual-History-Owner": h["x-actual-history-owner"],
        },
        body: r.postData(),
      },
    );
    expect(result.status, await result.clone().text()).toBe(200);
    expect((await call(root + "/transactions")).data).toHaveLength(3);
    await route.abort("failed");
  });
  await page
    .getByRole("button", { name: "Refresh linked account", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("Cannot reach");
  const first = (await call(root + "/transactions")).data,
    firstIds = first.map((t) => t.id).sort();
  expect(new Set(firstIds).size).toBe(3);
  expect(wires[0].clock).toMatch(/Z-/);
  expect(wires[0].owner).toBeTruthy();
  await page.reload();
  await page
    .getByRole("button", { name: "Retry original request", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Saved bank request", exact: true }),
  ).toHaveCount(0);
  expect(wires).toHaveLength(2);
  expect(wires[1]).toEqual(wires[0]);
  expect(
    (await call(root + "/transactions")).data.map((t) => t.id).sort(),
  ).toEqual(firstIds);
  await expect(
    page.getByRole("status").filter({ hasText: "Refresh confirmed:" }),
  ).toContainText("Refresh confirmed: 3 added, 0 updated, 0 ignored.");
  const actualPayees = (await call(root + "/payees")).data,
    actualCategories = (await call(root + "/categories")).data;
  function project(rows, payees, categories, original) {
    const names = new Map(payees.map((p) => [p.id, p.name])),
      cats = new Map(categories.map((c) => [c.id, c.name]));
    return rows
      .map((t) => ({
        amount: t.amount,
        date: t.date,
        notes: t.notes ?? "",
        cleared: !!t.cleared,
        reconciled: !!t.reconciled,
        starting: !!t[original ? "starting_balance_flag" : "starting_balance"],
        payee: names.get(t[original ? "payee" : "payee_id"]) ?? null,
        category: cats.get(t[original ? "category" : "category_id"]) ?? null,
        category_id: t[original ? "category" : "category_id"] ?? null,
        retained_payee_id: source.before.payees.some(
          (p) => p.id === t[original ? "payee" : "payee_id"],
        )
          ? t[original ? "payee" : "payee_id"]
          : null,
        imported_id: t.imported_id ?? null,
        imported_payee: t.imported_payee ?? null,
        parent: t.parent_id ?? null,
        transfer: t.transfer_id ?? null,
      }))
      .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  }
  const expected = source.outcomes[0].after;
  expect(
    project(
      (await call(root + "/transactions")).data,
      actualPayees,
      actualCategories,
      false,
    ),
  ).toEqual(
    project(expected.transactions, expected.payees, expected.categories, true),
  );
  await page.unroute(commandPattern);
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Refresh linked account", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  expect(
    (await call(root + "/transactions")).data.map((t) => t.id).sort(),
  ).toEqual(firstIds);
  expect(claims).toBe(1);
  expect(downloads).toBe(3); // discovery, initial refresh, explicit repeated refresh; receipt lookup does not refetch.
  await page
    .getByRole("button", { name: "Unlink account", exact: true })
    .click();
  await expect
    .poll(async () => (await call(root + "/banking")).data.links[0].deleted)
    .toBe(true);
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeDisabled();
  await page
    .getByLabel("Initial opening balance", { exact: true })
    .selectOption("none");
  await page
    .getByRole("button", { name: "Save account mapping", exact: true })
    .click();
  await expect
    .poll(async () => (await call(root + "/banking")).data.links[0].deleted)
    .toBe(false);
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Refresh linked account", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  expect(
    (await call(root + "/transactions")).data.map((t) => t.id).sort(),
  ).toEqual(firstIds);
  const oldGeneration = (await call(root + "/banking")).data.connections[0]
    .generation;
  await page
    .locator("summary")
    .filter({ hasText: /^Replace credentials$/ })
    .click();
  const replacement = Buffer.from(
    process.env.ACTUALBUDGET_BANK_TEST_ORIGIN +
      "/claim/REPLACEMENT-FAKE-ONE-USE-TOKEN",
  ).toString("base64");
  await page
    .getByLabel("SimpleFIN setup token", { exact: true })
    .fill(replacement);
  await page
    .getByRole("button", { name: "Replace setup token", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/banking")).data.connections[0].generation,
    )
    .not.toBe(oldGeneration);
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Claim setup token", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Claim setup token", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Discover provider accounts",
      exact: true,
    }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "Save account mapping", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Discover provider accounts", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save account mapping", exact: true }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Save account mapping", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Refresh linked account", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  const retained = (await call(root + "/transactions")).data;
  expect(retained.map((t) => t.id).sort()).toEqual(firstIds);
  expect(
    project(
      retained,
      (await call(root + "/payees")).data,
      (await call(root + "/categories")).data,
      false,
    ),
  ).toEqual(
    project(expected.transactions, expected.payees, expected.categories, true),
  );
  expect(claims).toBe(2);
  expect(downloads).toBe(6);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".balances")).not.toContainText("100 USD");
  await expect(page.locator(".balances")).toContainText("••••");
  await expect(
    page.getByRole("heading", { name: "Source checking", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Privacy mode", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Privacy mode", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  await page
    .getByRole("button", { name: "Refresh connection details", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();
  // Distinct provider errors can normalize to identical safe public messages.
  repeatedWarnings = true;
  await page
    .getByRole("button", { name: "Discover provider accounts", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/banking")).data.accounts.find(
          (r) => r.remote_id === "synthetic-remote-account",
        ).diagnostics.length,
    )
    .toBe(2);
  await expect(
    page.getByRole("button", {
      name: "Discover provider accounts",
      exact: true,
    }),
  ).toBeEnabled();
  await expect(
    page
      .getByText("The provider returned incomplete transactions. Retry later", {
        exact: true,
      })
      .first(),
  ).toBeVisible();
  expect(await page.locator("body").innerText()).not.toContain(
    "RAW PRIVATE DIAGNOSTIC",
  );
  await page
    .getByRole("heading", { name: "Source checking", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-attention.png", directory).pathname,
  });
  repeatedWarnings = false;
  await page
    .getByRole("button", { name: "Discover provider accounts", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/banking")).data.accounts.find(
          (r) => r.remote_id === "synthetic-remote-account",
        ).diagnostics.length,
    )
    .toBe(0);
  await expect(
    page.getByRole("button", { name: "Refresh linked account", exact: true }),
  ).toBeEnabled();

  await page.screenshot({
    path: new URL("provider-desktop.png", directory).pathname,
    fullPage: true,
  });
  await page
    .getByRole("heading", {
      name: "Provider accounts and local mapping",
      exact: true,
    })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-desktop-viewport.png", directory).pathname,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: new URL("provider-mobile.png", directory).pathname,
    fullPage: true,
  });
  await page
    .getByRole("heading", {
      name: "Provider accounts and local mapping",
      exact: true,
    })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-mobile-viewport.png", directory).pathname,
  });
  await page.evaluate(() => (document.documentElement.style.fontSize = "32px"));
  await page.screenshot({
    path: new URL("provider-large-text.png", directory).pathname,
    fullPage: true,
  });
  await page
    .getByRole("heading", {
      name: "Provider accounts and local mapping",
      exact: true,
    })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-large-text-viewport.png", directory).pathname,
  });
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <=
        document.documentElement.clientWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
  await page.screenshot({
    path: new URL("provider-dark-large-text.png", directory).pathname,
    fullPage: true,
  });
  await page.screenshot({
    path: new URL("provider-dark-large-text-viewport.png", directory).pathname,
  });
  await page
    .getByLabel("Local account", { exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-dark-large-controls.png", directory).pathname,
  });
  await page
    .getByRole("heading", { name: "Refresh linked account", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("provider-dark-large-refresh.png", directory).pathname,
  });
  const disposable = randomUUID();
  await call(
    root + "/accounts",
    {
      id: disposable,
      name: "Disposable local mapping",
      off_budget: false,
      opening_balance: 0,
      opening_date: "2026-11-01",
    },
    201,
  );
  const currentBank = (await call(root + "/banking")).data,
    extraRemote = currentBank.accounts.find(
      (r) => r.remote_id === "unused-provider-account",
    ),
    extraHeaders = {
      "Content-Type": "application/json",
      "X-Actual-Budget-Epoch": currentBank.epoch,
      "Idempotency-Key": randomUUID(),
    };
  const extra = await fetch(
    api + root + `/accounts/${disposable}/banking/commands`,
    {
      method: "POST",
      headers: extraHeaders,
      body: JSON.stringify({
        action: "link",
        snapshot: currentBank.snapshot,
        connection_id: currentBank.connections[0].id,
        generation: currentBank.connections[0].generation,
        remote_account_id: extraRemote.id,
        opening_balance_mode: "none",
      }),
    },
  );
  expect(extra.status, await extra.clone().text()).toBe(200);
  const deleted = await fetch(api + root + `/accounts/${disposable}`, {
    method: "DELETE",
    headers: { ...extraHeaders, "Idempotency-Key": randomUUID() },
    body: JSON.stringify({ confirm_name: "Disposable local mapping" }),
  });
  expect(deleted.status, await deleted.clone().text()).toBe(200);
  await page
    .getByRole("button", { name: "Refresh connection details", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Unlink unavailable account",
      exact: true,
    }),
  ).toBeEnabled();
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "16px";
    document.documentElement.dataset.theme = "light";
  });
  await page.setViewportSize({ width: 1440, height: 1050 });
  await page
    .getByRole("heading", { name: "Unavailable local accounts", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: new URL("unavailable-account.png", directory).pathname,
  });
  await page
    .getByRole("button", { name: "Unlink unavailable account", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await call(root + "/banking")).data.links.find(
          (l) => l.id === disposable,
        ).deleted,
    )
    .toBe(true);
  await expect(
    page.getByRole("button", {
      name: "Unlink unavailable account",
      exact: true,
    }),
  ).toHaveCount(0);
  expect(
    (await call(root + "/transactions")).data.map((t) => t.id).sort(),
  ).toEqual(firstIds);
  await page
    .getByRole("button", { name: "Remove connection", exact: true })
    .click();
  await expect
    .poll(async () => (await call(root + "/banking")).data.connections.length)
    .toBe(0);
  expect(
    (await call(root + "/transactions")).data.map((t) => t.id).sort(),
  ).toEqual(firstIds);
  expect(claims).toBe(2);
  expect(downloads).toBe(8);
  expect(errors).toEqual([]);
  console.log(
    "PASS local provider UI claim/link/unlink/relink/replacement/removal/preferences/raw draft/reload/privacy + source financial graph + committed lost ACK immutable retry/dedup + responsive renders",
  );
} catch (e) {
  await page?.screenshot({
    path: "/tmp/bank28-provider-ui-failure.png",
    fullPage: true,
  });
  throw e;
} finally {
  await browser?.close();
  await server.stop();
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
