// Exact pinned desktop query/mutation seams, followed by persisted source reads.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdir, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';

process.env.TZ = 'UTC';
const NativeDate = Date;
let clock = '2026-11-09T12:00:00Z';
globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [clock])); }
  static now() { return NativeDate.parse(clock); }
};
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json'), 'utf8')).version, '26.10.0');
const root = await mkdtemp(`${tmpdir()}/schedule-matching-oracle-`);
const out = { oracle: '@actual-app/api@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577', clock, matching: [], lifecycle: [], discovery: [], conversion: [], warnings: [] };
const discoveryArchives = [];
const discoveryDirectory = new URL('../tests/fixtures/schedule-discovery/', import.meta.url);
const conversionArchives = [];
const conversionDirectory = new URL('../tests/fixtures/schedule-conversion/', import.meta.url);
const fixed = n => `22220000-0000-4000-8000-${String(n).padStart(12, '0')}`;
let engine;
let serial = 0;
const send = (name, body) => engine.send(name, body);

async function budget(name) {
  const ids = {};
  await api.runImport(name, async () => {
    ids.ACCOUNT = await api.createAccount({ name: 'Spending' }, 0);
    ids.PEER = await api.createAccount({ name: 'Savings' }, 0);
    ids.PAYEE = await api.createPayee({ name: 'Bill' });
    ids.OTHER_PAYEE = await api.createPayee({ name: 'Other' });
    const group = await api.createCategoryGroup({ name: 'Expenses', is_income: false });
    ids.CATEGORY = await api.createCategory({ name: 'Utilities', group_id: group });
  });
  const payees = await api.getPayees();
  ids.TRANSFER = payees.find(p => p.transfer_acct === ids.PEER).id;
  ids.BACK_TRANSFER = payees.find(p => p.transfer_acct === ids.ACCOUNT).id;
  const labels = new Map(Object.entries(ids).map(([label, id]) => [id, label]));
  let generated = 0;
  const id = label => {
    if (!ids[label]) { ids[label] = fixed(++serial); labels.set(ids[label], label); }
    return ids[label];
  };
  const resolve = value => JSON.parse(JSON.stringify(value, (_, v) => typeof v === 'string' && ids[v] ? ids[v] : v));
  const normalize = value => JSON.parse(JSON.stringify(value, (_, v) => {
    if (typeof v === 'string' && /^[0-9a-f]{8}-[0-9a-f-]{27}$/.test(v)) {
      assert.ok(labels.has(v), `Unmapped reference ${v}`);
      return labels.get(v);
    }
    return v;
  }));
  const snapshot = async step => {
    const lists = await Promise.all([api.getTransactions(ids.ACCOUNT), api.getTransactions(ids.PEER)]);
    const groups = [];
    for (const list of lists) {
      list.sort((a, b) => a.date.localeCompare(b.date) || a.amount - b.amount || String(a.notes ?? '').localeCompare(String(b.notes ?? '')) || (labels.get(a.id) ?? '').localeCompare(labels.get(b.id) ?? ''));
      for (const parent of list) {
        groups.push([parent, ...(parent.subtransactions ?? [])]);
      }
    }
    for (const row of groups.flat()) if (!labels.has(row.id)) labels.set(row.id, `generated-${++generated}`);
    // Canonicalize only unrelated parent UUID ties; preserve child allocation order and every reference.
    groups.sort(([a], [b]) => labels.get(a.account).localeCompare(labels.get(b.account)) || a.date.localeCompare(b.date) || a.amount - b.amount || String(a.notes ?? '').localeCompare(String(b.notes ?? '')) || labels.get(a.id).localeCompare(labels.get(b.id)));
    const rows = groups.flat().map(row => normalize({ id: row.id, account: row.account, date: row.date, amount: row.amount,
      payee: row.payee ?? null, category: row.category ?? null, notes: row.notes ?? '',
      cleared: !!row.cleared, reconciled: !!row.reconciled, is_parent: !!row.is_parent,
      parent_id: row.parent_id ?? null, transfer_id: row.transfer_id ?? null, schedule: row.schedule ?? null }));
    const schedules = (await api.aqlQuery(api.q('schedules').select('*'))).data.map(row => normalize({
      id: row.id, next_date: row.next_date, completed: row.completed, posts_transaction: row.posts_transaction,
    })).sort((a, b) => a.id.localeCompare(b.id));
    return { step, rows, schedules, balances: await Promise.all([api.getAccountBalance(ids.ACCOUNT), api.getAccountBalance(ids.PEER)]) };
  };
  const create = async (label, automatic, date = '2026-11-09', amount = -100, payee = ids.PAYEE) => {
    const conditions = [{ field: 'account', op: 'is', value: ids.ACCOUNT }, { field: 'payee', op: 'is', value: payee },
      { field: 'amount', op: 'is', value: amount }, { field: 'date', op: 'is', value: date }];
    assert.equal(await send('schedule/create', { schedule: { id: id(label), name: label, posts_transaction: automatic }, conditions }), ids[label]);
    return conditions;
  };
  return { ids, id, labels, resolve, normalize, snapshot, create };
}

