// Development oracle: only the published Actual API executes financial behavior.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mock } from 'node:test';

const requireOracle = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = requireOracle('@actual-app/api');
const packagePath = requireOracle.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json');
assert.equal(JSON.parse(await readFile(packagePath, 'utf8')).version, '26.10.0');
const destination = new URL('../tests/fixtures/register.json', import.meta.url);
const dataDir = await mkdtemp(join(tmpdir(), 'actualbudget-register-oracle-'));
const transactionId = index => `10000000-0000-4000-8000-${String(index).padStart(12, '0')}`;
const input = {
  account: { name: 'Spending', opening_balance: 10000, opening_date: '2026-10-15', off_budget: false },
  payee: { name: 'Cafe' },
  category: { name: 'Food', is_income: false },
  transactions: [
    { id: transactionId(1), date: '2025-12-31', amount: 20000, notes: 'income', cleared: true },
    { id: transactionId(2), date: '2026-01-01', amount: -1234, notes: 'expense', cleared: false, payee_name: 'Cafe', category_name: 'Food' },
    { id: transactionId(3), date: '2026-01-02', amount: 234, notes: 'refund', cleared: true, payee_name: 'Cafe', category_name: 'Food' },
    { id: transactionId(4), date: '2024-02-29', amount: -500, notes: 'uncategorized', cleared: false },
    { id: transactionId(5), date: '2027-01-01', amount: -100, notes: 'future', cleared: true },
  ],
  operations: [
    { name: 'created', action: 'create' },
    { name: 'edited', action: 'update', id: transactionId(2), patch: { amount: -1200, date: '2026-01-03', cleared: true, notes: 'edited expense' } },
    { name: 'duplicated', action: 'duplicate', source_id: transactionId(2), id: transactionId(6) },
    { name: 'deleted refund', action: 'delete', id: transactionId(3) },
  ],
};
process.env.TZ = 'UTC';
mock.timers.enable({ apis: ['Date'], now: new Date('2026-10-15T12:00:00Z') });
try {
  await api.init({ dataDir });
  let account;
  await api.runImport('Disposable register oracle', async () => {
    account = await api.createAccount({ name: input.account.name }, input.account.opening_balance);
  });
  const group = await api.createCategoryGroup({ name: 'Everyday' });
  const category = await api.createCategory({ name: input.category.name, group_id: group });
  const payee = await api.createPayee(input.payee);
  const snapshots = [];
  for (const operation of input.operations) {
    switch (operation.action) {
      case 'create':
        await api.addTransactions(account, input.transactions.map(({ payee_name, category_name, ...transaction }) => ({
          ...transaction, ...(payee_name ? { payee } : {}), ...(category_name ? { category } : {}),
        })));
        break;
      case 'update':
        await api.updateTransaction(operation.id, operation.patch);
        break;
      case 'duplicate': {
        const source = (await api.getTransactions(account)).find(row => row.id === operation.source_id);
        assert.ok(source, 'Source transaction has the requested stable identity');
        // Actual UI useTransactionBatchActions.ts duplicates the fields, then clears
        // cleared/reconciled. Public API has no duplicate method: submit that payload.
        await api.addTransactions(account, [{
          id: operation.id, date: source.date, amount: source.amount, notes: source.notes,
          payee: source.payee, category: source.category, cleared: false, reconciled: false,
        }]);
        break;
      }
      case 'delete':
        await api.deleteTransaction(operation.id);
        break;
    }
    const rows = await api.getTransactions(account);
    const payees = await api.getPayees();
    const categories = await api.getCategories();
    const total = await api.aqlQuery(api.q('transactions').filter({ account }).calculate({ $sum: '$amount' }));
    const cleared = await api.aqlQuery(api.q('transactions').filter({ account, cleared: true }).calculate({ $sum: '$amount' }));
    const uncleared = await api.aqlQuery(api.q('transactions').filter({ account, cleared: false }).calculate({ $sum: '$amount' }));
    const dates = [...new Set([...rows.map(row => row.date), '2026-01-01', '2026-10-15'])].sort();
    const through_dates = [];
    for (const date of dates) {
      through_dates.push({ date, balance: await api.getAccountBalance(account, new Date(`${date}T12:00:00Z`)) });
    }
    snapshots.push({
      name: operation.name,
      balances: { total: total.data, cleared: cleared.data, uncleared: uncleared.data },
      current_api_balance: await api.getAccountBalance(account),
      through_dates,
      transactions: rows.map(row => ({
        id: row.starting_balance_flag ? 'opening' : row.id,
        date: row.date, amount: row.amount, notes: row.notes ?? '', cleared: row.cleared,
        reconciled: row.reconciled, starting_balance: row.starting_balance_flag,
        payee_name: payees.find(value => value.id === row.payee)?.name ?? null,
        category_name: categories.find(value => value.id === row.category)?.name ?? null,
      })).sort((left, right) => left.id.localeCompare(right.id, 'en')),
    });
  }
  const rendered = JSON.stringify({
    oracle: '@actual-app/api@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577',
    clock: '2026-10-15T12:00:00Z', timezone: 'UTC',
    description: 'Public API mutations, AQL all-date register totals and getAccountBalance calendar cutoffs. Random opening identity normalized. Duplicate payload follows pinned UI hook; no replacement engine, sleeps, live server or user data.',
    input, snapshots,
  }, null, 2) + '\n';
  if (process.argv.includes('--check')) {
    assert.equal(await readFile(destination, 'utf8'), rendered, 'Pinned register outputs differ from fixture');
    console.log('Register fixture matches pinned Actual 26.10.0');
  } else {
    await writeFile(destination, rendered);
    console.log(`Wrote ${destination.pathname}`);
  }
} finally {
  await api.shutdown();
  mock.timers.reset();
  await rm(dataDir, { recursive: true, force: true });
}
