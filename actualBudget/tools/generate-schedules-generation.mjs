// Genuine synchronized source mutations; never manufacture SQLite state or clocks.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:net';
import { randomBytes, createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

process.env.TZ = 'UTC';
const NativeDate = Date;
let clock = '2026-11-09T12:00:00.000Z';
globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [clock])); }
  static now() { return NativeDate.parse(clock); }
};
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
const fixtures = new URL('../tests/fixtures/', import.meta.url);
const metadataPath = new URL('schedules-generation.json', fixtures);
const check = process.argv.includes('--check');
const existing = await readFile(metadataPath, 'utf8').catch(error => {
  if (error.code === 'ENOENT') return null;
  throw error;
});
assert.ok(check ? existing : !existing, check ? 'Missing committed generation metadata' : 'Refusing to overwrite source fixtures');
const root = await mkdtemp(join(tmpdir(), 'schedules-generation-'));
const schedule = '21220000-0000-4000-8000-000000000919';
let child, initialized = false, logs = '';
const result = { oracle: '@actual-app/api@26.10.0 + @actual-app/sync-server@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577', schedule_id: schedule, stages: [] };
const inspect = path => JSON.parse(execFileSync('python3', ['-c', `
import json,pathlib,sqlite3,tempfile,zipfile
with zipfile.ZipFile(__import__('sys').argv[1]) as z,tempfile.TemporaryDirectory() as tmp:
 p=pathlib.Path(tmp)/'source.sqlite';p.write_bytes(z.read('db.sqlite'))
 db=sqlite3.connect(p.as_uri()+'?mode=ro&immutable=1',uri=True);db.row_factory=sqlite3.Row
 assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
 states=[dict(r) for r in db.execute('SELECT * FROM schedules_next_date WHERE tombstone=0')]
 assert len(states)==1
 state=states[0]
 clocks={r['column']:r['timestamp'] for r in db.execute('SELECT * FROM messages_crdt WHERE dataset=? AND row=? ORDER BY timestamp',('schedules_next_date',state['id']))}
 assert all(k in clocks for k in ['local_next_date','base_next_date','local_next_date_ts','base_next_date_ts'])
 print(json.dumps({'state':state,'clocks':clocks,'transaction_count':db.execute('SELECT COUNT(*) FROM transactions WHERE tombstone=0').fetchone()[0]}))
`, path], { encoding: 'utf8' }));
async function json(url, body) {
  const response = await fetch(url, { ...(body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }), signal: AbortSignal.timeout(5000) });
  assert.equal(response.status, 200, await response.clone().text());
  const value = await response.json(); assert.equal(value.status, 'ok'); return value.data;
}
try {
  for (const name of ['api', 'sync-server']) assert.equal(JSON.parse(await readFile(new URL(`../../actualExpenses/tools/compatibility/node_modules/@actual-app/${name}/package.json`, import.meta.url), 'utf8')).version, '26.10.0');
  const reservation = createServer();
  await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  const serverDir = join(root, 'server'); await mkdir(serverDir);
  child = spawn(process.execPath, [fileURLToPath(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/sync-server/build/bin/actual-server.js', import.meta.url))], {
    cwd: root, env: { ...process.env, NODE_ENV: 'production', ACTUAL_DATA_DIR: serverDir, ACTUAL_HOSTNAME: '127.0.0.1', ACTUAL_PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { logs = (logs + data).slice(-16384); });
  const url = `http://127.0.0.1:${port}`, deadline = performance.now() + 30000;
  for (;;) {
    assert.equal(child.exitCode, null, logs);
    try { if ((await fetch(url + '/account/needs-bootstrap', { signal: AbortSignal.timeout(1000) })).ok) break; } catch {}
    assert.ok(performance.now() < deadline, logs); await delay(100);
  }
  const password = randomBytes(24).toString('hex');
  await json(url + '/account/bootstrap', { password });
  const dataDir = join(root, 'client'); await mkdir(dataDir);
  const engine = await api.init({ dataDir, serverURL: url, password }); initialized = true;
  let account;
  await api.runImport('Schedule generation source', async () => { account = await api.createAccount({ name: 'Spending' }, 0); });
  await api.sync();
  const conditions = start => [{ field: 'account', op: 'is', value: account }, { field: 'amount', op: 'is', value: -100 }, { field: 'date', op: 'is', value: { start, frequency: 'daily', endMode: 'never' } }];
  const capture = async name => {
    await api.sync();
    const observed = (await api.aqlQuery(api.q('schedules').filter({ id: schedule }).select('*'))).data[0];
    assert.ok(observed);
    const bytes = await api.exportBudget();
    const local = join(root, `${name}.zip`); await writeFile(local, bytes);
    const audit = inspect(local);
    assert.equal(audit.transaction_count, 0);
    const selected = audit.state.local_next_date_ts === audit.state.base_next_date_ts ? 'local_next_date' : 'base_next_date';
    assert.equal(observed.next_date.replaceAll('-', ''), String(audit.state[selected]));
    const guard = [audit.clocks[selected], audit.clocks.local_next_date_ts, audit.clocks.base_next_date_ts].sort().at(-1);
    const archive = `schedules-generation-${name}-26.10.zip`;
    if (!check) await writeFile(new URL(archive, fixtures), bytes, { flag: 'wx' });
    result.stages.push({ name, archive, archive_sha256: createHash('sha256').update(bytes).digest('hex'), clock,
      next_date: observed.next_date, selected_field: selected, selected_clock: audit.clocks[selected], guard_clock: guard,
      selector_newer_than_date: guard > audit.clocks[selected], ...audit });
  };
  clock = '2026-11-09T12:00:01.000Z';
  assert.equal(await engine.send('schedule/create', { schedule: { id: schedule, name: 'Generation guards', posts_transaction: false }, conditions: conditions('2026-11-09') }), schedule);
  await capture('created');
  clock = '2026-11-09T12:00:02.000Z'; await engine.send('schedule/skip-next-date', { id: schedule }); await capture('skipped');
  clock = '2026-11-09T12:00:03.000Z'; await engine.send('schedule/update', { schedule: { id: schedule }, conditions: conditions('2026-11-13'), resetNextDate: true }); await capture('reset');
  clock = '2026-11-09T12:00:04.000Z'; await engine.send('schedule/skip-next-date', { id: schedule }); await capture('skipped-after-reset');
  assert.ok(result.stages.some(stage => stage.selected_field === 'base_next_date' && stage.selector_newer_than_date));
  if (check) {
    const prior = JSON.parse(existing);
    const semantic = stage => ({ name: stage.name, next_date: stage.next_date, selected_field: stage.selected_field,
      selector_newer_than_date: stage.selector_newer_than_date, clock_fields: Object.keys(stage.clocks).sort(),
      state: { ...stage.state, id: '<generated>' }, transaction_count: stage.transaction_count });
    assert.deepEqual(result.stages.map(semantic), prior.stages.map(semantic));
    for (const stage of prior.stages) {
      const path = fileURLToPath(new URL(stage.archive, fixtures));
      assert.equal(createHash('sha256').update(await readFile(path)).digest('hex'), stage.archive_sha256);
      const audit = inspect(path);
      assert.deepEqual(audit, { state: stage.state, clocks: stage.clocks, transaction_count: stage.transaction_count });
    }
  } else await writeFile(metadataPath, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  console.log('PASS four genuine synced source generation states, original cell clocks and selector-newer-than-date witness');
} finally {
  if (initialized) await api.shutdown();
  if (child && child.exitCode === null) { const closed = new Promise(resolve => child.once('close', resolve)); child.kill('SIGTERM'); await closed; }
  globalThis.Date = NativeDate;
  await rm(root, { recursive: true, force: true });
}
