// Large split discovery, response construction and atomic rollback via public HTTP.
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { measure, percentile } from "./resource-metrics.mjs";
const [labArgument, outputArgument, countArgument = "20"] =
  process.argv.slice(2);
assert.ok(
  labArgument && outputArgument,
  "Usage: node check-resource-graphs.mjs LAB_DIRECTORY NEW_OUTPUT_JSON [SAMPLES=20]",
);
const config = JSON.parse(
    await readFile(resolve(labArgument) + "/native.json"),
  ),
  output = resolve(outputArgument),
  count = Number(countArgument);
assert.equal(new URL(config.server).hostname, "127.0.0.1");
assert.ok(config.cgroup?.startsWith("/sys/fs/cgroup/"));
assert.ok(Number.isInteger(count) && count >= 1 && count <= 100);
await writeFile(output, "{}\n", { flag: "wx" });
const bytes = await readFile(
  new URL("../tests/fixtures/reconciliation-large.zip", import.meta.url),
);
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/reconciliation-large.json", import.meta.url),
  ),
);
const large = fixture.accounts.find((item) => item.count === 1001),
  small = fixture.accounts.find((item) => item.count === 101);
assert.ok(large && small);
let headers = {};
async function call(path, body, status = 200) {
  const response = await fetch(config.server + "/api/v1" + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": Buffer.isBuffer(body)
        ? "application/zip"
        : "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    body:
      body === undefined
        ? undefined
        : Buffer.isBuffer(body)
          ? body
          : JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, status, path + ": " + text.slice(0, 1000));
  return {
    status: response.status,
    bytes: Buffer.byteLength(text),
    ...JSON.parse(text),
  };
}
const result = {
  image: config.image,
  count,
  source_archive_sha256: createHash("sha256").update(bytes).digest("hex"),
  protocol:
    "Production HTTP; pinned101/1001-child split fixture; repeated cap refusal, mid-batch rollback after deleting the large graph, successful owned sparse allocation-note edit, complete split response. Financial checks outside measurements. Node caller counters are not browser costs; browser event arrays uninstrumented, use kernel network counters. No claim for the100000-member ceiling.",
  bounded: [],
  rollback: [],
  edit: [],
  response: [],
};
async function measured(action) {
  return measure({
    browserPid: process.pid,
    serverCgroup: config.cgroup,
    context: new EventEmitter(),
    action: async () => {
      const start = performance.now(),
        response = await action();
      return {
        http_ms: performance.now() - start,
        response_bytes: response.bytes,
        status: response.status,
        error_code: response.error?.code ?? null,
      };
    },
  });
}
const hash = (value) =>
  createHash("sha256").update(JSON.stringify(value)).digest("hex");
