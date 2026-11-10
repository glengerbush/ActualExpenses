// Public HTTP tracer; genuine pinned whole-handler expectations, isolated imports.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const server = await startServer();
try {
  const origin = server.url.replace("actualbudget.test", "127.0.0.1");
  async function api(path, body, headers = {}) {
    const response = await fetch(origin + "/api/v1" + path, {
      method: body === undefined ? "GET" : "POST",
      headers: { "Content-Type": "application/json", ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return {
      status: response.status,
      ...(await response.json().catch(() => ({}))),
    };
  }
  const source = JSON.parse(
    await readFile(
      new URL(
        "../../tests/fixtures/crossover-source-26.10.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const imported = await fetch(origin + "/api/v1/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: isolatedImportBytes(
      await readFile(
        new URL(
          "../../tests/fixtures/crossover-source-26.10.zip",
          import.meta.url,
        ),
      ),
    ),
  });
  assert.equal(imported.status, 201);
  const stage = await imported.json();
  assert.ok(
    ["ready", "activated"].includes(stage.data.status),
    JSON.stringify(stage),
  );
  if (stage.data.status === "ready")
    assert.equal(
      (
        await api(
          `/imports/${stage.data.id}/activate`,
          {},
          { "Idempotency-Key": randomUUID() },
        )
      ).status,
      201,
    );
  const original = "/budgets/" + stage.data.budget_id,
    importedState = await api(original + "/reporting"),
    budget = randomUUID(),
    root = "/budgets/" + budget;
  assert.equal(
    (
      await api(
        original + "/duplicate",
        { id: budget, name: "Crossover proof " + budget },
        {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": importedState.data.epoch,
        },
      )
    ).status,
    201,
  );
  const before = await api(root + "/reporting"),
    widget = before.data.documents.find((d) => d.id === source.widgets[0].id);
  assert.deepEqual(widget.config.meta, source.widgets[0].meta);
  const response = await api(root + "/reports/crossover", {
    version: 1,
    as_of: source.clock.slice(0, 10),
    options: source.cases[0].options,
  });
  assert.equal(response.status, 200, JSON.stringify(response));
  assert.deepEqual(response.data.result, source.cases[0].result);
  assert.deepEqual(await api(root + "/reporting"), before);
  console.log(
    "PASS whole-source saved Crossover projection via public HTTP, imported parameters preserved and read-only",
  );
} finally {
  await server.stop();
}
