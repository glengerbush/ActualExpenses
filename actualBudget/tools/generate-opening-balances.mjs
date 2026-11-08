// Development oracle only: the production application never loads Actual's engine.
// Install the pinned harness first: npm ci --prefix actualExpenses/tools/compatibility
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mock } from 'node:test';

const requireOracle = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = requireOracle('@actual-app/api');
const packagePath = requireOracle.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json');
assert.equal(JSON.parse(await readFile(packagePath, 'utf8')).version, '26.10.0');
const destination = new URL('../tests/fixtures/opening-balances.json', import.meta.url);
const dataDir = await mkdtemp(join(tmpdir(), 'actualbudget-opening-oracle-'));
const cases = [
  { name: 'Empty spending', off_budget: false, opening_balance: 0 },
  { name: 'Cash', off_budget: false, opening_balance: 12345 },
  { name: 'Credit card', off_budget: false, opening_balance: -6789 },
  { name: 'Asset', off_budget: true, opening_balance: 234567 },
  { name: 'Loan', off_budget: true, opening_balance: -345678 },
  { name: 'One cent', off_budget: false, opening_balance: 1 },
  { name: 'Negative cent', off_budget: true, opening_balance: -1 },
];

// Freeze only the clock; ordinary async timers continue to run.
process.env.TZ = 'UTC';
mock.timers.enable({ apis: ['Date'], now: new Date('2026-10-15T12:00:00Z') });
try {
  await api.init({ dataDir });
  const ids = [];
  await api.runImport('Disposable opening balance oracle', async () => {
    for (const input of cases) {
      ids.push(await api.createAccount({ name: input.name, offbudget: input.off_budget }, input.opening_balance));
    }
  });
  const payees = await api.getPayees();
  const categories = await api.getCategories();
  const results = [];
  for (let index = 0; index < cases.length; index++) {
    const entries = await api.getTransactions(ids[index]);
    results.push({
      input: { ...cases[index], opening_date: '2026-10-15' },
      expected: {
        balance: await api.getAccountBalance(ids[index]),
        transactions: entries.map(entry => ({
          amount: entry.amount,
          date: entry.date,
          cleared: entry.cleared,
          reconciled: entry.reconciled,
          starting_balance: entry.starting_balance_flag,
          payee_name: payees.find(payee => payee.id === entry.payee)?.name ?? null,
          category_name: categories.find(category => category.id === entry.category)?.name ?? null,
          category_is_income: categories.find(category => category.id === entry.category)?.is_income ?? null,
        })),
      },
    });
  }
  const fixture = {
    oracle: '@actual-app/api@26.10.0',
    source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577',
    clock: '2026-10-15T12:00:00Z',
    timezone: 'UTC',
    description: 'Public createAccount/getAccountBalance/getTransactions outputs; randomized identities normalized to names. No live server or user data.',
    cases: results,
  };
  const rendered = `${JSON.stringify(fixture, null, 2)}\n`;
  if (process.argv.includes('--check')) {
    assert.equal(await readFile(destination, 'utf8'), rendered, 'Pinned Actual outputs differ from recorded fixture');
    console.log('Opening-balance fixture matches pinned Actual 26.10.0');
  } else {
    await mkdir(new URL('../tests/fixtures/', import.meta.url), { recursive: true });
    await writeFile(destination, rendered);
    console.log(`Wrote ${destination.pathname}`);
  }
} finally {
  await api.shutdown();
  mock.timers.reset();
  await rm(dataDir, { recursive: true, force: true });
}