try {
  const stage = (await call("/imports/actual", bytes, 201)).data;
  const template = (
    stage.status === "ready"
      ? await call("/imports/" + stage.id + "/activate", {}, 201)
      : await call("/budgets/" + stage.budget_id)
  ).data;
  headers = {
    "X-Actual-Budget-Epoch": (await call("/budgets/" + template.id)).data.epoch,
  };
  const budget = (
    await call(
      "/budgets/" + template.id + "/duplicate",
      { id: randomUUID(), name: "Resource graph qualification" },
      201,
    )
  ).data;
  const root = "/budgets/" + budget.id;
  result.budget_id = budget.id;
  headers = {
    "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
    "X-Actual-History-Owner": randomUUID(),
  };
  const graph = async () =>
    (await call(root + "/transactions/" + large.parent + "/splits")).data;
  const balance = async () =>
    (await call(root + "/accounts")).data
      .map(({ id, balance }) => ({ id, balance }))
      .sort((a, b) => a.id.localeCompare(b.id));
  const financial = async () => {
    const rows = [],
      snapshot = (await call(root + "/checkpoint")).data.snapshot;
    for (let offset = 0; ; offset += 1000) {
      const page = (
        await call(root + "/query", {
          version: 1,
          table: "transactions",
          splits: "all",
          select: [
            "id",
            "account",
            "date",
            "amount",
            "parent_id",
            "is_parent",
            "is_child",
          ],
          order_by: ["id"],
          limit: 1000,
          offset,
          snapshot,
        })
      ).data;
      rows.push(...page);
      if (page.length < 1000) break;
    }
    assert.equal(
      rows.length,
      1104,
      "Complete pinned split fixture, including parents",
    );
    return hash(rows);
  };
  const initial = await graph(),
    originalBalance = await balance(),
    originalFinancial = await financial();
  assert.equal(initial.children.length, 1001);
  assert.equal(initial.amount, -1001);
  assert.equal(
    initial.children.reduce((sum, row) => sum + row.amount, 0),
    -1001,
  );
  assert.equal(
    originalBalance.find((row) => row.id === small.account).balance,
    -101,
  );
  assert.equal(
    originalBalance.find((row) => row.id === large.account).balance,
    -1001,
  );
  for (let index = 0; index < count; index++) {
    const checkpoint = (await call(root + "/checkpoint")).data,
      history = (await call(root + "/history")).data;
    const prior = (await call(root + "/transactions/" + large.parent)).data
      .notes;
    const deletion = { action: "delete", id: large.parent };
    const note = "Resource graph " + index;
    result.bounded.push(
      await measured(async () => {
        const response = await call(
          root + "/transactions/batch",
          {
            snapshot: checkpoint.snapshot,
            max_graph_members: 1001,
            operations: [deletion],
          },
          400,
        );
        assert.equal(response.error.code, "graph_limit");
        return response;
      }),
    );
    assert.deepEqual((await call(root + "/checkpoint")).data, checkpoint);
    result.rollback.push(
      await measured(async () => {
        const response = await call(
          root + "/transactions/batch",
          {
            snapshot: checkpoint.snapshot,
            max_graph_members: 1104,
            operations: [
              deletion,
              {
                action: "update",
                id: small.parent,
                fields: {
                  notes:
                    "Ordinary split edits must be refused after the first deletion",
                },
              },
            ],
          },
          409,
        );
        assert.equal(response.error.code, "batch_conflict");
        assert.ok(
          response.error.message.includes("transaction " + small.parent),
          "Failure must occur while executing the second operation, after the first write",
        );
        return response;
      }),
    );
    assert.equal(
      (await call(root + "/transactions/" + large.parent)).data.notes,
      prior,
    );
    assert.deepEqual((await call(root + "/checkpoint")).data, checkpoint);
    assert.deepEqual((await call(root + "/history")).data, history);
    assert.equal(await financial(), originalFinancial);
    result.edit.push(
      await measured(() =>
        call(root + "/transactions/" + large.parent + "/splits", {
          snapshot: checkpoint.snapshot,
          changes: {
            upsert: [{ id: initial.children[0].id, amount: -1, notes: note }],
          },
        }),
      ),
    );
    assert.equal(
      (await call(root + "/transactions/" + initial.children[0].id)).data.notes,
      note,
    );
    let response;
    result.response.push(
      await measured(
        async () =>
          (response = await call(
            root + "/transactions/" + large.parent + "/splits",
          )),
      ),
    );
    assert.equal(response.data.children.length, 1001);
    assert.equal(response.data.amount, -1001);
    assert.equal(
      response.data.children.reduce((sum, row) => sum + row.amount, 0),
      -1001,
    );
    assert.deepEqual(await balance(), originalBalance);
    assert.equal(await financial(), originalFinancial);
    await writeFile(output, JSON.stringify(result, null, 2) + "\n");
    console.log("PASS graph cap/rollback/edit/response", index + 1, "/", count);
  }
  result.financial = {
    rows: 1104,
    ledger_sha256_without_notes: originalFinancial,
    balances: originalBalance,
    graph_members: 1002,
    unchanged_after_refusals_and_edits: true,
  };
  const q = (values) => ({
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
  });
  result.summary = Object.fromEntries(
    ["bounded", "rollback", "edit", "response"].map((phase) => {
      const rows = result[phase];
      return [
        phase,
        {
          samples: rows.length,
          http_ms: q(rows.map((row) => row.value.http_ms)),
          server_cpu_ms: q(rows.map((row) => row.server.cpu_ms)),
          server_peak_memory_bytes: q(
            rows.map((row) => row.server.peak_memory_bytes),
          ),
          response_bytes: q(rows.map((row) => row.value.response_bytes)),
          server_tx_bytes: q(
            rows.map((row) => row.server.network.eth0.tx_bytes),
          ),
          server_write_bytes_by_device: Object.fromEntries(
            [...new Set(rows.flatMap((row) => Object.keys(row.server.io)))].map(
              (device) => [
                device,
                q(rows.map((row) => row.server.io[device]?.wbytes ?? 0)),
              ],
            ),
          ),
        },
      ];
    }),
  );
} catch (error) {
  result.error = String(error);
  throw error;
} finally {
  await writeFile(output, JSON.stringify(result, null, 2) + "\n");
}
