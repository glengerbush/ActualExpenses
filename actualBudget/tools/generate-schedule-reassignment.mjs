// Independent pinned-source observation: moving a posted payment between schedules.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
process.env.TZ = 'UTC';
const NativeDate = Date;
globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : ['2026-11-09T12:00:00Z'])); }
  static now() { return NativeDate.parse('2026-11-09T12:00:00Z'); }
};
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json'), 'utf8')).version, '26.10.0');
const root = await mkdtemp(`${tmpdir()}/schedule-reassignment-`);
const out = { oracle: '@actual-app/api@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577', clock: '2026-11-09T12:00:00Z', cases: [] };
try {
  const engine = await api.init({ dataDir: root });
  const send = (name, body) => engine.send(name, body);
  for (const transfer of [false, true]) for (const targetAutomatic of [false, true]) {
    const ids = { A: '22220000-0000-4000-8000-000000000901', B: '22220000-0000-4000-8000-000000000902' };
    await api.runImport('Schedule reassignment', async () => {
      ids.ACCOUNT = await api.createAccount({ name: 'Spending' }, 0);
      ids.PEER = await api.createAccount({ name: 'Savings' }, 0);
      ids.PAYEE = await api.createPayee({ name: 'Bill' });
    });
    const payees = await api.getPayees();
    ids.TRANSFER = payees.find(p => p.transfer_acct === ids.PEER).id;
    ids.BACK_TRANSFER = payees.find(p => p.transfer_acct === ids.ACCOUNT).id;
    const normalize = value => JSON.parse(JSON.stringify(value, (_, v) => {
      if (typeof v === 'string' && /^[0-9a-f]{8}-[0-9a-f-]{27}$/.test(v)) {
        const label = Object.keys(ids).find(k => ids[k] === v);
        assert.ok(label, `Unknown original reference ${v}`); return label;
      }
      return v;
    }));
    const create = async (label, automatic) => {
      assert.equal(await send('schedule/create', { schedule: { id: ids[label], name: label, posts_transaction: automatic }, conditions: [
        { field: 'account', op: 'is', value: ids.ACCOUNT },
        { field: 'payee', op: 'is', value: transfer ? ids.TRANSFER : ids.PAYEE },
        { field: 'amount', op: 'is', value: -100 }, { field: 'date', op: 'is', value: '2026-11-09' },
      ] }), ids[label]);
    };
    let generated = 0;
    const snapshot = async step => {
      const rows = [...await api.getTransactions(ids.ACCOUNT), ...await api.getTransactions(ids.PEER)];
      // Match newly generated peers by reciprocal edge, never by arbitrary UUID order.
      for (const row of rows.filter(r => r.account === ids.ACCOUNT)) if (!Object.values(ids).includes(row.id)) {
        const label = `payment-${++generated}`; ids[label] = row.id;
        if (row.transfer_id) ids[`${label}-peer`] = row.transfer_id;
      }
      const ledger = rows.map(r => normalize({ id: r.id, account: r.account, amount: r.amount, date: r.date,
        payee: r.payee ?? null, category: r.category ?? null, notes: r.notes ?? '', cleared: !!r.cleared,
        reconciled: !!r.reconciled, transfer_id: r.transfer_id ?? null, parent_id: r.parent_id ?? null,
        is_parent: !!r.is_parent, schedule: r.schedule ?? null })).sort((a, b) => a.id.localeCompare(b.id));
      const schedules = (await api.aqlQuery(api.q('schedules').select('*'))).data.map(r => normalize({
        id: r.id, next_date: r.next_date, completed: r.completed, posts_transaction: r.posts_transaction,
      })).sort((a, b) => a.id.localeCompare(b.id));
      return { step, ledger, schedules, balances: await Promise.all([api.getAccountBalance(ids.ACCOUNT), api.getAccountBalance(ids.PEER)]) };
    };
    await create('A', true);
    await send('schedule/force-run-service');
    const snapshots = [await snapshot('A posted')];
    assert.equal(generated, 1);
    await create('B', targetAutomatic);
    snapshots.push(await snapshot('B created'));
    await send('transactions-batch-update', { updated: [{ id: ids['payment-1'], schedule: ids.B }] });
    snapshots.push(await snapshot('payment moved to B'));
    await send('schedule/force-run-service'); snapshots.push(await snapshot('service after reassignment'));
    await send('schedule/force-run-service'); snapshots.push(await snapshot('repeat service'));
    out.cases.push({ transfer, target_automatic: targetAutomatic, snapshots });
  }
  const path = new URL('../tests/fixtures/schedule-reassignment.json', import.meta.url);
  if (process.argv.includes('--check')) assert.deepEqual(out, JSON.parse(await readFile(path, 'utf8')));
  else await writeFile(path, `${JSON.stringify(out, null, 2)}\n`, { flag: 'wx' });
  console.log(`PASS ${out.cases.length} independent source reassignment cases`);
} finally {
  await api.shutdown();
  await rm(root, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}