async function matchingCases() {
  const b = await budget('Schedule matching boundaries');
  const { ids, id, normalize } = b;
  const input = [];
  for (const [label, amount, date] of [
    ['exact', -10000, '2026-11-09'], ['amount-low-edge', -10750, '2026-11-09'],
    ['amount-low-outside', -10751, '2026-11-09'], ['amount-high-edge', -9250, '2026-11-09'],
    ['amount-high-outside', -9249, '2026-11-09'], ['date-low-edge', -10000, '2026-11-07'],
    ['date-low-outside', -10000, '2026-11-06'], ['date-high-edge', -10000, '2026-11-11'],
    ['date-high-outside', -10000, '2026-11-12'], ['income', 10000, '2026-11-09'],
    ['zero', 0, '2026-11-09'], ['older-recurrence', -10000, '2026-11-02'],
  ]) input.push({ id: id(label), date, amount, payee: ids.PAYEE, notes: label });
  input.push({ id: id('wrong-payee'), date: '2026-11-09', amount: -10000, payee: ids.OTHER_PAYEE });
  input.push({ id: id('reconciled'), date: '2026-11-09', amount: -10000, payee: ids.PAYEE, cleared: true, reconciled: true });
  input.push({ id: id('transfer'), date: '2026-11-09', amount: -10000, payee: ids.TRANSFER, notes: 'Transfer' });
  input.push({ id: id('split'), date: '2026-11-09', amount: -10000, payee: ids.PAYEE, notes: 'Split',
    subtransactions: [{ id: id('split-child'), amount: -6000, payee: ids.PAYEE }, { id: id('split-transfer'), amount: -4000, payee: ids.TRANSFER }] });
  await api.addTransactions(ids.ACCOUNT, input, { runTransfers: true });
  await b.create('OTHER_SCHEDULE', false);
  await send('transactions-batch-update', { updated: [{ id: ids.exact, schedule: ids.OTHER_SCHEDULE }] });
  const seed = await b.snapshot('seeded');
  const archivePath = new URL('../tests/fixtures/schedule-matching-26.10.zip', import.meta.url);
  const existing = await readFile(archivePath).catch(error => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (existing) {
    const prior = JSON.parse(await readFile(new URL('../tests/fixtures/schedule-matching.json', import.meta.url), 'utf8'));
    assert.equal(createHash('sha256').update(existing).digest('hex'), prior.matching_archive_sha256);
    assert.deepEqual(seed, prior.matching_seed, 'Retained archive must keep the same normalized source seed');
  } else {
    assert.ok(!process.argv.includes('--check'), 'Missing committed source archive');
    await writeFile(archivePath, await api.exportBudget(), { flag: 'wx' });
  }
  out.matching_archive_sha256 = createHash('sha256').update(await readFile(archivePath)).digest('hex');
  for (const [name, amountOp, amount, dateOp, date, withPayee = true, account = ids.ACCOUNT] of [
    ['exact money exact date', 'is', -10000, 'is', '2026-11-09'],
    ['approx money and date boundaries', 'isapprox', -10000, 'isapprox', '2026-11-09'],
    ['inclusive range', 'isbetween', { num1: -10750, num2: -9250 }, 'isapprox', '2026-11-09'],
    ['reversed range', 'isbetween', { num1: -9250, num2: -10750 }, 'is', '2026-11-09'],
    ['weekly approximate date', 'is', -10000, 'isapprox', { start: '2026-11-02', frequency: 'weekly', endMode: 'never' }],
    ['positive amounts', 'isapprox', 10000, 'is', '2026-11-09'],
    ['zero amount', 'isapprox', 0, 'is', '2026-11-09'],
    ['parent and outgoing transfer', 'is', -10000, 'is', '2026-11-09', false],
    ['ordinary split child', 'is', -6000, 'is', '2026-11-09', false],
    ['transfer split child', 'is', -4000, 'is', '2026-11-09', false],
    ['reciprocal transfer leg', 'is', 10000, 'is', '2026-11-09', false, ids.PEER],
    ['reciprocal split transfer leg', 'is', 4000, 'is', '2026-11-09', false, ids.PEER],
  ]) {
    const conditions = [{ field: 'account', op: 'is', value: account }, ...(withPayee ? [{ field: 'payee', op: 'is', value: ids.PAYEE }] : []),
      { field: 'amount', op: amountOp, value: amount }, { field: 'date', op: dateOp, value: date }];
    // Exact useScheduleEdit matched-mode seam: filters plus splits:'all'.
    const { filters, errors } = await send('make-filters-from-conditions', { conditions });
    assert.deepEqual(errors, []);
    const rows = (await api.aqlQuery(api.q('transactions').filter({ $and: filters }).select('*').options({ splits: 'all' }))).data;
    out.matching.push({ name, conditions: normalize(conditions), matched_ids: normalize(rows.map(row => row.id)).sort() });
  }
  out.matching_seed = seed;
}

async function lifecycleCases() {
  for (const scenario of ['link then unlink before auto-post', 'auto-post then link and unlink', 'unlink the sole auto-posted row', 'link transfer outgoing', 'link transfer incoming', 'link split parent', 'link split transfer child']) {
    const b = await budget(scenario);
    const { ids, id } = b;
    const split = scenario.includes('split');
    const transfer = scenario.includes('transfer');
    const original = { id: id('ORIGINAL'), date: '2026-11-09', amount: -100, payee: transfer && !split ? ids.TRANSFER : ids.PAYEE, notes: 'Original purchase',
      ...(split ? { subtransactions: [{ id: id('CHILD'), amount: -40, payee: transfer ? ids.TRANSFER : ids.PAYEE }, { id: id('REMAINDER'), amount: -60, payee: ids.PAYEE }] } : {}) };
    if (scenario !== 'unlink the sole auto-posted row') await api.addTransactions(ids.ACCOUNT, [original], { runTransfers: true });
    await b.create('SCHEDULE', true);
    const snapshots = [await b.snapshot('created')];
    let linked = ids.ORIGINAL;
    if (scenario === 'link transfer incoming') linked = (await api.getTransactions(ids.PEER))[0].id;
    if (scenario === 'link split transfer child') linked = ids.CHILD;
    if (scenario.startsWith('auto-post') || scenario === 'unlink the sole auto-posted row') {
      await send('schedule/force-run-service'); snapshots.push(await b.snapshot('posted first'));
    }
    if (scenario === 'unlink the sole auto-posted row') linked = (await api.getTransactions(ids.ACCOUNT))[0].id;
    // Exact desktop ScheduleLink/ScheduleEditModal mutation seam.
    await send('transactions-batch-update', { updated: [{ id: linked, schedule: ids.SCHEDULE }] });
    snapshots.push(await b.snapshot('linked'));
    await send('schedule/force-run-service'); snapshots.push(await b.snapshot('service while linked'));
    await send('transactions-batch-update', { updated: [{ id: linked, schedule: null }] });
    snapshots.push(await b.snapshot('unlinked'));
    await send('schedule/force-run-service'); snapshots.push(await b.snapshot('service after unlink'));
    await send('schedule/force-run-service'); snapshots.push(await b.snapshot('repeat service'));
    out.lifecycle.push({ name: scenario, linked_id: b.normalize(linked), snapshots });
  }
}

async function discoveryCases() {
  const cases = [
    { name: 'weekly exact', dates: ['2026-10-19', '2026-10-26', '2026-11-02'] },
    { name: 'weekly four entries', dates: ['2026-10-12', '2026-10-19', '2026-10-26', '2026-11-02'] },
    { name: 'biweekly exact', dates: ['2026-09-28', '2026-10-12', '2026-10-26'] },
    { name: 'biweekly four entries', dates: ['2026-09-14', '2026-09-28', '2026-10-12', '2026-10-26'] },
    { name: 'monthly day15', dates: ['2026-08-15', '2026-09-15', '2026-10-15'] },
    { name: 'monthly last day', dates: ['2026-08-31', '2026-09-30', '2026-10-31'] },
    { name: 'monthly last day four entries', dates: ['2026-07-31', '2026-08-31', '2026-09-30', '2026-10-31'] },
    { name: 'first and third Mondays', dates: ['2026-08-03', '2026-08-17', '2026-09-07', '2026-09-21', '2026-10-05', '2026-10-19', '2026-11-02'] },
    { name: 'second and fourth Mondays', dates: ['2026-03-09', '2026-03-23', '2026-04-13', '2026-04-27', '2026-05-11', '2026-05-25', '2026-06-08', '2026-06-22', '2026-07-13', '2026-07-27', '2026-08-10', '2026-08-24', '2026-09-14', '2026-09-28', '2026-10-12', '2026-10-26'] },
    { name: 'variable weekly', dates: ['2026-10-19', '2026-10-27', '2026-11-02'], amounts: [-10000, -10100, -9950] },
    { name: 'insufficient two entries', dates: ['2026-10-26', '2026-11-02'] },
    { name: 'already linked excluded', dates: ['2026-10-19', '2026-10-26', '2026-11-02'], linked: true },
    { name: 'closed account excluded', dates: ['2026-10-19', '2026-10-26', '2026-11-02'], closed: true },
    { name: 'transfers excluded', dates: ['2026-10-19', '2026-10-26', '2026-11-02'], transfer: true },
    { name: 'split history', dates: ['2026-10-19', '2026-10-26', '2026-11-02'], split: true },
    { name: 'income history', dates: ['2026-10-19', '2026-10-26', '2026-11-02'], amounts: [10000, 10000, 10000] },
  ];
  for (const c of cases) {
    const b = await budget(c.name);
    const rows = c.dates.map((date, i) => ({ id: b.id(`HISTORY${i}`), date, amount: c.amounts?.[i] ?? -10000,
      payee: c.transfer ? b.ids.TRANSFER : b.ids.PAYEE, notes: `History${i}`,
      ...(c.split ? { subtransactions: [{ id: b.id(`ALLOCATION${i}`), amount: -10000, payee: b.ids.PAYEE, category: b.ids.CATEGORY }] } : {}) }));
    await api.addTransactions(b.ids.ACCOUNT, rows, { runTransfers: true });
    if (c.linked) {
      await b.create('EXISTING_SCHEDULE', false);
      await send('transactions-batch-update', { updated: rows.map(row => ({ id: row.id, schedule: b.ids.EXISTING_SCHEDULE })) });
    }
    if (c.closed) await send('account-close', { id: b.ids.ACCOUNT, forced: true });
    const seed = await b.snapshot('history');
    if (process.argv.includes('--export-discovery-fixtures')) {
      const filename = `${c.name.replaceAll(' ', '-')}.zip`;
      const archive = await api.exportBudget();
      await writeFile(new URL(filename, discoveryDirectory), archive, { flag: 'wx' });
      discoveryArchives.push({ name: c.name, filename, sha256: createHash('sha256').update(archive).digest('hex'),
        ids: Object.fromEntries([...b.labels].map(([id, label]) => [label, id])) });
    }
    const found = await send('schedule/discover');
    const suggestions = found.map(({ id, ...suggestion }) => b.normalize(suggestion));
    suggestions.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
    out.discovery.push({ name: c.name, seed, suggestions });
  }
}

async function sourceConversionHelpers() {
  const ts = createRequire(new URL('../web/package.json', import.meta.url))('typescript');
  const source = new URL('../reference/actual-26.10.0/packages/loot-core/src/', import.meta.url);
  const loaded = new Map();
  const compile = (text, dependencies) => {
    const exports = {};
    new Function('require', 'exports', ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText)(name => dependencies.has(name) ? dependencies.get(name) : require(name), exports);
    return exports;
  };
  const load = async name => {
    if (loaded.has(name)) return loaded.get(name);
    const text = await readFile(new URL(`${name}.ts`, source), 'utf8');
    const dependencies = new Map();
    for (const [, imported] of text.matchAll(/from ['"](#shared\/[^'"]+)['"]/g)) dependencies.set(imported, await load(imported.slice(1)));
    const exports = compile(text, dependencies); loaded.set(name, exports); return exports;
  };
  const months = await load('shared/months');
  const text = await readFile(new URL('shared/schedules.ts', source), 'utf8');
  const ast = ts.createSourceFile('schedules.ts', text, ts.ScriptTarget.Latest, true);
  const declaration = ast.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === 'getUpcomingDays');
  const constant = ast.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === 'DEFAULT_UPCOMING_SCHEDULE_DAYS'));
  assert.ok(declaration && constant);
  const horizons = {};
  new Function('monthUtils', 'exports', ts.transpileModule(`${constant.getText(ast)}\n${declaration.getText(ast)}`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText)(months, horizons);
  const helperText = await readFile(new URL('../reference/actual-26.10.0/packages/desktop-client/src/util/schedule-actions.ts', import.meta.url), 'utf8');
  out.conversion_helper_sha256 = createHash('sha256').update(helperText).digest('hex');
  // Only transport/import wiring is supplied; the entire pinned helper executes unchanged.
  return compile(helperText, new Map([
    ['@actual-app/core/platform/client/connection', { send }],
    ['@actual-app/core/shared/months', months],
    ['@actual-app/core/shared/query', { q: api.q }],
    ['@actual-app/core/shared/schedules', horizons],
  ]));
}

