// Immutable genuine22 snapshots supply expectations; no database rewriting.
import assert from "node:assert/strict";
import { readFile, mkdtemp, copyFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-schema22.json", import.meta.url),
  ),
);
const linked = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-linked-source-26.10.json", import.meta.url),
  ),
).cases[0];
const provider = createServer((req, res) => {
  if (req.method === "POST") {
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  const url = new URL(req.url, "http://fixture.invalid");
  const rows = linked.input.rounds[0].map((t) => ({
    id: t.transactionId,
    posted: t.booked ? Date.parse(t.date + "T00:00:00Z") / 1000 : 0,
    transacted_at: Date.parse(t.date + "T00:00:00Z") / 1000,
    pending: !t.booked,
    amount: t.transactionAmount.amount,
    payee: t.payeeName,
    description: t.notes,
  }));
  res.setHeader("Content-Type", "application/json");
  res.end(
    JSON.stringify({
      accounts: [
        {
          id: "synthetic-remote-account",
          org: { id: "discovered-institution", name: "Synthetic institution" },
          name: "Source checking",
          currency: "USD",
          balance: "100.00",
          "balance-date": 1794225600,
          transactions: url.searchParams.has("balances-only") ? [] : rows,
        },
      ],
    }),
  );
});
await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
process.env.ACTUALBUDGET_BANK_TEST_ORIGIN = `http://127.0.0.1:${provider.address().port}`;
try {
  for (const mode of ["database", "archive"]) {
    const directory = await mkdtemp(`${tmpdir()}/bank-upgrade-`);
    if (mode === "database")
      await copyFile(
        new URL("../tests/fixtures/bank-schema22.sqlite3", import.meta.url),
        `${directory}/actualbudget.sqlite3`,
      );
    const server = await startServer({ dataDirectory: directory });
    const api =
      server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    const root = `/budgets/${fixture.budget_id}`;
    async function call(
      path,
      body,
      headers = {},
      status = 200,
      method = "POST",
    ) {
      const response = await fetch(api + path, {
        method: body === undefined ? "GET" : method,
        headers: { "Content-Type": "application/json", ...headers },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      const result = await response.json();
      assert.equal(
        response.status,
        status,
        `${mode} ${path}: ${JSON.stringify(result)}`,
      );
      return result;
    }
    try {
      if (mode === "archive") {
        const response = await fetch(api + "/restores", {
          method: "POST",
          headers: {
            "Content-Type": "application/zip",
            "Idempotency-Key": randomUUID(),
          },
          body: await readFile(
            new URL("../tests/fixtures/bank-schema22.zip", import.meta.url),
          ),
        });
        const stage = await response.json();
        assert.equal(response.status, 201, JSON.stringify(stage));
        assert.equal(stage.data.report.schema_version, 23);
        await call(
          `${root}/restores/${stage.data.id}/activate`,
          { confirm_name: fixture.snapshot.budget.data.name },
          { "Idempotency-Key": randomUUID() },
        );
      }
      const paths = {
        accounts: "/accounts",
        transactions: "/transactions",
        payees: "/payees",
        categories: "/categories",
        settings: `/accounts/${fixture.account_id}/transaction-import-settings?format=csv`,
      };
      for (const [name, suffix] of Object.entries(paths)) {
        const actual = await call(root + suffix),
          expected = structuredClone(fixture.snapshot[name]);
        // Restoring rotates the operational cursor, never financial field clocks.
        if (mode === "archive") {
          delete actual.snapshot;
          delete expected.snapshot;
          if (name === "settings")
            for (const key of ["snapshot", "epoch"]) {
              delete actual.data[key];
              delete expected.data[key];
            }
        }
        assert.deepEqual(
          actual,
          expected,
          `${mode}: original ${name} including field clocks`,
        );
      }
      const checkpoint = await call(root + "/checkpoint");
      if (mode === "database") {
        assert.deepEqual(checkpoint, fixture.snapshot.checkpoint);
        assert.deepEqual(
          await call(root + "/history", undefined, {
            "X-Actual-History-Owner": fixture.owner,
          }),
          fixture.snapshot.history,
        );
      } else
        assert.notEqual(
          checkpoint.data.epoch,
          fixture.snapshot.checkpoint.data.epoch,
        );
      // Original operation results remain exact even after current epoch rotates.
      for (const receipt of fixture.receipts)
        assert.deepEqual(
          await call(
            receipt.path,
            receipt.body,
            { ...receipt.headers, "Idempotency-Key": receipt.key },
            receipt.status ?? 200,
            receipt.method ?? "POST",
          ),
          receipt.response,
        );
      assert.deepEqual(
        await call(root + "/checkpoint"),
        checkpoint,
        "receipt replay cannot stamp new changes",
      );
      if (mode === "database") {
        assert.deepEqual(await call(`/imports/${fixture.foreign_stage.id}`), {
          data: fixture.foreign_stage,
        });
        for (const name of [
          "blocked_actual_stage",
          "blocked_preference_stage",
        ]) {
          const stage = (await call(`/imports/${fixture[name].id}`)).data;
          assert.equal(
            stage.status,
            "ready",
            `${name}: supported SimpleFIN state must become ready`,
          );
          await call(
            `/imports/${stage.id}/activate`,
            {},
            { "Idempotency-Key": randomUUID() },
            201,
          );
          const state = (await call(`/budgets/${stage.budget_id}/banking`))
            .data;
          if (name === "blocked_actual_stage") {
            assert.equal(state.connections.length, 1);
            assert.equal(state.connections[0].state, "credentials_missing");
            assert.equal(state.links.length, 1);
            assert.equal(state.links[0].initial_sync, false);
            assert.equal(state.accounts.length, 1);
            assert.equal(state.accounts[0].balance, "100.00");
            assert.equal(
              state.accounts[0].currency,
              "",
              "export did not retain provider currency",
            );
            const bankRoot = `/budgets/${stage.budget_id}`,
              account = state.links[0].id;
            const actualBefore = (await call(bankRoot + "/transactions")).data;
            const source = linked.snapshot.transactions;
            function projection(rows, source) {
              return rows
                .map((t) => ({
                  id: t.id,
                  account: t[source ? "account" : "account_id"],
                  category: t[source ? "category" : "category_id"] ?? null,
                  payee: t[source ? "payee" : "payee_id"] ?? null,
                  amount: t.amount,
                  date: t.date,
                  notes: t.notes ?? "",
                  cleared: !!t.cleared,
                  reconciled: !!t.reconciled,
                  starting:
                    !!t[source ? "starting_balance_flag" : "starting_balance"],
                  imported_id: t.imported_id ?? null,
                  imported_payee: t.imported_payee ?? null,
                  parent: t.parent_id ?? null,
                  transfer: t.transfer_id ?? null,
                }))
                .sort((a, b) => a.id.localeCompare(b.id));
            }
            assert.deepEqual(
              projection(actualBefore, false),
              projection(source, true),
              "Original linked source IDs/full financial graph",
            );
            const headers = {
              "Idempotency-Key": randomUUID(),
              "X-Actual-Budget-Epoch": state.epoch,
            };
            const commandPath =
              bankRoot +
              `/banking/connections/${state.connections[0].id}/commands`;
            let connection = (
              await call(
                commandPath,
                {
                  action: "replace",
                  generation: state.connections[0].generation,
                  setup_token: Buffer.from(
                    process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim",
                  ).toString("base64"),
                },
                headers,
              )
            ).data;
            connection = (
              await call(
                commandPath,
                { action: "claim", generation: connection.generation },
                { ...headers, "Idempotency-Key": randomUUID() },
              )
            ).data;
            await call(
              commandPath,
              { action: "discover", generation: connection.generation },
              { ...headers, "Idempotency-Key": randomUUID() },
            );
            let fresh = (await call(bankRoot + "/banking")).data;
            const remote = fresh.accounts.find(
              (a) =>
                !a.deleted &&
                a.generation === connection.generation &&
                a.remote_connection_id === "discovered-institution",
            );
            await call(
              bankRoot + `/accounts/${account}/banking/commands`,
              {
                action: "link",
                snapshot: fresh.snapshot,
                connection_id: connection.id,
                generation: connection.generation,
                remote_account_id: remote.id,
                opening_balance_mode: "none",
              },
              { ...headers, "Idempotency-Key": randomUUID() },
            );
            fresh = (await call(bankRoot + "/banking")).data;
            const refresh = {
              action: "refresh",
              generation: connection.generation,
              snapshot: fresh.snapshot,
              start_date: "2026-11-01",
              end_date: "2026-11-10",
              accounts: [
                { account_id: account, revision: fresh.links[0].revision },
              ],
            };
            const refreshHeaders = {
              ...headers,
              "Idempotency-Key": randomUUID(),
              "X-Actual-History-Owner": fixture.owner,
            };
            const result = await call(commandPath, refresh, refreshHeaders);
            assert.deepEqual(
              await call(commandPath, refresh, refreshHeaders),
              result,
            );
            assert.deepEqual(
              projection((await call(bankRoot + "/transactions")).data, false),
              projection(source, true),
              "Explicit legacy relink preserves every source row and prevents duplicate initial opening/import",
            );
          }
        }
      }
      console.log(
        `PASS genuine22 ${mode}: financial/reference clocks, settings, original receipts and bank conversion`,
      );
    } finally {
      await server.stop();
      await rm(directory, { recursive: true, force: true });
    }
  }
} finally {
  await new Promise((resolve) => provider.close(resolve));
}
