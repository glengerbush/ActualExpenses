import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { startServer } from "../web/tests/server.mjs";
let fixture = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-import-edges.json",
        import.meta.url,
      ),
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
  assert.ok(r.ok, `${path}: ${r.status} ${await r.clone().text()}`);
  return r.json();
}
function shape(rows, item, source) {
  return rows
    .flatMap((r) => [
      r,
      ...(source ? (r.subtransactions ?? []) : (r.children ?? [])),
    ])
    .map((r) => ({
      id: r.id.startsWith("25290000-") ? r.id : "<new>",
      parent: !!r.parent_id,
      is_parent: !!r.is_parent,
      account: r[source ? "account" : "account_id"],
      date: r.date,
      amount: r.amount,
      payee: r[source ? "payee" : "payee_id"] ?? null,
      category: r[source ? "category" : "category_id"] ?? null,
      cleared: source ? true : !!r.cleared,
      reconciled: source ? false : !!r.reconciled,
      notes: source ? "" : (r.notes ?? ""),
      imported_id: r.imported_id ?? null,
      imported_payee: r.imported_payee ?? null,
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
const failures = [];
try {
  for (const item of fixture.cases) {
    await server.stop();
    server = await startServer();
    api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    try {
      const bytes = await readFile(
        new URL(
          "../tests/fixtures/transaction-import-edges/" + item.archive.filename,
          import.meta.url,
        ),
      );
      assert.equal(
        createHash("sha256").update(bytes).digest("hex"),
        item.archive.sha256,
      );
      const staged = await fetch(api + "/imports/actual", {
        method: "POST",
        headers: {
          "Idempotency-Key": randomUUID(),
          "Content-Type": "application/zip",
        },
        body: bytes,
      });
      assert.equal(staged.status, 201);
      const stage = (await staged.json()).data;
      assert.ok(["ready", "activated"].includes(stage.status));
      stage.budget_id = (
        await call(`/imports/${stage.id}/activate`, {})
      ).data.id;
      const root = `/budgets/${stage.budget_id}`;
      assert.deepEqual(
        shape((await call(root + "/transactions")).data, item, false),
        shape(item.before.transactions, item, true),
      );
      const escape = (s) =>
        String(s ?? "")
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;");
      const xml =
        "<OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>" +
        item.input.rows
          .map(
            (row) =>
              "<STMTTRN><DTPOSTED>" +
              row.date.replaceAll("-", "") +
              "</DTPOSTED><TRNAMT>" +
              row.amount +
              "</TRNAMT><NAME>" +
              escape(row.payee_name) +
              "</NAME><MEMO>" +
              escape(row.notes) +
              "</MEMO>" +
              (row.imported_id
                ? "<FITID>" + row.imported_id + "</FITID>"
                : "") +
              "</STMTTRN>",
          )
          .join("") +
        "</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";
      const upload = await fetch(
        `${api}${root}/accounts/${item.ids.account}/transaction-imports?filename=edges.ofx`,
        {
          method: "POST",
          headers: { "Idempotency-Key": randomUUID() },
          body: xml,
        },
      );
      assert.equal(upload.status, 201);
      const path =
        root + "/transaction-imports/" + (await upload.json()).data.id;
      let firstIds;
      for (const [round, expected] of [item.after, item.repeated].entries()) {
        const cp = (await call(root + "/checkpoint")).data,
          before = await call(root + "/transactions");
        const base = {
          snapshot: cp.snapshot,
          options: { import_notes: true, reimport_deleted: false },
        };
        const full = (await call(path + "/preview", base)).data;
        assert.deepEqual(full.errors, []);
        const choices = item.input.rows.map((row, i) => ({
          row: i,
          action:
            !row.selected && !row.ignored
              ? "skip"
              : full.rows[i].match_id
                ? "merge"
                : "add",
          ...(full.rows[i].match_id ? { match_id: full.rows[i].match_id } : {}),
        }));
        if (
          round === 0 &&
          ["file-order", "same-bank-id-twice"].includes(item.id)
        )
          choices.reverse();
        const body = { ...base, rows: choices };
        const preview = (await call(path + "/preview", body)).data;
        const expectedIncluded = item.input.rows
          .map((row, i) => (row.selected || row.ignored ? i : null))
          .filter((i) => i !== null);
        assert.deepEqual(
          preview.rows.map((row) => row.row),
          expectedIncluded,
          "Preview matches only the finalized selection; ignored rows remain included",
        );
        for (const row of preview.rows) {
          const choice = choices.find((choice) => choice.row === row.row);
          choice.action =
            round === 0 ? (row.match_id ? "merge" : "add") : "auto";
          if (row.match_id) choice.match_id = row.match_id;
          else delete choice.match_id;
        }
        assert.deepEqual(await call(root + "/transactions"), before);
        assert.deepEqual((await call(root + "/checkpoint")).data, cp);
        const headers = {
          "Idempotency-Key": randomUUID(),
          "X-Actual-Budget-Epoch": cp.epoch,
        };
        const result = await call(path + "/commit", body, headers);
        const rows = (await call(root + "/transactions")).data;
        assert.deepEqual(
          shape(rows, item, false),
          shape(expected.transactions, item, true),
          "Complete independent financial state",
        );
        if (item.id === "file-order")
          assert.deepEqual(
            rows.map((row) => [row.date, row.amount, row.notes]),
            expected.transactions.map((row) => [row.date, row.amount, ""]),
            "Observable input-file order survives",
          );
        const ids = rows
          .flatMap((row) => [row, ...(row.children ?? [])])
          .map((row) => row.id)
          .sort();
        if (round === 0) firstIds = ids;
        else assert.deepEqual(ids, firstIds);
        assert.deepEqual(await call(path + "/commit", body, headers), result);
        assert.deepEqual((await call(root + "/transactions")).data, rows);
      }
      console.log("PASS " + item.id);
    } catch (error) {
      failures.push(item.id);
      console.error("FAIL " + item.id + ": " + error.message);
    }
  }
  assert.equal(failures.length, 0, failures.join(","));
  console.log("PASS five independent selection, ID and file-order workflows");
} finally {
  await server.stop();
}
