// Compare the baseline web's saved-file import with native backup restoration.
// These recovery workflows differ: source creates a file, native replaces one.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { randomUUID, createHash } from "node:crypto";
import { readFile, writeFile, mkdir, mkdtemp, rm } from "node:fs/promises";
import { resolve, basename } from "node:path";
import os from "node:os";
import {
  browserProcess,
  measureBrowser,
  summarize,
} from "./resource-browser.mjs";
const web = createRequire(new URL("../web/package.json", import.meta.url));
const sourceRequire = createRequire(
  new URL(
    "../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const { chromium, expect } = web("playwright/test"),
  source = sourceRequire("@actual-app/api");
assert.equal(
  JSON.parse(
    await readFile(
      sourceRequire
        .resolve("@actual-app/api")
        .replace(/dist\/index\.js$/, "package.json"),
    ),
  ).version,
  "26.10.0",
);
const [labArgument, outputArgument, countArgument = "20"] =
  process.argv.slice(2);
assert.ok(
  labArgument && outputArgument,
  "Usage: node measure-browser-restores.mjs LAB_DIRECTORY NEW_OUTPUT_DIRECTORY [SAMPLES=20]",
);
const lab = resolve(labArgument),
  output = resolve(outputArgument),
  count = Number(countArgument);
assert.ok(Number.isInteger(count) && count >= 1 && count <= 30);
const configs = {};
for (const variant of ["baseline", "native"]) {
  configs[variant] = JSON.parse(await readFile(lab + "/" + variant + ".json"));
  assert.equal(new URL(configs[variant].server).hostname, "127.0.0.1");
  assert.ok(configs[variant].cgroup?.startsWith("/sys/fs/cgroup/"));
}
await mkdir(output);
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const archive = await readFile(
  new URL("../tests/fixtures/reconciliation-large.zip", import.meta.url),
);
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/reconciliation-large.json", import.meta.url),
  ),
);
const test = fixture.accounts.find((item) => item.count === 1001);
assert.ok(test);
const fields = [
  "id",
  "account",
  "date",
  "amount",
  "parent_id",
  "is_parent",
  "is_child",
  "notes",
];
const normalize = (rows) =>
  rows
    .map((row) =>
      Object.fromEntries(
        fields.map((field) => [
          field,
          ["is_parent", "is_child"].includes(field)
            ? Boolean(row[field])
            : (row[field] ?? null),
        ]),
      ),
    )
    .sort((a, b) => a.id.localeCompare(b.id));