async function conversionCases() {
  const helpers = await sourceConversionHelpers();
  for (const date of ['2026-11-08', '2026-11-09', '2026-11-16', '2026-11-17', '2026-12-01']) {
    for (const horizon of ['7', 'currentMonth', 'oneMonth', '2-week']) {
      out.warnings.push({ date, horizon, future: helpers.isFutureTransaction({ date }), ...helpers.calculateFutureTransactionInfo({ date }, horizon) });
    }
  }
  for (const c of [
    { name: 'new future draft', persisted: false },
    { name: 'existing future expense', persisted: true },
    { name: 'existing future split', persisted: true, split: true },
    { name: 'existing future transfer', persisted: true, transfer: true },
    { name: 'existing future split transfer', persisted: true, split: true, transfer: true },
  ]) {
    clock = '2026-11-09T12:00:00Z';
    const b = await budget(c.name);
    let transaction = { id: b.id('ORIGINAL'), account: b.ids.ACCOUNT, date: '2026-11-17', amount: -101,
      payee: c.transfer && !c.split ? b.ids.TRANSFER : b.ids.PAYEE, category: c.split ? null : b.ids.CATEGORY, notes: 'Future parent note',
      ...(c.split ? { is_parent: true, subtransactions: [
        { id: b.id('FIRST'), amount: -40, payee: c.transfer ? b.ids.TRANSFER : b.ids.PAYEE, category: c.transfer ? null : b.ids.CATEGORY, notes: 'First allocation' },
        { id: b.id('SECOND'), amount: -61, payee: b.ids.PAYEE, category: b.ids.CATEGORY, notes: 'Second allocation' },
      ] } : {}) };
    if (c.persisted) {
      await api.addTransactions(b.ids.ACCOUNT, [transaction], { runTransfers: true });
      transaction = (await api.getTransactions(b.ids.ACCOUNT)).find(row => row.id === b.ids.ORIGINAL);
      assert.ok(transaction);
    }
    const snapshots = [await b.snapshot('before conversion')];
    const input = b.normalize(transaction);
    if (process.argv.includes('--export-conversion-fixtures')) {
      const filename = `${c.name.replaceAll(' ', '-')}.zip`;
      const archive = await api.exportBudget();
      await writeFile(new URL(filename, conversionDirectory), archive, { flag: 'wx' });
      conversionArchives.push({ name: c.name, filename, sha256: createHash('sha256').update(archive).digest('hex'),
        ids: Object.fromEntries([...b.labels].map(([id, label]) => [label, id])) });
    }
    const schedule = await helpers.createSingleTimeScheduleFromTransaction(transaction);
    b.ids.SCHEDULE = schedule; b.labels.set(schedule, 'SCHEDULE');
    const scheduleRow = (await api.aqlQuery(api.q('schedules').filter({ id: schedule }).select('*'))).data[0];
    const rule = await send('rule-get', { id: scheduleRow.rule });
    const savedRule = b.normalize({ conditions: rule.conditions, actions: rule.actions });
    snapshots.push(await b.snapshot('helper created schedule'));
    // Exact existing-mobile callsite: helper success, then transaction-delete.
    if (c.persisted) await send('transaction-delete', { id: transaction.id });
    snapshots.push(await b.snapshot('conversion complete'));
    clock = '2026-11-17T12:00:00Z';
    await send('schedule/force-run-service'); snapshots.push(await b.snapshot('due post'));
    await send('schedule/force-run-service'); snapshots.push(await b.snapshot('repeat due service'));
    out.conversion.push({ ...c, input, rule: savedRule, snapshots });
  }
  clock = out.clock;
}

