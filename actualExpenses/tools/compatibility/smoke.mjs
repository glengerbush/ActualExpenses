import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const fixturePath = process.env.ACTUAL_FIXTURE || path.join(path.dirname(fileURLToPath(import.meta.url)), '.fixture/fixture.json');
const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
async function oracle(route, method = 'GET', body) {
  const response = await fetch(fixture.oracleUrl + route, {
    method, headers: { 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  assert.equal(response.status, 200, JSON.stringify(value));
  return value;
}
const initial = await oracle('/state');
assert.equal(initial.accounts.find(x => x.id === fixture.accountId).name, 'Spending');
assert.equal(initial.categories.find(x => x.id === fixture.categoryId).name, 'Groceries');
assert.equal(initial.payees.find(x => x.id === fixture.payeeId).name, 'Fixture shop');
assert(initial.rules.some(rule => rule.conditions.some(c => c.field === 'payee' && c.value === fixture.payeeId) && rule.actions.some(a => a.field === 'category' && a.value === fixture.categoryId)));
const { ids } = await oracle('/transactions', 'POST', { transactions: [{ date: '2026-09-01', amount: -1001, payee: fixture.payeeId, category: fixture.categoryId, cleared: true, notes: 'Harness smoke' }] });
assert.equal(ids.length, 1);
let state = await oracle('/state');
let transaction = state.transactions.find(x => x.id === ids[0]);
assert.equal(transaction.amount, -1001);
assert.equal(transaction.date, '2026-09-01');
assert.equal(transaction.cleared, true);
assert.equal(transaction.payee, fixture.payeeId);
assert.equal(transaction.category, fixture.categoryId);
await oracle(`/transactions/${ids[0]}`, 'PATCH', { amount: -1002, notes: 'Harness smoke edited' });
state = await oracle('/state');
transaction = state.transactions.find(x => x.id === ids[0]);
assert.equal(transaction.amount, -1002);
assert.equal(transaction.notes, 'Harness smoke edited');
await oracle(`/transactions/${ids[0]}`, 'DELETE');
state = await oracle('/state');
assert.equal(state.transactions.some(x => x.id === ids[0]), false);
console.log('PASS independent Actual API fixture: setup, seeded association, exact money, cleared create, edit, delete.');
