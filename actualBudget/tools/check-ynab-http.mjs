// Compare running import results with independently executed pinned Actual imports.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/ynab-source.json", import.meta.url),
    ),
  ),
  server = await startServer(),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function call(path, body, headers = {}) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.ok(
    r.ok,
    `${path}: ${r.status} ${(await r.clone().text()).slice(0, 1000)}`,
  );
  return r.json();
}
const sorted = (rows) =>
  rows.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
function compareLedger(snapshot, actual, refs) {
  const nativeAccounts = new Map(
      refs.accounts.map((a) => [a.id, "account:" + a.name]),
    ),
    nativePayees = new Map(
      refs.payees.map((p) => [
        p.id,
        p.transfer_account_id
          ? "transfer:" + nativeAccounts.get(p.transfer_account_id)
          : "payee:" + p.name,
      ]),
    ),
    nativeCats = new Map(
      refs.categories.map((c) => [
        c.id,
        "category:group:" +
          refs.groups.find((g) => g.id === c.group_id)?.name +
          "/" +
          c.name,
      ]),
    );
  const flatten = (rows, key) =>
      rows.flatMap((row) => [row, ...(row[key] ?? [])]),
    sourceRows = snapshot.ledgers.flatMap((l) =>
      flatten(l.transactions, "subtransactions"),
    ),
    nativeRows = flatten(actual, "children");
  assert.equal(
    new Set(nativeRows.map((r) => r.id)).size,
    nativeRows.length,
    "Every financial row has its own stable identity",
  );
  const project = (rows, source) => {
    const reference = (r, key) =>
      source
        ? (r[
            key === "account_id"
              ? "account"
              : key === "payee_id"
                ? "payee"
                : "category"
          ] ?? null)
        : ((key === "payee_id" && r.transfer_account_id
            ? "transfer:" + nativeAccounts.get(r.transfer_account_id)
            : (key === "account_id"
                ? nativeAccounts
                : key === "payee_id"
                  ? nativePayees
                  : nativeCats
              ).get(r[key])) ?? null);
    const identity = (r) => ({
      account: reference(r, "account_id"),
      date: r.date,
      amount: r.amount,
      notes: source ? "" : (r.notes ?? ""),
      parent: !!r[source ? "is_parent" : "is_parent"],
    });
    const byId = new Map(rows.map((r) => [r.id, identity(r)]));
    return sorted(
      rows.map((r) => ({
        ...identity(r),
        payee: reference(r, "payee_id"),
        category: reference(r, "category_id"),
        cleared: source ? true : !!r.cleared,
        reconciled: source ? false : !!r.reconciled,
        starting_balance:
          !!r[source ? "starting_balance_flag" : "starting_balance"],
        imported_payee: r.imported_payee ?? null,
        imported_id: r.imported_id ?? null,
        parent_reference: r.parent_id ? byId.get(r.parent_id) : null,
        transfer_reference: r.transfer_id ? byId.get(r.transfer_id) : null,
      })),
    );
  };
  assert.deepEqual(
    project(nativeRows, false),
    project(sourceRows, true),
    "Complete financial rows, multiplicity, split parents and reciprocal transfer edges",
  );
  for (const account of refs.accounts) {
    const expected = snapshot.ledgers.find(
      (l) => l.account_id === nativeAccounts.get(account.id),
    );
    assert.ok(expected);
    const ordered = (rows, childKey) =>
      rows.map((row) => ({
        amount: row.amount,
        date: row.date,
        notes: childKey === "subtransactions" ? "" : (row.notes ?? ""),
        parent: !!row.is_parent,
        children: (row[childKey] ?? []).map((child) => ({
          amount: child.amount,
          notes: childKey === "subtransactions" ? "" : (child.notes ?? ""),
        })),
      }));
    assert.deepEqual(
      ordered(
        actual.filter((row) => row.account_id === account.id),
        "children",
      ),
      ordered(expected.transactions, "subtransactions"),
      "Source account row and allocation order",
    );
    assert.equal(
      account.balance,
      expected.balance,
      "Source settled account balance",
    );
  }
  return nativeCats;
}
try {
  for (const item of fixture.cases) {
    const selected = process.argv
      .find((a) => a.startsWith("--case="))
      ?.slice(7);
    if (selected && selected !== item.id) continue;
    const bytes = await readFile(
      new URL("../tests/fixtures/ynab/" + item.input.filename, import.meta.url),
    );
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      item.input.sha256,
    );
    const budgetsBefore = (await call("/budgets")).data,
      key = randomUUID(),
      path = `/imports/${item.type === "ynab4" ? "ynab4" : "nynab"}?filename=${encodeURIComponent(item.filename)}`;
    const upload = async (k) => {
      const r = await fetch(api + path, {
        method: "POST",
        headers: {
          "Idempotency-Key": k,
          "Content-Type":
            item.type === "ynab4" ? "application/zip" : "application/json",
        },
        body: bytes,
      });
      assert.equal(r.status, 201, await r.clone().text());
      return r.json();
    };
    const staged = await upload(key),
      stage = staged.data;
    assert.ok(["ready", "activated"].includes(stage.status));
    assert.equal(
      stage.report.source_format,
      item.type === "ynab4" ? "ynab4" : "nynab",
    );
    assert.equal(
      stage.report.source_field_clock_count,
      0,
      "YNAB entity versions are not invented HLCs",
    );
    assert.deepEqual(
      await upload(key),
      staged,
      "Exact original upload receipt",
    );
    assert.equal(
      (await upload(randomUUID())).data.id,
      stage.id,
      "Repeated original export resolves the same stage",
    );
    assert.deepEqual(
      (await call("/budgets")).data,
      budgetsBefore,
      "Staging creates no active financial budget",
    );
    const original = await fetch(api + `/imports/${stage.id}/original`);
    assert.equal(original.status, 200);
    assert.deepEqual(
      Buffer.from(await original.arrayBuffer()),
      bytes,
      "Original export is retained byte for byte",
    );
    const activationKey = randomUUID(),
      activated = await call(
        `/imports/${stage.id}/activate`,
        {},
        { "Idempotency-Key": activationKey },
      ),
      root = `/budgets/${activated.data.id}`;
    assert.deepEqual(
      await call(
        `/imports/${stage.id}/activate`,
        {},
        { "Idempotency-Key": activationKey },
      ),
      activated,
    );
    const state = await call(root + "/transactions?limit=1000"),
      refs = {
        accounts: (await call(root + "/accounts")).data,
        payees: (await call(root + "/payees")).data,
        categories: (await call(root + "/categories")).data,
      };
    refs.groups = (await call(root + "/category-groups")).data;
    assert.deepEqual(
      refs.payees.map((p) => p.name).sort(),
      item.snapshot.payees
        .filter((p) => !p.transfer_acct)
        .map((p) => p.name)
        .sort(),
      "All baseline ordinary payees remain available, including unused source records",
    );
    const cats = compareLedger(item.snapshot, state.data, refs);
    for (const expected of item.snapshot.months.filter((m) => m.data)) {
      const response = (await call(root + "/budget?month=" + expected.month))
        .data;
      assert.ok(response.months[expected.month]);
      const view = response.months[expected.month];
      // Tracking keeps source allocations and actuals; missing cells carry by default.
      // Envelope-only available-funds/buffering totals have no current equivalent.
      const cells = (
        await call(root + "/query", {
          version: 1,
          table: "budget_cells",
          select: ["category", "carryover"],
          filter: [{ month: expected.month, method: "tracking" }],
        })
      ).data;
      const [year, month] = expected.month.split("-").map(Number),
        priorMonth =
          month === 1
            ? `${year - 1}-12`
            : `${year}-${String(month - 1).padStart(2, "0")}`;
      const previous = (await call(root + "/budget?month=" + priorMonth)).data
        .months[priorMonth];
      for (const group of expected.data.categoryGroups)
        for (const category of group.categories) {
          const row = view.categories.find(
            (r) => cats.get(r.id) === category.id,
          );
          assert.ok(row, category.id);
          assert.deepEqual(
            {
              budgeted: row.budgeted,
              actual: row.actual,
              balance: row.balance,
              carryover: row.carryover,
            },
            {
              budgeted: category.budgeted ?? 0,
              actual: category.received ?? category.spent ?? 0,
              balance:
                (category.budgeted ?? 0) +
                (row.is_income ? -1 : 1) *
                  (category.received ?? category.spent ?? 0) +
                (previous.categories.find((p) => p.id === row.id)?.carryover
                  ? previous.categories.find((p) => p.id === row.id).balance
                  : 0),
              carryover: cells.some((cell) => cell.category === row.id)
                ? (category.carryover ?? false)
                : true,
            },
            `${expected.month}:${category.id}`,
          );
        }
      const keys = {
        total_budgeted: "totalBudgeted",
        total_income: "totalIncome",
        total_spent: "totalSpent",
      };
      for (const [native, source] of Object.entries(keys)) {
        assert.equal(typeof expected.data[source], "number", source);
        assert.equal(
          view.summary[native],
          native === "total_budgeted"
            ? -expected.data[source] || 0
            : expected.data[source],
          `${expected.month}:${native}`,
        );
      }
    }
    if (item.id === "ynab4-demo") {
      const learning = JSON.parse(
          await readFile(
            new URL("../tests/fixtures/ynab-learning.json", import.meta.url),
          ),
        ),
        checkpoint = (await call(root + "/checkpoint")).data,
        ledgerBefore = await call(root + "/transactions?limit=1000");
      for (const expected of learning.effects) {
        const payee = refs.payees.find(
          (p) => p.name === expected.payee && !p.transfer_account_id,
        );
        assert.ok(payee, expected.payee);
        const transaction = {
          id: randomUUID(),
          account_id: refs.accounts[0].id,
          date: "2026-11-09",
          amount: -100,
          payee_id: payee.id,
          category_id: null,
          notes: "",
          cleared: false,
        };
        const preview = (
          await call(root + "/rules/preview", {
            snapshot: checkpoint.snapshot,
            transaction,
          })
        ).data;
        const after = preview.transactions[0]?.after ?? transaction;
        const category = after.category_id
          ? refs.categories.find((c) => c.id === after.category_id)
          : null;
        const name = category
          ? refs.groups.find((g) => g.id === category.group_id).name +
            "/" +
            category.name
          : null;
        assert.equal(
          name,
          expected.category,
          "Baseline learned category for " + expected.payee,
        );
        assert.equal(after.amount, expected.amount);
        assert.equal(after.notes, "");
      }
      assert.deepEqual(
        await call(root + "/transactions?limit=1000"),
        ledgerBefore,
      );
      assert.deepEqual(
        (await call(root + "/checkpoint")).data,
        checkpoint,
        "Learned-rule preview remains read-only",
      );
    }

    if (item.id === "nynab-demo") {
      const behavior = JSON.parse(
          await readFile(
            new URL("../tests/fixtures/nynab-behavior.json", import.meta.url),
          ),
        ),
        checkpoint = (await call(root + "/checkpoint")).data,
        page = (await call(root + "/schedules?limit=100")).data;
      assert.equal(page.next_offset, null);
      assert.equal(page.schedules.length, behavior.definitions.length);
      const labels = new Map([
        ...refs.accounts.map((a) => [a.id, "account:" + a.name]),
        ...(await call(root + "/payees?include_transfers=true")).data.map(
          (p) => [
            p.id,
            p.transfer_account_id
              ? "transfer:account:" +
                refs.accounts.find((a) => a.id === p.transfer_account_id).name
              : "payee:" + p.name,
          ],
        ),
        ...refs.categories.map((c) => [
          c.id,
          "category:" +
            refs.groups.find((g) => g.id === c.group_id).name +
            "/" +
            c.name,
        ]),
        ...page.schedules.map((s) => [s.id, "schedule:" + s.name]),
      ]);
      const normalize = (v) =>
        Array.isArray(v)
          ? v.map(normalize)
          : v && typeof v === "object"
            ? Object.fromEntries(
                Object.entries(v)
                  .filter(
                    ([k, x]) =>
                      !(k === "type" && "op" in v) &&
                      !(k === "field" && x === null),
                  )
                  .map(([k, x]) => [k, normalize(x)]),
              )
            : (labels.get(v) ?? v);
      for (const expected of behavior.definitions) {
        const row = page.schedules.find((s) => s.name === expected.name);
        assert.ok(row, expected.name);
        assert.deepEqual(
          normalize(row.conditions),
          normalize(expected.conditions),
          "Exact imported schedule predicates",
        );
        assert.deepEqual(
          normalize(row.actions),
          normalize(
            expected.actions.filter(
              (action) =>
                action.field !== "notes" &&
                !["append-notes", "prepend-notes"].includes(action.op),
            ),
          ),
          "Exact schedule actions and indexed split allocations",
        );
        assert.equal(row.posts_transaction, false);
        assert.equal(row.completed, false);
        const next =
          typeof expected.date === "string"
            ? expected.date
            : expected.dates.find((d) => d >= page.today);
        assert.ok(
          next,
          "Current date within independently observed schedule calendar range",
        );
        assert.equal(row.next_date, next, expected.name);
      }
      assert.equal(
        (await fetch(api + root + "/tags?include_hidden=true")).status,
        404,
      );
      for (const expected of behavior.effects) {
        const p = refs.payees.find((p) => "payee:" + p.name === expected.payee);
        assert.ok(p);
        const transaction = {
          id: randomUUID(),
          account_id: refs.accounts[0].id,
          date: "2026-11-09",
          amount: -100,
          payee_id: p.id,
          category_id: null,
          notes: "",
          cleared: false,
        };
        const response = (
          await call(root + "/rules/preview", {
            snapshot: checkpoint.snapshot,
            transaction,
          })
        ).data;
        const after = response.transactions[0]?.after ?? transaction;
        assert.equal(
          after.category_id ? labels.get(after.category_id) : null,
          expected.category,
        );
        assert.equal(after.amount, expected.amount);
        assert.equal(after.notes, "");
      }
      assert.deepEqual(
        (await call(root + "/checkpoint")).data,
        checkpoint,
        "Schedule/tag/default observations are read-only",
      );
    }
    const before = await call(root + "/checkpoint");
    await call(`/imports/${stage.id}/activate`, {});
    assert.deepEqual(await call(root + "/transactions?limit=1000"), state);
    assert.deepEqual(
      await call(root + "/checkpoint"),
      before,
      "New activation key cannot reinstall source history",
    );
    assert.deepEqual(
      Buffer.from(
        await (
          await fetch(api + `/imports/${stage.id}/original`)
        ).arrayBuffer(),
      ),
      bytes,
    );
    console.log(
      "PASS " + item.id + " full financial graph and historical budgets",
    );
  }
  if (
    !process.argv.some((a) => a.startsWith("--case=")) ||
    process.argv.includes("--case=nynab-income")
  ) {
    const income = JSON.parse(
      await readFile(
        new URL("../tests/fixtures/nynab-income.json", import.meta.url),
      ),
    );
    for (const expected of income.cases) {
      const bytes = await readFile(
        new URL("../tests/fixtures/ynab/" + expected.input, import.meta.url),
      );
      assert.equal(
        createHash("sha256").update(bytes).digest("hex"),
        expected.input_sha256,
      );
      const r = await fetch(api + "/imports/nynab?filename=" + expected.input, {
        method: "POST",
        headers: { "Idempotency-Key": randomUUID() },
        body: bytes,
      });
      assert.equal(r.status, 201, await r.clone().text());
      const stage = (await r.json()).data,
        activated = (await call("/imports/" + stage.id + "/activate", {})).data,
        root = "/budgets/" + activated.id,
        rows = (await call(root + "/transactions?limit=1000")).data,
        categories = (await call(root + "/categories")).data,
        accounts = (await call(root + "/accounts")).data;
      assert.equal(rows.length, 1);
      assert.equal(rows[0].amount, expected.amount);
      assert.equal(rows[0].payee_id, expected.payee);
      assert.equal(rows[0].starting_balance, expected.starting_balance_flag);
      assert.equal(
        categories.find((c) => c.id === rows[0].category_id).name,
        expected.category,
        "Null payee never aliases an absent Starting Balance payee",
      );
      assert.equal(accounts[0].balance, expected.balance);
      const month = (await call(root + "/budget?month=2023-08")).data.months[
        "2023-08"
      ];
      assert.equal(
        month.categories
          .filter((c) => c.is_income)
          .reduce((n, c) => n + c.actual, 0),
        expected.amount,
        "Income totals remain exact",
      );
      console.log(
        "PASS " + expected.id + " genuine source category and amount",
      );
    }
  }

  if (
    !process.argv.some((a) => a.startsWith("--case=")) ||
    process.argv.includes("--case=nynab-names")
  ) {
    const names = JSON.parse(
      await readFile(
        new URL("../tests/fixtures/nynab-names.json", import.meta.url),
      ),
    );
    for (const expected of names.cases) {
      const bytes = await readFile(
        new URL("../tests/fixtures/ynab/" + expected.input, import.meta.url),
      );
      assert.equal(
        createHash("sha256").update(bytes).digest("hex"),
        expected.input_sha256,
      );
      const r = await fetch(api + "/imports/nynab?filename=" + expected.input, {
        method: "POST",
        headers: { "Idempotency-Key": randomUUID() },
        body: bytes,
      });
      assert.equal(r.status, 201, await r.clone().text());
      const stage = (await r.json()).data,
        activated = (await call("/imports/" + stage.id + "/activate", {})).data,
        root = "/budgets/" + activated.id,
        accounts = (await call(root + "/accounts")).data,
        payees = (await call(root + "/payees?include_transfers=true")).data,
        categories = (await call(root + "/categories")).data,
        groups = (await call(root + "/category-groups")).data,
        checkpoint = await call(root + "/checkpoint"),
        page = (await call(root + "/schedules?limit=100")).data;
      assert.equal(page.next_offset, null);
      const ids = new Map([
        ...accounts.map((a) => [a.id, "account:" + a.name]),
        ...payees.map((p) => [
          p.id,
          p.transfer_account_id
            ? "transfer:account:" +
              accounts.find((a) => a.id === p.transfer_account_id).name
            : "payee:" + p.name,
        ]),
        ...categories.map((c) => [
          c.id,
          "category:" +
            groups.find((g) => g.id === c.group_id).name +
            "/" +
            c.name,
        ]),
        ...page.schedules.map((s) => [s.id, "schedule:" + s.amount]),
      ]);
      const normalize = (v) =>
        Array.isArray(v)
          ? v.map(normalize)
          : v && typeof v === "object"
            ? Object.fromEntries(
                Object.entries(v)
                  .filter(
                    ([k, x]) =>
                      !(k === "type" && "op" in v) &&
                      !(k === "field" && x === null),
                  )
                  .map(([k, x]) => [k, normalize(x)]),
              )
            : (ids.get(v) ?? v);
      const actual = page.schedules
        .map((row) =>
          normalize({
            name: row.name,
            amount: row.amount,
            date: row.conditions.find((c) => c.field === "date").value,
            next_date: row.next_date,
            posts_transaction: row.posts_transaction,
            completed: row.completed,
            conditions: row.conditions,
            actions: row.actions,
          }),
        )
        .sort((a, b) => a.amount - b.amount);
      assert.deepEqual(
        actual,
        normalize(
          expected.definitions.map((row) => ({
            ...row,
            actions: row.actions.filter(
              (action) =>
                action.field !== "notes" &&
                !["append-notes", "prepend-notes"].includes(action.op),
            ),
          })),
        ),
        "Exact null/trim/case schedule names, predicates and financial actions",
      );
      assert.deepEqual(
        (await call(root + "/transactions?limit=1000")).data,
        [],
        "Manual schedules never post imported money",
      );
      assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
      console.log(
        "PASS " + expected.id + " exact source definitions without posting",
      );
    }
  }
  if (
    !process.argv.some((a) => a.startsWith("--case=")) ||
    process.argv.includes("--case=ynab4-carry")
  ) {
    const carry = JSON.parse(
      await readFile(
        new URL("../tests/fixtures/ynab4-carry.json", import.meta.url),
      ),
    );
    for (const expected of carry.cases) {
      const bytes = await readFile(
        new URL("../tests/fixtures/ynab/" + expected.input, import.meta.url),
      );
      assert.equal(
        createHash("sha256").update(bytes).digest("hex"),
        expected.input_sha256,
      );
      const r = await fetch(api + "/imports/ynab4?filename=" + expected.input, {
        method: "POST",
        headers: { "Idempotency-Key": randomUUID() },
        body: bytes,
      });
      assert.equal(r.status, 201, await r.clone().text());
      const stage = (await r.json()).data,
        active = (await call("/imports/" + stage.id + "/activate", {})).data,
        root = "/budgets/" + active.id,
        groups = (await call(root + "/category-groups")).data,
        categories = (await call(root + "/categories")).data;
      const ledger = await call(root + "/transactions?limit=1000");
      // Recompute tracking carry from independent source monthly amounts and explicit carry flags.
      const trackingBalances = new Map();
      for (const source of expected.months) {
        const view = (await call(root + "/budget?month=" + source.month)).data
          .months[source.month];
        const cells = (
          await call(root + "/query", {
            version: 1,
            table: "budget_cells",
            select: ["category", "carryover"],
            filter: [{ month: source.month, method: "tracking" }],
          })
        ).data;
        for (const c of source.categories) {
          const category = categories.find(
            (row) =>
              groups.find((g) => g.id === row.group_id).name +
                "/" +
                row.name ===
              c.category,
          );
          assert.ok(category, c.category);
          const row = view.categories.find((row) => row.id === category.id);
          const carryover = cells.some((cell) => cell.category === category.id)
            ? c.carryover
            : true;
          const previous = trackingBalances.get(category.id);
          const balance =
            c.budgeted +
            (category.is_income ? -c.actual : c.actual) +
            (previous?.carryover ? previous.balance : 0);
          trackingBalances.set(category.id, { balance, carryover });
          assert.deepEqual(
            {
              category: c.category,
              budgeted: row.budgeted,
              actual: row.actual,
              balance: row.balance,
              carryover: row.carryover,
            },
            { ...c, balance, carryover },
            expected.id + ":" + source.month + ":" + c.category,
          );
        }
        const keys = {
          total_budgeted: "totalBudgeted",
          total_income: "totalIncome",
          total_spent: "totalSpent",
        };
        for (const [k, v] of Object.entries(keys))
          assert.equal(
            view.summary[k],
            k === "total_budgeted"
              ? -source.summary[v] || 0
              : source.summary[v],
            source.month + ":" + k,
          );
      }
      assert.deepEqual(await call(root + "/transactions?limit=1000"), ledger);
      console.log(
        "PASS YNAB4 " + expected.id + " exact source gap/carry/reset budgets",
      );
    }
  }
} finally {
  await server.stop();
}