try {
  if (process.argv.includes('--export-discovery-fixtures')) await mkdir(discoveryDirectory);
  if (process.argv.includes('--export-conversion-fixtures')) await mkdir(conversionDirectory);
  engine = await api.init({ dataDir: root });
  await matchingCases();
  await lifecycleCases();
  await discoveryCases();
  await conversionCases();
  const path = new URL('../tests/fixtures/schedule-matching.json', import.meta.url);
  if (process.argv.includes('--check')) assert.deepEqual(out, JSON.parse(await readFile(path, 'utf8')));
  else await writeFile(path, `${JSON.stringify(out, null, 2)}\n`);
  if (discoveryArchives.length) await writeFile(new URL('manifest.json', discoveryDirectory),
    `${JSON.stringify({ oracle: out.oracle, source_commit: out.source_commit, clock: out.clock, archives: discoveryArchives }, null, 2)}\n`, { flag: 'wx' });
  if (conversionArchives.length) await writeFile(new URL('manifest.json', conversionDirectory),
    `${JSON.stringify({ oracle: out.oracle, source_commit: out.source_commit, clock: out.clock, archives: conversionArchives }, null, 2)}\n`, { flag: 'wx' });
  console.log(`PASS ${out.matching.length} matching +${out.lifecycle.length} link/unlink +${out.discovery.length} discovery +${out.conversion.length} conversion +${out.warnings.length} horizon cases`);
} finally {
  await api.shutdown();
  await rm(root, { recursive: true, force: true });
  globalThis.Date = NativeDate;
}