const oracleCache = await mkdtemp(output + "/oracle-");
let sourceEngine;
let epoch;
async function call(path, body, status = 200) {
  const response = await fetch(configs.native.server + "/api/v1" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      ...(epoch ? { "X-Actual-Budget-Epoch": epoch } : {}),
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
  assert.equal(
    response.status,
    status,
    path + ": " + (await response.clone().text()).slice(0, 1000),
  );
  return (await response.json()).data;
}
const result = {
  source_archive_sha256: hash(archive),
  source_version: "26.10.0",
  count,
  protocol:
    "Fresh disk browser profile per sample; warm file chooser; source web imports a saved Actual ZIP as a new budget, native reviews a portable ZIP then replaces its existing budget. Native review and activation are measured separately; confirmation typing excluded. Both finish at the1001-child account with the expected balance. Source explicit sync included; financial oracle reads occur outside measurements. No full offline/desktop baseline claim.",
  hardware: {
    hostname: os.hostname(),
    cpu: os.cpus()[0].model,
    cpus: os.cpus().length,
    memory: os.totalmem(),
    kernel: os.release(),
  },
  variants: {},
};
try {
  sourceEngine = await source.init({ dataDir: oracleCache });
  await source.importBudget(archive, { filename: "reconciliation-large.zip" });
  const expected = normalize(
    (
      await source.aqlQuery(
        source.q("transactions").options({ splits: "all" }).select(fields),
      )
    ).data,
  );
  const accounts = await source.getAccounts();
  const accountName = accounts.find((item) => item.id === test.account).name;
  const expectedBalance = (
    await source.aqlQuery(
      source
        .q("transactions")
        .filter({ account: test.account })
        .calculate({ $sum: "$amount" }),
    )
  ).data;
  assert.equal(expectedBalance, -1001);
  result.financial = {
    rows: expected.length,
    ledger_sha256: hash(JSON.stringify(expected)),
    account: test.account,
    parent: test.parent,
    children: 1001,
    balance: expectedBalance,
  };
  await source.shutdown();
  sourceEngine = undefined;
  const stage = await call("/imports/actual", archive, 201);
  const template =
    stage.status === "ready"
      ? await call("/imports/" + stage.id + "/activate", {}, 201)
      : await call("/budgets/" + stage.budget_id);
  epoch = template.epoch ?? (await call("/budgets/" + template.id)).epoch;
  const budget = await call(
    "/budgets/" + template.id + "/duplicate",
    { id: randomUUID(), name: "Resource restore " + basename(output) },
    201,
  );
  const root = "/budgets/" + budget.id;
  epoch = (await call(root)).epoch;
  const backup = await call(
    root + "/backups",
    { label: "Resource restore original" },
    201,
  );
  const download = await fetch(
    configs.native.server +
      "/api/v1" +
      root +
      "/backups/" +
      backup.id +
      "/download",
  );
  assert.ok(download.ok);
  const nativeArchive = Buffer.from(await download.arrayBuffer());
  result.native_archive_sha256 = hash(nativeArchive);
  result.archive_bytes = {
    baseline: archive.length,
    native: nativeArchive.length,
  };
  result.budget_id = budget.id;
  await writeFile(output + "/native.zip", nativeArchive, { flag: "wx" });
  sourceEngine = await source.init({
    dataDir: oracleCache,
    serverURL: configs.baseline.server,
    password: configs.baseline.password,
  });
  for (const variant of ["baseline", "native"]) {
    const config = configs[variant],
      observations = {
        image: config.image,
        review: [],
        restore: [],
        financial_checks: [],
      };
    result.variants[variant] = observations;
    for (let index = 0; index < count; index++) {
      const profile = await mkdtemp(output + "/profile-");
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
            "X-Forwarded-For": `198.21.${1 + (parseInt(hash(output).slice(0, 2), 16) % 250)}.${index + 1}`,
          },
        });
        result.browser ??= context.browser().version();
        assert.equal(context.browser().version(), result.browser);
        const browserPid = await browserProcess(profile);
        assert.ok(browserPid);
        if (variant === "baseline")
          await context.setStorageState(lab + "/auth.json");
        page = context.pages()[0];
        page.setDefaultTimeout(60000);
        const errors = [];
        page.on("pageerror", (error) => errors.push(String(error)));
        const measured = (action) =>
          measureBrowser(context, action, {
            variant,
            server: config.server,
            serverCgroup: config.cgroup,
            browserPid,
          });
        await page.goto(config.server);
        let financial;
        if (variant === "baseline") {
          const previous = new Set(
            (await source.getBudgets())
              .map((item) => item.cloudFileId)
              .filter(Boolean),
          );
          await page
            .getByRole("button", { name: "Import file", exact: true })
            .click();
          await page
            .getByRole("button", {
              name: "Actual Import a file exported from Actual",
              exact: true,
            })
            .click();
          const choice = page.waitForEvent("filechooser");
          await page
            .getByRole("button", { name: "Select file...", exact: true })
            .click();
          const chooser = await choice;
          observations.restore.push(
            await measured(async () => {
              await chooser.setFiles({
                name: "reconciliation-large.zip",
                mimeType: "application/zip",
                buffer: archive,
              });
              await page.locator(`a[href="/accounts/${test.account}"]`).click();
              await expect(page.getByTestId("account-balance")).toHaveText(
                "-10.01",
              );
              const synced = page.waitForResponse(
                (r) =>
                  new URL(r.url()).pathname === "/sync/sync" &&
                  r.request().method() === "POST",
              );
              await page
                .getByRole("button", { name: "Server Sync", exact: true })
                .click();
              const response = await synced;
              assert.ok(response.ok());
              await response.finished();
            }),
          );
          const added = (await source.getBudgets()).filter(
            (item) => item.cloudFileId && !previous.has(item.cloudFileId),
          );
          assert.equal(added.length, 1, "One new restored source file");
          await source.downloadBudget(added[0].groupId);
          financial = normalize(
            (
              await source.aqlQuery(
                source
                  .q("transactions")
                  .options({ splits: "all" })
                  .select(fields),
              )
            ).data,
          );
          await sourceEngine.send("close-budget");
        } else {
          await page
            .getByRole("button", { name: "Restore backup", exact: true })
            .click();
          await page
            .getByLabel("actualBudget backup ZIP", { exact: true })
            .setInputFiles({
              name: "resource-native.zip",
              mimeType: "application/zip",
              buffer: nativeArchive,
            });
          observations.review.push(
            await measured(async () => {
              await page
                .getByRole("button", { name: "Review backup", exact: true })
                .click();
              await expect(
                page.getByText(
                  "Backup validated. The original archive is retained.",
                  { exact: true },
                ),
              ).toBeVisible();
            }),
          );
          await page
            .getByLabel("Confirm budget name", { exact: true })
            .fill(budget.name);
          const oldEpoch = epoch;
          observations.restore.push(
            await measured(async () => {
              const restored = page.waitForResponse(
                (r) =>
                  new URL(r.url()).pathname.endsWith("/activate") &&
                  r.request().method() === "POST",
              );
              await page
                .getByRole("button", {
                  name: "Restore reviewed backup",
                  exact: true,
                })
                .click();
              const response = await restored;
              assert.ok(response.ok());
              await response.finished();
              await expect(
                page.getByRole("heading", { name: "Accounts", exact: true }),
              ).toBeVisible();
              await page
                .getByRole("button", {
                  name: new RegExp(
                    "^" +
                      accountName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") +
                      " ",
                  ),
                })
                .first()
                .click();
              await expect(
                page.getByRole("button", {
                  name: "Refresh transactions",
                  exact: true,
                }),
              ).toBeEnabled();
              await expect(page.getByTestId("register-total")).toContainText(
                "-10.01",
              );
            }),
          );
          epoch = (await call(root)).epoch;
          assert.notEqual(epoch, oldEpoch);
          const snapshot = (await call(root + "/checkpoint")).snapshot;
          const rows = [];
          for (let offset = 0; ; offset += 1000) {
            const pageRows = await call(root + "/query", {
              version: 1,
              table: "transactions",
              splits: "all",
              select: fields,
              order_by: ["id"],
              limit: 1000,
              offset,
              snapshot,
            });
            rows.push(...pageRows);
            if (pageRows.length < 1000) break;
          }
          financial = normalize(rows);
        }
        assert.deepEqual(
          financial,
          expected,
          "Every restored financial row matches the pinned engine",
        );
        assert.deepEqual(errors, []);
        observations.financial_checks.push({
          sample: index + 1,
          rows: financial.length,
          ledger_sha256: hash(JSON.stringify(financial)),
        });
        await writeFile(
          output + "/observations.json",
          JSON.stringify(result, null, 2) + "\n",
        );
        console.log("PASS", variant, "recovery", index + 1, "/", count);
      } catch (error) {
        observations.error = String(error);
        if (page)
          await writeFile(
            output + "/" + variant + "-failure.txt",
            await page.locator("body").ariaSnapshot(),
          );
        throw error;
      } finally {
        await context?.close();
        await rm(profile, { recursive: true, force: true });
      }
    }
    observations.summary = {
      restore: summarize(observations.restore),
      ...(observations.review.length
        ? { review: summarize(observations.review) }
        : {}),
    };
  }
} catch (error) {
  result.error = String(error);
  throw error;
} finally {
  await source.shutdown();
  await rm(oracleCache, { recursive: true, force: true });
  await writeFile(
    output + "/observations.json",
    JSON.stringify(result, null, 2) + "\n",
  );
}
