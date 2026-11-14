import { chromium, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { copyFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { startServer } from "./server.mjs";

const directory = await mkdtemp(join(tmpdir(), "actual-preference-receipt-"));
let server, browser;
try {
  const database = join(directory, "actualbudget.sqlite3");
  await copyFile(
    fileURLToPath(
      new URL(
        "../../tests/fixtures/budget-currency-schema19-disabled.sqlite3",
        import.meta.url,
      ),
    ),
    database,
  );
  const keys = [randomUUID(), randomUUID()];
  // Authentic old-server committed receipt. The CLI fixture used a non-UUID key;
  // copy only its identity for the released browser draft's UUID constraint.
  const fixture = JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `
import json, sqlite3, sys
with sqlite3.connect(sys.argv[1]) as db:
    operations = {key: (scope,payload,response) for scope,key,payload,response in db.execute("SELECT scope,operation_id,payload,response FROM operations WHERE operation_id IN ('currency-0','currency-1')")}
    base = json.loads(operations['currency-0'][2])
    initial = json.loads(json.dumps(base)); initial['revision']=0; initial['currency_code']=''; initial['experimental']['currency']=False
    cases=[]
    for operation,key,before in [('currency-1',sys.argv[2],base),('currency-0',sys.argv[3],initial)]:
        scope,payload,response=operations[operation]
        db.execute("INSERT INTO operations(scope,operation_id,payload,response) VALUES(?,?,?,?)", (scope,key,payload,response))
        cases.append(dict(key=key,budget=scope.split('/')[2], changes=json.loads(payload), response=json.loads(response), base=before))
    print(json.dumps(cases))
`,
        database,
        ...keys,
      ],
      { encoding: "utf8" },
    ),
  );
  server = await startServer({ dataDirectory: directory });
  const api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
  const root = `/budgets/${fixture[0].budget}`;
  const get = async (path) => {
    const response = await fetch(api + path);
    expect(response.ok, await response.clone().text()).toBe(true);
    return response.json();
  };
  const before = await get(root + "/preferences");
  const checkpoint = await get(root + "/checkpoint");
  browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
    args: [
      "--no-sandbox",
      "--no-proxy-server",
      "--host-resolver-rules=MAP actualbudget.test 127.0.0.1",
    ],
  });
  const page = await browser.newPage();
  await page.goto(server.url);
  await page.waitForLoadState("networkidle");
  const requests = [];
  await page.route(`**/api/v1${root}/preferences`, async (route) => {
    if (route.request().method() === "PATCH")
      requests.push({
        key: route.request().headers()["idempotency-key"],
        body: route.request().postDataJSON(),
      });
    await route.continue();
  });
  for (const sample of fixture) {
    const key = sample.key;
    await page.evaluate(
      ({ fixture, key }) => {
        const values = { ...structuredClone(fixture.base), ...fixture.changes };
        values.experimental = {
          ...fixture.base.experimental,
          ...fixture.changes.experimental,
        };
        localStorage.setItem(
          `actualBudget.preferencesDraft.v1.${fixture.budget}`,
          JSON.stringify({
            version: 1,
            origin: location.origin,
            budget_id: fixture.budget,
            draft_id: key,
            base: fixture.base,
            values,
            pending: { key, changes: fixture.changes },
          }),
        );
      },
      { fixture: sample, key },
    );
    await page.goto(`${server.url}/?budget=${sample.budget}&view=settings`);
    await page.waitForLoadState("networkidle");
    await expect(
      page.getByRole("button", { name: "Retry saving", exact: true }),
    ).toBeEnabled();
    const replayed = page.waitForResponse(
      (response) =>
        response.request().method() === "PATCH" &&
        response.url().endsWith(root + "/preferences"),
      { timeout: 5000 },
    );
    await page
      .getByRole("button", { name: "Retry saving", exact: true })
      .click();
    const response = await replayed;
    expect(response.status()).toBe(200);
    expect(await response.json()).toEqual({ data: sample.response });
    expect(requests.at(-1)).toEqual({ key, body: sample.changes });
    expect(requests.filter((request) => request.key === key)).toHaveLength(1);
    await expect
      .poll(() =>
        page.evaluate(
          (budget) =>
            localStorage.getItem(`actualBudget.preferencesDraft.v1.${budget}`),
          sample.budget,
        ),
      )
      .toBeNull();
    expect(await get(root + "/preferences")).toEqual(before);
    expect(await get(root + "/checkpoint")).toEqual(checkpoint);
    await expect(page.locator("#number-format")).toHaveValue(
      before.data.number_format,
    );
  }
  console.log(
    "PASS experimental-only and currency committed preference receipts: exact replay, drafts cleared, current settings/cursor unchanged",
  );
} finally {
  await browser?.close();
  await server?.stop();
  await rm(directory, { recursive: true, force: true });
}
