// Genuine, disposable Actual 26.10.0 public-API export; no SQLite fabrication.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json', import.meta.url))).version, '26.10.0');
const directory = await mkdtemp(join(tmpdir(), 'actual-payee-names-'));
try {
  await api.init({ dataDir: directory });
  let account;
  const payees = [];
  await api.runImport('Source payee names', async () => {
    account = await api.createAccount({ name: 'Spending' }, 0);
    for (const [index, name] of ['', '   ', '\t', 'Coffee\nShop', 'Merchant '.repeat(20), '店'.repeat(121)].entries()) {
      const id = await api.createPayee({ name });
      payees.push({ id, name });
      await api.addTransactions(account, [{ date: '2026-01-02', amount: -(index + 1) * 100, payee: id, notes: `Source payee ${index}` }]);
    }
  });
  const visible = await api.getPayees();
  for (const payee of payees) assert.equal(visible.find(p => p.id === payee.id)?.name, payee.name);
  const transactions = await api.getTransactions(account);
  const deletedPayee = await api.createPayee({ name: 'Removed source payee' });
  await api.updatePayee(deletedPayee, { name: '' });
  await api.deletePayee(deletedPayee);
  await writeFile(new URL('../tests/fixtures/import-payee-names.zip', import.meta.url), await api.exportBudget());
  await writeFile(new URL('../tests/fixtures/import-payee-names.json', import.meta.url), JSON.stringify({
    oracle: '@actual-app/api@26.10.0', account, payees, deletedPayee, transactions,
    balance: transactions.reduce((sum, row) => sum + row.amount, 0),
  }, null, 2) + '\n');
  console.log('PASS Actual accepts and exports all six payee-name cases unchanged');
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}
