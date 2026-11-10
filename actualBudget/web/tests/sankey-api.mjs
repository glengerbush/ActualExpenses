// Public HTTP tracer against unchanged pinned frontend/budget/CSV expectations.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { startServer } from "./server.mjs";
import { isolatedImportBytes } from "./import-fixture.mjs";
const server = await startServer();
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
  const page = await browser.newPage();
  await page.goto(server.url);
  const api = (path, body, headers = {}, method) =>
    page.evaluate(
      async ({ path, body, headers, method }) => {
        const response = await fetch("/api/v1" + path, {
          method: method || (body === undefined ? "GET" : "POST"),
          headers: { "Content-Type": "application/json", ...headers },
          body: body === undefined ? undefined : JSON.stringify(body),
        });
        return {
          status: response.status,
          ...(await response.json().catch(() => ({}))),
        };
      },
      { path, body, headers, method },
    );
  for (const method of ["envelope", "tracking"]) {
    const source = JSON.parse(
        await readFile(
          new URL(
            "../../tests/fixtures/sankey-" + method + "-source-26.10.json",
            import.meta.url,
          ),
          "utf8",
        ),
      ),
      bytes = isolatedImportBytes(
        await readFile(
          new URL(
            "../../tests/fixtures/sankey-" + method + "-source-26.10.zip",
            import.meta.url,
          ),
        ),
      ).toString("base64");
    const stage = await page.evaluate(
      async ({ bytes, key }) => {
        const r = await fetch("/api/v1/imports/actual", {
          method: "POST",
          headers: {
            "Content-Type": "application/zip",
            "Idempotency-Key": key,
          },
          body: Uint8Array.from(atob(bytes), (c) => c.charCodeAt(0)),
        });
        return { status: r.status, ...(await r.json()) };
      },
      { bytes, key: randomUUID() },
    );
    assert.equal(stage.status, 201, JSON.stringify(stage));
    assert.ok(
      ["ready", "activated"].includes(stage.data.status),
      JSON.stringify(stage),
    );
    if (stage.data.status === "ready")
      assert.equal(
        (
          await api(
            "/imports/" + stage.data.id + "/activate",
            {},
            { "Idempotency-Key": randomUUID() },
          )
        ).status,
        201,
      );
    const original = "/budgets/" + stage.data.budget_id,
      registry = await api(original + "/reporting"),
      id = randomUUID();
    assert.equal(
      (
        await api(
          original + "/duplicate",
          { id, name: "Sankey proof " + id },
          {
            "Idempotency-Key": randomUUID(),
            "X-Actual-Budget-Epoch": registry.data.epoch,
          },
        )
      ).status,
      201,
    );
    const root = "/budgets/" + id;
    const state = await api(root + "/reporting");
    assert.deepEqual(
      state.data.documents.find((d) => d.id === source.widgets[0].id).config
        .meta,
      source.widgets[0].meta,
    );
    assert.equal(
      (await api(root + "/preferences")).data.experimental.sankeyReport,
      true,
    );
    const companion = await api(
      root + "/companion?account_id=" + source.ids.checking,
    );
    const extra =
      method === "envelope"
        ? JSON.parse(
            await readFile(
              new URL(
                "../../tests/fixtures/sankey-details-source-26.10.json",
                import.meta.url,
              ),
              "utf8",
            ),
          ).cases
        : [];
    for (const oracle of [...source.cases, ...extra]) {
      const response = await api(root + "/reports/sankey", {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: oracle.options,
      });
      assert.equal(
        response.status,
        200,
        JSON.stringify({ case: oracle.name, ...response }),
      );
      assert.deepEqual(
        response.data.base_graph,
        oracle.baseGraph,
        oracle.name + " ordered base graph",
      );
      assert.deepEqual(
        response.data.result,
        oracle.result,
        oracle.name + " full source nodes/links/Other/percentages",
      );
    }
    const detailed =
      method === "envelope"
        ? JSON.parse(
            await readFile(
              new URL(
                "../../tests/fixtures/sankey-details-source-26.10.json",
                import.meta.url,
              ),
              "utf8",
            ),
          )
        : null;
    async function inspect(oracle, link) {
      const query = {
          version: 1,
          as_of: source.clock.slice(0, 10),
          options: oracle.options,
        },
        view = await api(root + "/reports/sankey", query);
      assert.equal(view.status, 200);
      const rows = [];
      let offset = 0,
        page;
      do {
        page = await api(root + "/reports/sankey/drilldown", {
          query,
          snapshot: view.data.snapshot,
          link,
          offset,
          limit: 2,
        });
        assert.equal(page.status, 200, JSON.stringify(page));
        rows.push(...page.data.rows);
        offset += 2;
      } while (page.data.has_more);
      assert.equal(rows.length, page.data.total_count);
      assert.equal(
        rows.reduce((sum, row) => sum + row.amount, 0),
        page.data.total_amount,
      );
      return { ...page.data, rows, view: view.data, query };
    }
    const plannedCase = source.cases.find((c) => c.name === "budgeted"),
      planned = await inspect(plannedCase, {
        source: source.ids.expenses,
        target: source.ids.food,
      });
    const dto = source.traces.find(
        (t) => t.command === "api/budget-month" && t.args.month === "2026-11",
      ).data,
      sourcePlan = dto.categoryGroups
        .flatMap((g) => g.categories)
        .find((c) => c.id === source.ids.food).budgeted;
    assert.equal(planned.total_amount, sourcePlan);
    assert.equal(planned.rows.length, 1);
    assert.equal(planned.rows[0].basis, "budget_allocation");
    assert.equal(planned.rows[0].original_amount, sourcePlan);
    assert.equal(planned.rows[0].month, "2026-11");
    if (detailed) {
      const spentCase = source.cases.find((c) => c.name === "spent"),
        spending = await inspect(spentCase, {
          source: source.ids.expenses,
          target: source.ids.food,
        });
      const chosen = detailed.entry_groups.filter(
          (e) =>
            !e.isIncome && e.isNegative && e.categoryId === source.ids.food,
        ),
        expected = detailed.ledger.filter(
          (r) =>
            r.date >= "2026-11-01" &&
            r.date <= "2026-11-30" &&
            chosen.some(
              (e) => e.categoryId === r.category && e.accountId === r.account,
            ),
        );
      assert.equal(
        spending.total_amount,
        chosen.reduce((sum, e) => sum + e.value, 0),
      );
      assert.deepEqual(
        spending.rows.map((r) => r.id).sort(),
        expected.map((r) => r.id).sort(),
      );
      for (const row of spending.rows) {
        const original = expected.find((r) => r.id === row.id);
        assert.equal(row.original_amount, original.amount);
        assert.equal(row.amount, -original.amount);
        assert.equal(row.notes, original.notes ?? "");
        assert.equal(row.payee_name, original.payee_name ?? "");
        assert.equal(row.basis, "recorded_entry");
      }
      const otherCase = source.cases.find((c) => c.name === "small-global"),
        other = await inspect(otherCase, {
          source: "category_group__OTHER_BUCKET",
          target: "category__OTHER_BUCKET",
        });
      const otherChosen = detailed.entry_groups.filter(
          (e) => !e.isIncome && e.isNegative,
        ),
        otherExpected = detailed.ledger.filter(
          (r) =>
            r.date >= "2026-11-01" &&
            r.date <= "2026-11-30" &&
            otherChosen.some(
              (e) => e.categoryId === r.category && e.accountId === r.account,
            ),
        );
      assert.equal(
        other.total_amount,
        otherChosen.reduce((sum, e) => sum + e.value, 0),
      );
      assert.deepEqual(
        other.rows.map((r) => r.id).sort(),
        otherExpected.map((r) => r.id).sort(),
      );
      const transferCase = source.cases.find((c) => c.name === "transfers"),
        pair = detailed.transfer_pairs.find(
          (p) =>
            p.fromAccountId === source.ids.checking &&
            p.toAccountId === source.ids.savings,
        ),
        transfer = await inspect(transferCase, {
          source: pair.fromAccountId,
          target: pair.toAccountId,
        });
      const raw = new Map(detailed.ledger.map((r) => [r.id, r])),
        legs = detailed.ledger.filter((r) => {
          const peer = raw.get(r.transfer_id);
          return (
            peer &&
            r.date >= "2026-11-01" &&
            r.date <= "2026-11-30" &&
            peer.date >= "2026-11-01" &&
            peer.date <= "2026-11-30" &&
            r.account < peer.account &&
            [r.account, peer.account].sort().join() ===
              [pair.fromAccountId, pair.toAccountId].sort().join()
          );
        });
      assert.equal(transfer.total_amount, pair.amount);
      assert.deepEqual(
        transfer.rows.map((r) => r.id).sort(),
        legs.map((r) => r.id).sort(),
      );
      assert.ok(
        transfer.rows.some((r) => r.amount < 0),
        "Opposing transfer reduces net flow",
      );
      for (const row of transfer.rows) {
        assert.equal(row.original_amount, raw.get(row.id).amount);
        assert.equal(row.peer_id, raw.get(row.id).transfer_id);
        assert.equal(row.basis, "net_transfer");
      }
      const previous = await inspect(plannedCase, {
          source: "from_previous_month",
          target: "available_income",
        }),
        expectedPrevious = plannedCase.baseGraph.find(
          (n) => n.key === "from_previous_month",
        ).to[0][1];
      assert.equal(previous.total_amount, expectedPrevious);
      assert.equal(previous.rows[0].basis, "budget_context");
      assert.ok(previous.rows[0].id.startsWith("context:"));
      assert.equal(previous.rows[0].original_amount, expectedPrevious);
    }
    assert.deepEqual(
      await api(root + "/companion?account_id=" + source.ids.checking),
      companion,
    );
    assert.deepEqual((await api(root + "/reporting")).data, state.data);
    const headers = () => ({
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": state.data.epoch,
    });
    const baseline = {
        version: 1,
        as_of: source.clock.slice(0, 10),
        options: source.cases[0].options,
      },
      first = await api(root + "/reports/sankey", baseline);
    const visual = await api(root + "/reports/sankey", {
      ...baseline,
      options: {
        ...baseline.options,
        topNcategories: 100000,
        categorySort: "global",
        showPercentages: true,
        groupAccounts: true,
      },
    });
    assert.equal(visual.status, 200);
    assert.equal(visual.data.cache.hit, true);
    assert.equal(
      visual.data.cache.input_revision,
      first.data.cache.input_revision,
    );
    const renamed = structuredClone(
      state.data.documents.find((d) => d.id === source.widgets[0].id),
    );
    renamed.config.meta.name = "Unchanged flow inputs";
    assert.equal(
      (
        await api(
          root + "/reporting/commands",
          { writes: [renamed] },
          headers(),
        )
      ).status,
      200,
    );
    const afterName = await api(root + "/reports/sankey", baseline);
    assert.equal(afterName.data.cache.hit, true);
    assert.equal(
      afterName.data.cache.input_revision,
      first.data.cache.input_revision,
    );
    assert.notEqual(afterName.data.snapshot, first.data.snapshot);
    assert.deepEqual(afterName.data.result, first.data.result);
    assert.equal(
      (
        await api(root + "/reports/sankey/drilldown", {
          query: planned.query,
          snapshot: planned.view.snapshot,
          link: { source: source.ids.expenses, target: source.ids.food },
        })
      ).status,
      409,
    );
    assert.equal(
      (
        await api(
          root + "/preferences",
          { experimental: { sankeyReport: false } },
          headers(),
          "PATCH",
        )
      ).status,
      200,
    );
    assert.equal((await api(root + "/reports/sankey", baseline)).status, 409);
    assert.equal(
      (
        await api(
          root + "/preferences",
          { experimental: { sankeyReport: true } },
          headers(),
          "PATCH",
        )
      ).status,
      200,
    );
    assert.equal(
      (await api(root + "/reports/sankey", baseline)).data.cache.hit,
      true,
    );
    for (const extra of [
      { version: 2 },
      { options: { ...baseline.options, topNcategories: 0 } },
      { options: { ...baseline.options, mode: "unknown" } },
    ])
      assert.equal(
        (await api(root + "/reports/sankey", { ...baseline, ...extra })).status,
        400,
      );
    const budgetQuery = { ...baseline, options: plannedCase.options },
      beforeNote = await api(root + "/reports/sankey", budgetQuery);
    let budgetView = (await api(root + "/budget?month=2026-11")).data;
    assert.equal(
      (
        await api(
          root + "/budget",
          {
            action: "notes",
            month: "2026-11",
            notes: "Display note only",
            snapshot: budgetView.snapshot,
          },
          headers(),
        )
      ).status,
      200,
    );
    const noted = await api(root + "/reports/sankey", budgetQuery);
    assert.equal(noted.data.cache.hit, true);
    assert.equal(
      noted.data.cache.input_revision,
      beforeNote.data.cache.input_revision,
    );
    assert.deepEqual(noted.data.result, beforeNote.data.result);
    budgetView = (await api(root + "/budget?month=2026-11")).data;
    assert.equal(
      (
        await api(
          root + "/budget",
          {
            action: "set",
            month: "2026-11",
            category: source.ids.food,
            amount: sourcePlan + 1,
            snapshot: budgetView.snapshot,
          },
          headers(),
        )
      ).status,
      200,
    );
    const allocated = await api(root + "/reports/sankey", budgetQuery);
    assert.equal(allocated.data.cache.hit, false);
    assert.notEqual(
      allocated.data.cache.input_revision,
      noted.data.cache.input_revision,
    );
    assert.notDeepEqual(allocated.data.result, noted.data.result);
    if (method === "envelope") {
      const unchanged = await api(root + "/reports/sankey", baseline);
      assert.equal(unchanged.data.cache.hit, true);
      assert.deepEqual(unchanged.data.result, first.data.result);
    }
    console.log(
      "PASS source Sankey " +
        method +
        " " +
        (source.cases.length + extra.length) +
        " base/final graphs, source contribution details/paging, readonly companion, cache/notes/financial invalidation, opt-in/stale/bounds",
    );
  }
} finally {
  await browser?.close();
  await server.stop();
}
