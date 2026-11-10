// Immutable genuine22 snapshots supply expectations; no database rewriting.
import assert from "node:assert/strict";
import { readFile, mkdtemp, copyFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID, createHash } from "node:crypto";
import { createServer } from "node:http";
import { startServer } from "../web/tests/server.mjs";
const fixture = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-schema22.json", import.meta.url),
  ),
);
for (const extension of [".sqlite3", ".zip"]) {
  const bytes = await readFile(
    new URL(`../tests/fixtures/bank-schema22${extension}`, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    fixture.artifacts[extension].sha256,
    `Original schema22 ${extension} fixture`,
  );
}
const linked = JSON.parse(
  await readFile(
    new URL("../tests/fixtures/bank-linked-source-26.10.json", import.meta.url),
  ),
).cases[0];
let holdNext = false,
  heldRelease,
  heldStarted;
const provider = createServer((req, res) => {
  if (req.method === "POST") {
    res.end(
      `http://synthetic:NOT-A-REAL-SECRET@127.0.0.1:${provider.address().port}/access`,
    );
    return;
  }
  const url = new URL(req.url, "http://fixture.invalid");
  if (holdNext) {
    holdNext = false;
    heldRelease = () => provider.emit("request", req, res);
    heldStarted();
    return;
  }
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
        assert.equal(stage.data.report.schema_version, 24);
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
            // Portable copies carry metadata only. Repeated restore of a clone
            // must not re-create a second source-derived connection identity.
            const copyId = randomUUID(),
              copyRoot = `/budgets/${copyId}`;
            await call(
              bankRoot + "/duplicate",
              { id: copyId, name: "Bank portability copy" },
              { ...headers, "Idempotency-Key": randomUUID() },
              201,
            );
            async function portableState(rootPath) {
              const bank = (await call(rootPath + "/banking")).data;
              assert.equal(
                bank.connections.length,
                1,
                "Exactly one retained source connection",
              );
              assert.equal(bank.connections[0].state, "credentials_missing");
              const rows = (await call(rootPath + "/transactions")).data;
              assert.deepEqual(
                projection(rows, false),
                projection(source, true),
              );
              return bank;
            }
            let copyState = await portableState(copyRoot);
            await call(
              copyRoot + `/accounts/${account}/banking/commands`,
              {
                action: "unlink",
                revision: copyState.links[0].revision,
                snapshot: copyState.snapshot,
              },
              {
                "Idempotency-Key": randomUUID(),
                "X-Actual-Budget-Epoch": copyState.epoch,
              },
            );
            copyState = await portableState(copyRoot);
            assert.equal(
              copyState.links[0].deleted,
              true,
              "Intentional unlink tombstone is durable native state",
            );
            const point = (
              await call(
                copyRoot + "/backups",
                { label: "Current native metadata only" },
                {
                  "Idempotency-Key": randomUUID(),
                  "X-Actual-Budget-Epoch": copyState.epoch,
                },
                201,
              )
            ).data;
            const bytes = Buffer.from(
              await (
                await fetch(api + copyRoot + `/backups/${point.id}/download`)
              ).arrayBuffer(),
            );
            const { spawnSync } = await import("node:child_process");
            const decoded = spawnSync(
              "python3",
              [
                "-c",
                "import io,sys,zipfile; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); sys.stdout.buffer.write(b'\\n'.join(z.read(n) for n in z.namelist()))",
              ],
              { input: bytes, maxBuffer: 8 * 1024 * 1024 },
            );
            assert.equal(decoded.status, 0, decoded.stderr.toString());
            for (const secret of [
              "NOT-A-REAL-SECRET",
              process.env.ACTUALBUDGET_BANK_TEST_ORIGIN,
              Buffer.from(
                process.env.ACTUALBUDGET_BANK_TEST_ORIGIN + "/claim",
              ).toString("base64"),
            ])
              assert.ok(
                !decoded.stdout.includes(secret),
                "Portable copy excludes source credential and endpoints",
              );
            await call(
              copyRoot + `/backups/${point.id}/restore`,
              { confirm_name: "Bank portability copy" },
              {
                "Idempotency-Key": randomUUID(),
                "X-Actual-Budget-Epoch": copyState.epoch,
              },
            );
            const restored = await portableState(copyRoot);
            assert.notEqual(restored.epoch, copyState.epoch);
            assert.notEqual(
              restored.connections[0].generation,
              copyState.connections[0].generation,
            );
            assert.deepEqual(
              restored.links,
              copyState.links,
              "Original mapping and tombstones survive clone restore",
            );
            assert.deepEqual(
              restored.accounts,
              copyState.accounts,
              "No extra source remotes or revived tombstones",
            );
            assert.equal(
              restored.connections[0].id,
              copyState.connections[0].id,
            );
            const secondId = randomUUID(),
              secondRoot = `/budgets/${secondId}`;
            await call(
              copyRoot + "/duplicate",
              { id: secondId, name: "Second bank copy" },
              {
                "Idempotency-Key": randomUUID(),
                "X-Actual-Budget-Epoch": restored.epoch,
              },
              201,
            );
            const second = await portableState(secondRoot);
            assert.equal(second.connections[0].id, copyState.connections[0].id);
            assert.deepEqual(second.links, copyState.links);
            assert.deepEqual(second.accounts, copyState.accounts);
            const original = (await call(bankRoot + "/banking")).data;
            const originalPoint = (
              await call(
                bankRoot + "/backups",
                { label: "Before held provider response" },
                {
                  "Idempotency-Key": randomUUID(),
                  "X-Actual-Budget-Epoch": original.epoch,
                },
                201,
              )
            ).data;
            holdNext = true;
            const started = new Promise((resolve) => (heldStarted = resolve));
            const delayed = fetch(api + commandPath, {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                ...headers,
                "Idempotency-Key": randomUUID(),
              },
              body: JSON.stringify({ ...refresh, snapshot: original.snapshot }),
            });
            let timer;
            try {
              await Promise.race([
                started,
                new Promise(
                  (_, reject) =>
                    (timer = setTimeout(
                      () =>
                        reject(Error("Held provider request did not start")),
                      5000,
                    )),
                ),
              ]);
            } finally {
              clearTimeout(timer);
            }
            await call(
              bankRoot + `/backups/${originalPoint.id}/restore`,
              { confirm_name: (await call(bankRoot)).data.name },
              {
                "Idempotency-Key": randomUUID(),
                "X-Actual-Budget-Epoch": original.epoch,
              },
            );
            const afterRestore = (await call(bankRoot + "/banking")).data;
            assert.equal(
              afterRestore.connections[0].state,
              "credentials_missing",
            );
            assert.notEqual(afterRestore.epoch, original.epoch);
            heldRelease();
            heldRelease = undefined;
            const late = await delayed;
            assert.equal(late.status, 409, await late.text());
            assert.deepEqual(
              (await call(bankRoot + "/banking")).data,
              afterRestore,
              "Late old-epoch HTTP response cannot alter restored metadata/cursor",
            );
            assert.deepEqual(
              projection((await call(bankRoot + "/transactions")).data, false),
              projection(source, true),
            );
            assert.deepEqual(
              await call(commandPath, refresh, refreshHeaders),
              result,
              "Original refresh receipt remains exact after restore",
            );

            // Restoring clears only this budget's private provider custody.
            assert.equal(
              (await call(bankRoot + "/banking")).data.connections[0].state,
              "credentials_missing",
            );
          }
        }
      }
      console.log(
        `PASS genuine22 ${mode}: financial/reference clocks, settings, original receipts and bank conversion${mode === "database" ? "; current native clone/restore/tombstones/secrets/late-response fencing" : ""}`,
      );
    } finally {
      await server.stop();
      await rm(directory, { recursive: true, force: true });
    }
  }
} finally {
  heldRelease?.();
  provider.closeAllConnections();
  await new Promise((resolve) => provider.close(resolve));
}
