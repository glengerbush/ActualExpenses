// Complete the matched lab after prepare-resource-budgets.mjs. Run with access
// to Docker inspection; only explicitly named disposable containers are allowed.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { resolve } from "node:path";
const require = createRequire(new URL("../web/package.json", import.meta.url));
const { chromium } = require("playwright/test");
const [directoryArgument, baselineContainer, nativeContainer] =
  process.argv.slice(2);
assert.ok(
  directoryArgument && baselineContainer && nativeContainer,
  "Usage: node prepare-resource-browsers.mjs LAB_DIRECTORY actualbudget-q51-BASELINE actualbudget-q51-NATIVE",
);
const directory = resolve(directoryArgument);
const baseline = JSON.parse(await readFile(directory + "/baseline.json"));
const manifest = JSON.parse(
  await readFile(
    new URL(
      "../tests/fixtures/complete-migration/manifest.json",
      import.meta.url,
    ),
  ),
);
async function inspect(name) {
  assert.match(name, /^actualbudget-q51-[a-z0-9-]+$/);
  const [container] = JSON.parse(
    execFileSync("docker", ["inspect", name], { encoding: "utf8" }),
  );
  assert.ok(container.State.Running);
  const bindings = container.NetworkSettings.Ports["5006/tcp"];
  assert.equal(bindings.length, 1);
  assert.equal(
    bindings[0].HostIp,
    "127.0.0.1",
    "Lab ports must bind loopback only",
  );
  assert.equal(container.Mounts.length, 1);
  assert.equal(container.Mounts[0].Type, "volume");
  assert.equal(container.Mounts[0].Destination, "/data");
  const cgroup = (await readFile(`/proc/${container.State.Pid}/cgroup`, "utf8"))
    .trim()
    .split("\n")
    .find((line) => line.startsWith("0::"))
    ?.slice(3);
  assert.ok(
    cgroup?.includes(container.Id),
    "Container-specific cgroup v2 required",
  );
  return {
    server: "http://127.0.0.1:" + bindings[0].HostPort,
    cgroup: "/sys/fs/cgroup" + cgroup,
    image: container.Image,
    image_reference: container.Config.Image,
  };
}
const source = await inspect(baselineContainer),
  native = await inspect(nativeContainer);
assert.equal(source.server, baseline.server);
async function call(path, body, status = 200) {
  const response = await fetch(native.server + "/api/v1" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
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
assert.deepEqual(
  await call("/budgets"),
  [],
  "Refusing a populated replacement server",
);
await writeFile(directory + "/native.json", "{}\n", { flag: "wx" });
native.cases = [];
for (const test of baseline.cases) {
  const bytes = await readFile(directory + "/" + test.name + ".zip");
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    test.archive_sha256,
  );
  const start = performance.now(),
    stage = await call("/imports/actual", bytes, 201);
  assert.equal(stage.status, "ready");
  const staged = performance.now(),
    budget = await call("/imports/" + stage.id + "/activate", {}, 201);
  const activated = performance.now();
  const accounts = await call("/budgets/" + budget.id + "/accounts");
  assert.ok(accounts.some((account) => account.id === test.account));
  const expected = manifest.cases.find(
    (item) => item.name === (test.name === "typical" ? "complete" : test.name),
  );
  const financialRows = (rows) =>
    rows
      .map(({ id, balance }) => ({ id, balance }))
      .sort((a, b) => a.id.localeCompare(b.id));
  assert.deepEqual(
    financialRows(accounts),
    test.name === "empty"
      ? [{ id: test.account, balance: 0 }]
      : financialRows(expected.balances),
    "Source account balances survive resource-fixture conversion",
  );
  native.cases.push({
    ...test,
    id: budget.id,
    import_ms: staged - start,
    activation_ms: activated - staged,
    accounts,
  });
  await writeFile(
    directory + "/native.json",
    JSON.stringify(native, null, 2) + "\n",
  );
}
await writeFile(
  directory + "/baseline.json",
  JSON.stringify({ ...baseline, ...source }, null, 2) + "\n",
  { mode: 0o600 },
);
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
  args: ["--no-sandbox"],
});
try {
  const context = await browser.newContext(),
    page = await context.newPage();
  await page.goto(source.server + "/config-server");
  await page
    .getByRole("button", { name: "Use the current domain", exact: true })
    .click();
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Password", exact: true })
    .fill(baseline.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page
    .getByRole("row", { name: baseline.cases[0].label, exact: true })
    .waitFor();
  // No budget has been opened: authentication only, without financial caches.
  await writeFile(
    directory + "/auth.json",
    JSON.stringify(await context.storageState({ indexedDB: true })) + "\n",
    { flag: "wx", mode: 0o600 },
  );
} finally {
  await browser.close();
}
console.log(
  "Matched archives imported; browser auth and container counters ready:",
  directory,
);
