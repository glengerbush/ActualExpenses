// Independent ActualQL resource/relationship observations, through public HTTP.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/" +
          (process.argv.includes("--types")
            ? "query-types-source.json"
            : "query-resources-source.json"),
        import.meta.url,
      ),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function request(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, path + " " + (await r.clone().text()));
  return r.json();
}
// The replacement's public rule profile uses its documented canonical field names.
// Preserve every nested recurrence discriminator, value, option and action.
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [
        k,
        ["goal_def", "cleanup_def", "raw_synced_data"].includes(k) &&
        typeof v === "string"
          ? canonical(JSON.parse(v))
          : k === "field"
            ? ({
                acct: "account",
                description: "payee",
                imported_description: "imported_payee",
              }[v] ?? v)
            : canonical(v),
      ]),
    );
  return value;
}
try {
  const capabilities = (await request("/query-capabilities")).data;
  assert.ok(capabilities.tables.includes("budget_cells"));
  assert.equal(capabilities.max_page, 1000);
  for (const profile of capabilities.resource_profiles) {
    assert.ok(profile.fields.length);
    for (const field of profile.fields)
      assert.ok(
        field.type,
        profile.table + "." + field.name + " advertises a type",
      );
  }
  let tested = 0;
  const failures = [];
  for (const set of fixture.sets) {
    const bytes = await readFile(
      new URL("../tests/fixtures/" + set.archive.filename, import.meta.url),
    );
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      set.archive.sha256,
    );
    const response = await fetch(api + "/imports/actual", {
      method: "POST",
      headers: {
        "Idempotency-Key": randomUUID(),
        "Content-Type": "application/zip",
      },
      body: bytes,
    });
    assert.equal(response.status, 201, await response.clone().text());
    const stage = (await response.json()).data,
      budget = (await request("/imports/" + stage.id + "/activate", {}, 201))
        .data,
      root = "/budgets/" + budget.id,
      checkpoint = (await request(root + "/checkpoint")).data,
      before = await request(root + "/transactions?limit=1000");
    for (const item of set.cases) {
      const selected = process.argv
        .find((v) => v.startsWith("--case="))
        ?.slice(7);
      if (selected && selected !== item.id) continue;
      tested++;
      try {
        const q = item.query,
          method = { zero_budgets: "envelope", reflect_budgets: "tracking" }[
            q.table
          ],
          body = {
            version: 1,
            table: method ? "budget_cells" : q.table,
            select: ["month-literal", "year-literal"].includes(item.id)
              ? [...q.selectExpressions, "id"]
              : q.selectExpressions,
            filter: method
              ? [...q.filterExpressions, { method }]
              : q.filterExpressions,
            group_by: q.groupExpressions,
            order_by: q.orderExpressions,
            calculation: q.calculation,
            splits: q.tableOptions.splits ?? "inline",
            include_deleted: q.withDead,
            validate_refs: q.validateRefs,
            raw: q.rawMode,
            limit: q.limit ?? 1000,
            offset: q.offset ?? 0,
            snapshot: checkpoint.snapshot,
          },
          result = await request(root + "/query", body, item.error ? 400 : 200);
        if (!item.error) {
          const expected = method
            ? item.data.map(({ id, month, carryover, ...cell }) => ({
                ...cell,
                month: String(month).slice(0, 4) + "-" + String(month).slice(4),
                carryover: !!carryover,
              }))
            : item.id === "category-groups"
              ? item.data.map(({ categories, ...group }) => group)
              : item.data;
          assert.deepEqual(
            canonical(result.data),
            canonical(expected),
            item.id + " exact source result",
          );
          if (item.id === "category-groups") {
            for (const group of item.data) {
              const columns = ["id", "name", "is_income", "hidden", "group"],
                children = await request(root + "/query", {
                  version: 1,
                  table: "categories",
                  select: columns,
                  filter: [{ group: group.id }],
                  order_by: ["id"],
                  snapshot: checkpoint.snapshot,
                });
              assert.deepEqual(
                children.data,
                group.categories
                  .map((row) =>
                    Object.fromEntries(columns.map((k) => [k, row[k]])),
                  )
                  .sort((a, b) => a.id.localeCompare(b.id)),
                "Flat group expansion through bounded category profile",
              );
            }
          }
          assert.equal(result.snapshot, checkpoint.snapshot);
        }
        console.log("PASS " + item.id);
      } catch (e) {
        if (!process.argv.includes("--keep-going")) throw e;
        failures.push(item.id);
        console.error("FAIL " + item.id + ": " + String(e).slice(0, 1200));
      }
    }
    if (set.id === "references") {
      const all = await request(root + "/query", {
        version: 1,
        table: "accounts",
        select: ["*"],
        snapshot: checkpoint.snapshot,
      });
      const names = capabilities.resource_profiles
        .find((p) => p.table === "accounts")
        .fields.map((f) => f.name)
        .sort();
      for (const row of all.data)
        assert.deepEqual(
          Object.keys(row).sort(),
          names,
          "Wildcard expands only advertised fields",
        );
      for (const query of [
        { table: "bank_connections", select: ["id"] },
        { table: "accounts", select: ["password"] },
        { table: "categories", filter: [{ goal_def: { $like: "monthly" } }] },
        { table: "categories", order_by: ["goal_def"] },
        {
          table: "categories",
          select: [{ text: { $substr: ["$goal_def", 1, 2] } }],
        },
        { table: "transactions", order_by: [{ $sum: "$amount" }] },
      ])
        await request(
          root + "/query",
          { version: 1, snapshot: checkpoint.snapshot, ...query },
          400,
        );
    }
    assert.deepEqual(await request(root + "/transactions?limit=1000"), before);
    assert.deepEqual((await request(root + "/checkpoint")).data, checkpoint);
  }
  assert.ok(tested);
  assert.deepEqual(failures, []);
  console.log(
    "PASS " + tested + " source resource queries; ledger/checkpoint unchanged",
  );
} finally {
  await server.stop();
}
