// Independent observable oracle; only published Actual APIs mutate disposable data.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json', import.meta.url))).version, '26.10.0');
const fixture = new URL('../tests/fixtures/account-lifecycle.json', import.meta.url);
const root = await mkdtemp(join(tmpdir(), 'actualbudget-account-oracle-'));
const aliases = new Map();
const alias = (id, name) => { aliases.set(id, name); return id; };
const label = id => id == null ? null : aliases.get(id) ?? id;
const snapshots = [];
let initialized = false;
async function snapshot(step) {
  const accounts = [];
  for (const row of await api.getAccounts()) {
    const transactions = (await api.getTransactions(row.id)).map(tx => ({
      id: label(tx.id), amount: tx.amount, date: tx.date,
      cleared: tx.cleared, notes: tx.notes ?? null,
    })).sort((a, b) => a.id.localeCompare(b.id));
    const note = await api.getNote(row.id);
    accounts.push({ id: label(row.id), name: row.name, offbudget: row.offbudget,
      closed: row.closed, group: label(row.account_group_id),
      note: note ? { id: label(note.id), note: note.note } : null, transactions,
      total: (await api.aqlQuery(api.q('transactions').filter({ account: row.id }).calculate({ $sum: '$amount' }))).data ?? 0,
    });
  }
  accounts.sort((a, b) => a.id.localeCompare(b.id));
  const groups = (await api.getAccountGroups()).map(g => ({ id: label(g.id), name: g.name })).sort((a, b) => a.id.localeCompare(b.id));
  snapshots.push({ step, accounts, groups });
}
try {
  await api.init({ dataDir: root }); initialized = true;
  let spending, reserve, empty, doomed, group;
  await api.runImport('Disposable account lifecycle oracle', async () => {
    spending = alias(await api.createAccount({ name: 'Spending', offbudget: false }, 0), 'spending');
    reserve = alias(await api.createAccount({ name: 'Reserve', offbudget: true }, 0), 'reserve');
    empty = alias(await api.createAccount({ name: 'Empty', offbudget: false }, 0), 'empty');
    doomed = alias(await api.createAccount({ name: 'Delete with history', offbudget: false }, 0), 'doomed');
    group = alias(await api.createAccountGroup({ name: 'Daily accounts' }), 'daily');
    const transaction = (n, amount) => ({ id: alias(`09000000-0000-4000-8000-${String(n).padStart(12, '0')}`, `transaction-${n}`), date: '2026-01-01', amount, cleared: true });
    await api.addTransactions(spending, [transaction(1, 10000), transaction(2, -1250)]);
    await api.addTransactions(reserve, [transaction(3, -5000)]);
    await api.addTransactions(doomed, [transaction(4, -321)]);
  });
  await snapshot('initial');
  await api.updateAccount(spending, { name: 'Daily spending', account_group_id: group });
  await api.updateAccount(reserve, { account_group_id: group });
  await api.updateNote(spending, 'Account note with #travel and two lines\nRetain history');
  await api.updateAccountGroup(group, { name: 'Everyday' });
  await snapshot('renamed-grouped-and-annotated');
  let closeError = null;
  try { await api.closeAccount(spending); } catch (error) { closeError = String(error.message); }
  assert.ok(closeError?.includes('balance is non-zero'), closeError);
  await snapshot('nonzero-close-rejected');
  await api.addTransactions(spending, [{ id: alias('09000000-0000-4000-8000-000000000005', 'transaction-5'), date: '2026-01-02', amount: -8750, cleared: false }]);
  await api.closeAccount(spending);
  await snapshot('zero-balance-with-history-closed');
  await api.reopenAccount(spending);
  await snapshot('reopened-with-history');
  await api.closeAccount(empty);
  assert.deepEqual(await api.getTransactions(empty), []);
  await snapshot('empty-close-deletes-account');
  await api.deleteAccount(doomed);
  assert.deepEqual(await api.getTransactions(doomed), []);
  await snapshot('forced-delete-removes-account-history');
  await api.deleteAccountGroup(group);
  await snapshot('group-deletion-ungroups-accounts');
  const result = { oracle: '@actual-app/api@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577',
    normalization: 'Runtime-generated account/group IDs use stable aliases. Fixed transaction IDs use aliases. No financial values, dates, visibility or references are rewritten.',
    close_error: closeError, snapshots };
  if (process.argv.includes('--check')) assert.deepEqual(result, JSON.parse(await readFile(fixture, 'utf8')));
  else await writeFile(fixture, JSON.stringify(result, null, 2) + '\n');
  console.log(`PASS pinned Actual account lifecycle: ${snapshots.length} public snapshots`);
} finally {
  if (initialized) await api.shutdown();
  await rm(root, { recursive: true, force: true });
}
