// Pinned public-engine export/import oracle, never a production migration tool.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';
import { randomBytes } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay, setImmediate as yieldLoop } from 'node:timers/promises';

const requireOracle = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = requireOracle('@actual-app/api');
const accountFixture = process.argv.includes('--accounts');
let root;
const archivePath = fileURLToPath(new URL(accountFixture ? '../tests/fixtures/import-accounts-26.10.0.zip' : '../tests/fixtures/import-26.10.0.zip', import.meta.url));
const manifestPath = fileURLToPath(new URL(accountFixture ? '../tests/fixtures/import-accounts-26.10.0.json' : '../tests/fixtures/import-26.10.0.json', import.meta.url));
const inspector = fileURLToPath(new URL('./inspect-actual-archive.py', import.meta.url));
const unsupportedArchivePath = archivePath.replace('import-26', 'import-unsupported-rule-26');
const unsupportedManifestPath = manifestPath.replace('import-26', 'import-unsupported-rule-26');
const check = process.argv.includes('--check');
let child;
let logs = '';
let initialized = false;
for (const name of ['@actual-app/api', '@actual-app/sync-server']) {
  const entry = fileURLToPath(new URL(`../../actualExpenses/tools/compatibility/node_modules/${name}/package.json`, import.meta.url));
  assert.equal(JSON.parse(await readFile(entry, 'utf8')).version, '26.10.0');
}
root = await mkdtemp(join(tmpdir(), 'actualbudget-import-oracle-'));
const id = index => `04000000-0000-4000-8000-${String(index).padStart(12, '0')}`;
async function publicSnapshot() {
  const accounts = (await api.getAccounts()).sort((a, b) => a.id.localeCompare(b.id));
  const balances = [];
  const transactions = [];
  for (const account of accounts) {
    transactions.push(...await api.getTransactions(account.id));
    const total = await api.aqlQuery(api.q('transactions').filter({ account: account.id }).calculate({ $sum: '$amount' }));
    const cleared = await api.aqlQuery(api.q('transactions').filter({ account: account.id, cleared: true }).calculate({ $sum: '$amount' }));
    const uncleared = await api.aqlQuery(api.q('transactions').filter({ account: account.id, cleared: false }).calculate({ $sum: '$amount' }));
    balances.push({ account_id: account.id, total: total.data ?? 0, cleared: cleared.data ?? 0, uncleared: uncleared.data ?? 0 });
  }
  const sorted = rows => rows.sort((a, b) => a.id.localeCompare(b.id));
  return {
    ...(accountFixture ? { account_groups: await api.getAccountGroups(), notes: await Promise.all(accounts.map(account => api.getNote(account.id))) } : {}),
    accounts, transactions: sorted(transactions), balances,
    payees: sorted(await api.getPayees()), categories: sorted(await api.getCategories()),
    category_groups: sorted(await api.getCategoryGroups()), preferences: await api.getPreferences(), rules: sorted(await api.getRules()),
  };
}
async function init(dataDir, serverURL, password) {
  await mkdir(dataDir, { recursive: true });
  await api.init({ dataDir, ...(serverURL ? { serverURL, password } : {}) });
  initialized = true;
}
async function shutdown() {
  if (initialized) { await api.shutdown(); initialized = false; }
}
async function startPinnedServer() {
  const reservation = createServer();
  await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  const serverDir = join(root, 'server');
  await mkdir(serverDir);
  const packagePath = fileURLToPath(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/sync-server/package.json', import.meta.url));
  child = spawn(process.execPath, [join(packagePath, '../build/bin/actual-server.js')], {
    cwd: root, env: { ...process.env, NODE_ENV: 'production', ACTUAL_DATA_DIR: serverDir, ACTUAL_HOSTNAME: '127.0.0.1', ACTUAL_PORT: String(port) },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', chunk => { logs = (logs + chunk).slice(-16384); });
  child.stderr.on('data', chunk => { logs = (logs + chunk).slice(-16384); });
  const url = `http://127.0.0.1:${port}`;
  const deadline = performance.now() + 30000;
  for (;;) {
    assert.equal(child.exitCode, null, `Pinned server exited: ${logs}`);
    try { if ((await fetch(`${url}/account/needs-bootstrap`, { signal: AbortSignal.timeout(1000) })).ok) break; } catch {}
    assert.ok(performance.now() < deadline, `Pinned server failed startup: ${logs}`);
    await delay(100);
  }
  const password = randomBytes(24).toString('hex');
  const response = await fetch(`${url}/account/bootstrap`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ password }) });
  assert.equal(response.status, 200);
  return { url, password };
}
function inspect(path) {
  return JSON.parse(execFileSync('python3', [inspector, path], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 }));
}
async function verifyPublicImport(archive, expected) {
  await shutdown();
  await init(await mkdtemp(join(root, 'roundtrip-client-')));
  const imported = await api.importBudget(archive, { type: 'actual' });
  assert.equal(imported.id, expected.inspection.metadata.id, 'Public import preserves local budget identity');
  const result = await publicSnapshot();
  assert.deepEqual(result, expected.public, 'Pinned public import changes financial records, IDs, relationships or preferences');
  // Re-export through the public engine, inspect only for ordering/identity retention.
  const roundtripPath = join(root, 'roundtrip.zip');
  await writeFile(roundtripPath, await api.exportBudget());
  const audit = inspect(roundtripPath);
  assert.deepEqual(audit.latest_field_clocks, expected.inspection.latest_field_clocks, 'New client clock must not rewrite original field ordering');
  assert.deepEqual(audit.records.transactions, expected.inspection.records.transactions, 'Export/import preserves tombstones and original transaction IDs');
  assert.deepEqual(audit.records.messages_pending, expected.inspection.records.messages_pending);
}
try {
  if (check) {
    for (const [archive, manifest] of (accountFixture ? [[archivePath, manifestPath]] : [[archivePath, manifestPath], [unsupportedArchivePath, unsupportedManifestPath]])) {
      const expected = JSON.parse(await readFile(manifest, 'utf8'));
      assert.deepEqual(inspect(archive), expected.inspection, 'Archive bytes, source schema, IDs, clocks or retained records changed');
      await verifyPublicImport(await readFile(archive), expected);
    }
    console.log('PASS pinned Actual archive checksum/schema/identities/clocks and public import financial roundtrip');
  } else {
    const { url, password } = await startPinnedServer();
    await init(join(root, 'source-client'), url, password);
    let spending, reserve, category, payee;
    await api.runImport('Disposable actualBudget migration fixture', async () => {
      spending = await api.createAccount({ name: 'Spending', offbudget: false }, 12500);
      reserve = await api.createAccount({ name: 'Reserve', offbudget: true }, -5000);
      const group = await api.createCategoryGroup({ name: 'Everyday', is_income: false });
      category = await api.createCategory({ name: 'Food', group_id: group, is_income: false });
      payee = await api.createPayee({ name: 'Fixture shop' });
    });
    await api.sync();
    // Post-import public mutations run with real syncing enabled and produce HLC history.
    await api.setPreference('budgetType', 'tracking');
    await api.setPreference('numberFormat', 'comma-dot');
    await api.setPreference('dateFormat', 'yyyy-MM-dd');
    await api.setPreference('hideFraction', 'false');
    await api.setPreference('defaultCurrencyCode', 'USD');
    await api.addTransactions(spending, [
      { id: id(1), date: '2026-01-01', amount: 20000, cleared: true, notes: 'Income' },
      { id: id(2), date: '2026-01-02', amount: -1234, payee, category, cleared: false, notes: 'Expense before correction' },
      { id: id(3), date: '2026-01-03', amount: 234, payee, category, cleared: true, notes: 'Refund' },
      { id: id(4), date: '2026-01-04', amount: -99, cleared: false, notes: 'Published deletion' },
      { id: id(5), date: '2027-01-01', amount: -100, cleared: false, notes: 'Future expense' },
    ]);
    await api.sync();
    await api.updateTransaction(id(2), { amount: -1200, notes: 'Corrected expense', cleared: true });
    await api.deleteTransaction(id(4));
    await api.sync();
    const deadline = performance.now() + 5000;
    for (;;) {
      const rows = await api.getTransactions(spending);
      if (rows.find(row => row.id === id(2))?.amount === -1200 && !rows.some(row => row.id === id(4))) break;
      assert.ok(performance.now() < deadline, 'Public API mutation visibility timed out');
      await yieldLoop();
    }
    if (accountFixture) {
      const group = await api.createAccountGroup({ name: 'Daily accounts' });
      const removedGroup = await api.createAccountGroup({ name: 'Removed group' });
      await api.updateAccount(spending, { name: 'Daily spending', account_group_id: group });
      await api.updateAccount(reserve, { account_group_id: group });
      await api.updateNote(spending, 'Imported account note\nRetain history');
      const closed = await api.createAccount({ name: 'Closed history', offbudget: false }, 0);
      await api.addTransactions(closed, [{ id: id(21), amount: 2500, date: '2026-01-01', cleared: true }, { id: id(22), amount: -2500, date: '2026-01-02', cleared: true }]);
      await api.closeAccount(closed);
      const empty = await api.createAccount({ name: 'Deleted empty', offbudget: true }, 0);
      await api.closeAccount(empty);
      await api.deleteAccountGroup(removedGroup);
      await api.setPreference('flags.newSidebarUI', 'true');
      await api.sync();
    }
    const expected = { oracle: '@actual-app/api@26.10.0 + @actual-app/sync-server@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577', generated_at: new Date().toISOString(), public: await publicSnapshot() };
    const generated = join(root, 'generated.zip');
    const archive = await api.exportBudget();
    await writeFile(generated, archive);
    expected.inspection = inspect(generated);
    assert.ok(expected.inspection.latest_field_clocks.length > 0, 'Genuine synced field clocks required');
    assert.ok(expected.inspection.latest_field_clocks.some(row => row.row === id(4) && row.column === 'tombstone' && row.value === 'N:1'), 'Published deletion clock required');
    assert.equal(expected.public.balances.find(row => row.account_id === spending).total, 31434);
    assert.equal(expected.public.balances.find(row => row.account_id === reserve).total, -5000);
    if (!accountFixture) {
    // A genuine later-feature record proves the limited importer must report and
    // retain unsupported data instead of silently losing it.
    await api.createRule({ stage: null, conditionsOp: 'and', conditions: [{ field: 'payee', op: 'is', value: payee }], actions: [{ op: 'set', field: 'category', value: category }] });
    await api.sync();
    const unsupported = { ...expected, public: await publicSnapshot() };
    const unsupportedArchive = await api.exportBudget();
    const unsupportedPath = join(root, 'unsupported.zip');
    await writeFile(unsupportedPath, unsupportedArchive);
    unsupported.inspection = inspect(unsupportedPath);
    assert.equal(unsupported.public.rules.length, 1);
    await verifyPublicImport(archive, expected);
    await verifyPublicImport(unsupportedArchive, unsupported);
    await writeFile(unsupportedArchivePath, unsupportedArchive);
    await writeFile(unsupportedManifestPath, JSON.stringify(unsupported, null, 2) + '\n');
    } else await verifyPublicImport(archive, expected);
    await writeFile(archivePath, archive);
    await writeFile(manifestPath, JSON.stringify(expected, null, 2) + '\n');
    console.log('Wrote public export fixture and independent public financial/identity/HLC roundtrip manifest');
  }
} finally {
  await shutdown();
  if (child && child.exitCode === null) {
    const closed = new Promise(resolve => child.once('close', resolve));
    child.kill('SIGTERM');
    await closed;
  }
  await rm(root, { recursive: true, force: true });
}
