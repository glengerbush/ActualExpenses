// Independent Actual26.10.0 archive and ledger for catch-up exceeding the500 limit.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';

process.env.TZ = 'UTC';
const NativeDate = Date;
let asOf = '2025-01-01T12:00:00Z';
globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [NativeDate.parse(asOf)])); }
  static now() { return NativeDate.parse(asOf); }
};
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
const version = JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json'), 'utf8')).version;
assert.equal(version, '26.10.0');
const root = await mkdtemp(`${tmpdir()}/schedules-overdue-oracle-`);
const zipPath = new URL('../tests/fixtures/schedules-overdue-26.10.zip', import.meta.url);
const jsonPath = new URL('../tests/fixtures/schedules-overdue-26.10.json', import.meta.url);
const check = process.argv.includes('--check');
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const scheduleId = '21220000-0000-4000-8000-000000000678';

try {
  const engine = await api.init({ dataDir: root });
  let account;
  await api.runImport('Overdue daily schedule', async () => {
    account = await api.createAccount({ name: 'Spending' }, 0);
  });
  const config = { start: '2025-01-01', frequency: 'daily', endMode: 'never' };
  const conditions = [
    { field: 'account', op: 'is', value: account },
    { field: 'amount', op: 'is', value: -100 },
    { field: 'date', op: 'is', value: config },
  ];
  assert.equal(await engine.send('schedule/create', {
    schedule: { id: scheduleId, name: 'Daily expense', posts_transaction: true },
    conditions,
  }), scheduleId);
  const readRows = async () => (await api.aqlQuery(
    api.q('transactions').options({ splits: 'all' }).select('*').orderBy({ date: 'asc' }),
  )).data;
  const before = await readRows();
  assert.deepEqual(before, [], 'The exported archive must contain no posted transactions');
  const initial = (await api.aqlQuery(api.q('schedules').select('*'))).data;
  assert.equal(initial.length, 1);
  assert.equal(initial[0].next_date, '2025-01-01');
  assert.equal(initial[0].posts_transaction, true);
  const archive = await api.exportBudget();
  asOf = '2026-11-09T12:00:00Z';
  await engine.send('schedule/force-run-service');
  const rows = await readRows();
  assert.ok(rows.length > 500, `Expected more than500 overdue entries, got${rows.length}`);
  const transactions = rows.map(row => {
    assert.equal(row.account, account);
    assert.equal(row.schedule, scheduleId);
    assert.equal(row.amount, -100);
    assert.equal(row.cleared, false);
    assert.equal(row.payee, null);
    assert.equal(row.category, null);
    assert.equal(row.transfer_id, null);
    assert.equal(row.is_parent, false);
    assert.equal(row.parent_id, null);
    return { date: row.date, amount: row.amount, cleared: row.cleared, schedule_id: row.schedule };
  });
  const after = (await api.aqlQuery(api.q('schedules').select('*'))).data;
  assert.equal(after.length, 1);
  const result = {
    oracle: '@actual-app/api@26.10.0',
    source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577',
    created_as_of: '2025-01-01', through: '2026-11-09',
    schedule_id: scheduleId, config,
    before: { next_date: initial[0].next_date, transaction_count: before.length },
    after: { next_date: after[0].next_date, completed: after[0].completed,
      transaction_count: rows.length, total: rows.reduce((sum, row) => sum + row.amount, 0) },
    // Recovery cutoff comes from the settled independent source ledger, not replacement recurrence code.
    first_chunk: { through: transactions[399].date, transaction_count: 400, total: -40000 },
    transactions,
  };
  if (check) {
    const expected = JSON.parse(await readFile(jsonPath, 'utf8'));
    const { archive_sha256, ...observed } = expected;
    assert.equal(digest(await readFile(zipPath)), archive_sha256);
    assert.deepEqual(result, observed);
  } else {
    await writeFile(zipPath, archive, { flag: 'wx' });
    await writeFile(jsonPath, `${JSON.stringify({ archive_sha256: digest(archive), ...result }, null, 2)}\n`, { flag: 'wx' });
  }
  console.log(`PASS authentic empty archive; ${rows.length} pinned overdue postings, total${result.after.total}; first400 through${result.first_chunk.through}${check ? '; independent check' : ''}`);
} finally {
  await api.shutdown();
  await rm(root, { recursive: true, force: true });
}
