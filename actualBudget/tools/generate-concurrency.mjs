// Independent oracle: published Actual sync endpoint + public API, disposable data only.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:net';
import { randomBytes, createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const oracle = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = oracle('@actual-app/api');
const crdt = await import(pathToFileURL(oracle.resolve('@actual-app/crdt')));
const { Timestamp, makeClock, setClock, create, toBinary, MessageSchema, MessageEnvelopeSchema, SyncRequestSchema } = crdt;
const fixturePath = new URL('../tests/fixtures/concurrency.json', import.meta.url);
const root = await mkdtemp(join(tmpdir(), 'actualbudget-concurrency-oracle-'));
let child, initialized = false, logs = '';
const sourcePath = new URL('../reference/actual-26.10.0/packages/crdt/src/crdt/timestamp.ts', import.meta.url);
const fixedNow = Date.parse('2026-01-01T00:00:00.000Z');
const zero = '1970-01-01T00:00:00.000Z-0000-0000000000000000';
const id = n => `05000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const clock = (day, node, counter = '0000') => `2026-01-${String(day).padStart(2, '0')}T00:00:00.000Z-${counter}-${String(node).padStart(16, '0')}`;
const scenarios = [
  { name: 'late-independent-field-and-old-same-field', id: id(1), mutations: [
    { clock: clock(3, 1), fields: { notes: 'newest note' } },
    { clock: clock(2, 1), fields: { amount: -250 } },
    { clock: clock(1, 1), fields: { notes: 'older note' } },
    { clock: clock(2, 1), fields: { amount: -250 }, replay: true },
  ] },
  { name: 'same-time-node-tie', id: id(2), mutations: [
    { clock: clock(3, 22, '0001'), fields: { notes: 'larger node' } },
    { clock: clock(3, 21, '0001'), fields: { notes: 'smaller node arrives later' } },
  ] },
  { name: 'counter-before-node', id: id(3), mutations: [
    { clock: clock(3, 33, '0001'), fields: { notes: 'lower counter' } },
    { clock: clock(3, 31, '0002'), fields: { notes: 'higher counter' } },
  ] },
  { name: 'published-tombstone-with-later-ordinary-edit', id: id(4), mutations: [
    { clock: clock(3, 4), fields: { tombstone: 1 } },
    { clock: clock(4, 4), fields: { notes: 'later hidden edit' } },
  ] },
  { name: 'raw-actual-old-deletion-policy-boundary', id: id(5), mutations: [
    { clock: clock(3, 5), fields: { notes: 'newer retained work' } },
    { clock: clock(2, 5), fields: { tombstone: 1 } },
  ], policy: 'Actual applies the independent tombstone field. The approved replacement rejects this unpublished deletion before publication; this scenario documents the intentional stronger policy, not direct parity.' },
  { name: 'account-category-older-first', id: id(6), mutations: [
    { clock: clock(2, 6), fields: { acct: '$off' } },
    { clock: clock(2, 6, '0001'), fields: { category: null } },
    { clock: clock(3, 6), fields: { category: '$category' } },
  ] },
  { name: 'account-category-newer-first', id: id(7), mutations: [
    { clock: clock(3, 7), fields: { category: '$category' } },
    { clock: clock(2, 7), fields: { acct: '$off' } },
    { clock: clock(2, 7, '0001'), fields: { category: null } },
  ] },
];
// Keep runtime-generated IDs out of the fixture; Actual's wire null is 0:.
function serializeValue(value, references) {
  if (value === null) return '0:';
  const resolved = references[value] ?? value;
  return (typeof resolved === 'number' ? 'N:' : 'S:') + resolved;
}
function clockOracle() {
  const actualNow = Date.now;
  const result = { physical_time: new Date(fixedNow).toISOString(), sends: [], receives: [] };
  try {
    Date.now = () => fixedNow;
    setClock(makeClock(Timestamp.parse(zero)));
    result.sends.push(Timestamp.send().toString(), Timestamp.send().toString());
    Date.now = () => fixedNow - 1000;
    result.sends.push(Timestamp.send().toString());
    Date.now = () => fixedNow;
    for (const offset of [-1000, 300000, 300001]) {
      setClock(makeClock(Timestamp.parse(zero)));
      const input = new Timestamp(fixedNow + offset, 0, '0000000000000001');
      try { result.receives.push({ input: input.toString(), result: Timestamp.recv(input).toString() }); }
      catch (e) { result.receives.push({ input: input.toString(), error: e.name }); }
    }
    setClock(makeClock(new Timestamp(fixedNow, 65535, '0000000000000001')));
    try { Timestamp.send(); throw Error('Expected overflow'); }
    catch (e) { assert.equal(e.name, 'OverflowError'); result.counter_overflow = e.name; }
    return result;
  } finally { Date.now = actualNow; }
}
async function json(url, body, headers = {}) {
  const response = await fetch(url, { ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }), headers: { 'Content-Type': 'application/json', ...headers } });
  assert.equal(response.status, 200, `${url}: ${await response.clone().text()}`);
  const result = await response.json(); assert.equal(result.status, 'ok'); return result.data;
}
async function boot() {
  const socket = createServer();
  await new Promise(resolve => socket.listen(0, '127.0.0.1', resolve));
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const directory = join(root, 'server'); await mkdir(directory);
  const entry = fileURLToPath(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/sync-server/build/bin/actual-server.js', import.meta.url));
  child = spawn(process.execPath, [entry], { cwd: root, env: { ...process.env, NODE_ENV: 'production', ACTUAL_DATA_DIR: directory, ACTUAL_HOSTNAME: '127.0.0.1', ACTUAL_PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'] });
  for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { logs = (logs + data).slice(-16384); });
  const url = `http://127.0.0.1:${port}`, deadline = performance.now() + 30000;
  for (;;) {
    assert.equal(child.exitCode, null, logs);
    try { if ((await fetch(url + '/account/needs-bootstrap', { signal: AbortSignal.timeout(1000) })).ok) break; } catch {}
    assert.ok(performance.now() < deadline, logs); await delay(100);
  }
  const password = randomBytes(24).toString('hex');
  await json(url + '/account/bootstrap', { password });
  const { token } = await json(url + '/account/login', { password });
  return { url, password, token };
}
try {
  for (const name of ['api', 'sync-server']) {
    const pkg = JSON.parse(await readFile(new URL(`../../actualExpenses/tools/compatibility/node_modules/@actual-app/${name}/package.json`, import.meta.url)));
    assert.equal(pkg.version, '26.10.0');
  }
  const clockResults = clockOracle();
  const { url, password, token } = await boot();
  const clientDir = join(root, 'client'); await mkdir(clientDir);
  await api.init({ dataDir: clientDir, serverURL: url, password }); initialized = true;
  let account, off, category;
  await api.runImport('Disposable concurrency oracle', async () => {
    account = await api.createAccount({ name: 'Spending', offbudget: false }, 0);
    off = await api.createAccount({ name: 'Off', offbudget: true }, 0);
    const group = await api.createCategoryGroup({ name: 'Expenses', is_income: false });
    category = await api.createCategory({ name: 'Food', group_id: group, is_income: false });
    await api.addTransactions(account, scenarios.map(s => ({ id: s.id, date: '2026-01-01', amount: -100, notes: 'initial', cleared: true })));
  });
  await api.sync();
  const files = await json(url + '/sync/list-user-files', undefined, { 'X-ACTUAL-TOKEN': token });
  assert.equal(files.length, 1);
  const file = files[0];
  for (const scenario of scenarios) for (const mutation of scenario.mutations) {
    const messages = Object.entries(mutation.fields).map(([column, value]) => create(MessageEnvelopeSchema, {
      timestamp: mutation.clock, isEncrypted: false,
      content: toBinary(MessageSchema, create(MessageSchema, { dataset: 'transactions', row: scenario.id, column, value: serializeValue(value, { $off: off, $category: category }) })),
    }));
    const body = toBinary(SyncRequestSchema, create(SyncRequestSchema, { fileId: file.fileId, groupId: file.groupId, since: zero, messages }));
    const response = await fetch(url + '/sync/sync', { method: 'POST', headers: { 'X-ACTUAL-TOKEN': token, 'Content-Type': 'application/actual-sync' }, body });
    assert.equal(response.status, 200, await response.clone().text());
    await api.sync();
  }
  const archive = await api.exportBudget();
  const archivePath = join(root, 'observed.zip'); await writeFile(archivePath, archive);
  const inspector = fileURLToPath(new URL('./inspect-actual-archive.py', import.meta.url));
  const audit = JSON.parse(execFileSync('python3', [inspector, archivePath], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 }));
  await api.shutdown(); initialized = false;
  const freshDir = join(root, 'fresh'); await mkdir(freshDir);
  await api.init({ dataDir: freshDir }); initialized = true;
  await api.importBudget(archive, { type: 'actual' });
  const visible = [...await api.getTransactions(account), ...await api.getTransactions(off)];
  for (const scenario of scenarios) {
    const row = audit.records.transactions.find(row => row.id === scenario.id);
    assert.ok(row);
    const versions = Object.fromEntries(audit.latest_field_clocks.filter(v => v.dataset === 'transactions' && v.row === scenario.id).map(v => [v.column, v.timestamp]));
    scenario.initial = { amount: -100, notes: 'initial', date: '2026-01-01', cleared: true, clock: zero };
    scenario.expected = { amount: row.amount, notes: row.notes, tombstone: Boolean(row.tombstone), visible: visible.some(v => v.id === scenario.id), field_versions: versions };
    if (scenario.name.startsWith('account-category-')) {
      assert.equal(row.acct, off);
      assert.equal(row.category, category);
      scenario.expected.account = 'off';
      scenario.expected.category = 'Food';
    }
    assert.equal(scenario.expected.visible, !scenario.expected.tombstone);
  }
  const result = { oracle: 'Actual API/sync-server 26.10.0 and its locked CRDT package', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577', timestamp_source_sha256: createHash('sha256').update(await readFile(sourcePath)).digest('hex'), clocks: clockResults, scenarios };
  const output = JSON.stringify(result, null, 2) + '\n';
  if (process.argv.includes('--check')) assert.equal(await readFile(fixturePath, 'utf8'), output);
  else await writeFile(fixturePath, output);
  console.log('PASS seven pinned sync/public-API ordering scenarios and HLC drift/counter cases');
} finally {
  if (initialized) await api.shutdown();
  if (child && child.exitCode === null) { const closed = new Promise(resolve => child.once('close', resolve)); child.kill('SIGTERM'); await closed; }
  await rm(root, { recursive: true, force: true });
}
