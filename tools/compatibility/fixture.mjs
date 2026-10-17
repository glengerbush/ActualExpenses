import { spawn } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { createServer, request } from 'node:http';
import { createWriteStream } from 'node:fs';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as api from '@actual-app/api';

// This program creates its own loopback server and disposable budget. It never
// accepts a production server URL or reuses an existing server data directory.
const here = path.dirname(fileURLToPath(import.meta.url));
const root = await mkdtemp(path.join(tmpdir(), 'actualexpenses-fixture-'));
const fixturePath = path.resolve(process.env.ACTUAL_FIXTURE || path.join(here, '.fixture/fixture.json'));
const password = randomBytes(24).toString('hex');
const serverDir = path.join(root, 'server');
const clientDir = path.join(root, 'official-client');
await Promise.all([mkdir(serverDir), mkdir(clientDir), mkdir(path.dirname(fixturePath), { recursive: true })]);

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
}
const reservation = createServer();
const port = await listen(reservation);
await new Promise(resolve => reservation.close(resolve));
const directServerUrl = `http://127.0.0.1:${port}`;
const serverLog = createWriteStream(path.join(root, 'server.log'));
const child = spawn(process.execPath, [path.join(here, 'node_modules/@actual-app/sync-server/build/bin/actual-server.js')], {
  cwd: root,
  env: { ...process.env, NODE_ENV: 'production', ACTUAL_DATA_DIR: serverDir, ACTUAL_HOSTNAME: '127.0.0.1', ACTUAL_PORT: String(port) },
  stdio: ['ignore', 'pipe', 'pipe'],
});
child.stdout.pipe(serverLog);
child.stderr.pipe(serverLog);
let proxy;
let oracle;
let stopping = false;
async function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  try { await api.shutdown(); } catch {}
  proxy?.closeAllConnections();
  oracle?.closeAllConnections();
  proxy?.close();
  oracle?.close();
  child.kill('SIGTERM');
  serverLog.end();
  process.exit(code);
}
process.on('SIGINT', () => stop());
process.on('SIGTERM', () => stop());
process.on('uncaughtException', error => { console.error(error); stop(1); });
process.on('unhandledRejection', error => { console.error(error); stop(1); });

async function json(url, body) {
  const response = await fetch(url, body === undefined ? {} : {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok || result.status === 'error') throw new Error(`${url}: ${JSON.stringify(result)}`);
  return result;
}
let ready = false;
for (let attempt = 0; attempt < 150; attempt++) {
  if (child.exitCode !== null) throw new Error(`Actual exited; inspect ${root}/server.log`);
  try { await json(`${directServerUrl}/account/needs-bootstrap`); ready = true; break; } catch {}
  await new Promise(resolve => setTimeout(resolve, 200));
}
if (!ready) throw new Error(`Actual did not start; inspect ${root}/server.log`);
await json(`${directServerUrl}/account/bootstrap`, { password });
const login = await json(`${directServerUrl}/account/login`, { password });
const token = login.data.token;
await api.init({ dataDir: clientDir, serverURL: directServerUrl, password });
let accountId;
let payeeId;
let categoryId;
let categoryGroupId;
await api.runImport('ActualExpenses disposable compatibility budget', async () => {
  accountId = await api.createAccount({ name: 'Spending', offbudget: false }, 0);
  categoryGroupId = await api.createCategoryGroup({ name: 'Everyday', is_income: false });
  categoryId = await api.createCategory({ name: 'Groceries', group_id: categoryGroupId, is_income: false });
  payeeId = await api.createPayee({ name: 'Fixture shop' });
  await api.createRule({ stage: null, conditionsOp: 'and', conditions: [{ field: 'payee', op: 'is', value: payeeId }], actions: [{ op: 'set', field: 'category', value: categoryId }] });
  await api.addTransactions(accountId, [{ date: '2026-10-01', amount: -1234, payee: payeeId, category: categoryId, notes: 'Independent client seed', cleared: true }]);
});
await api.sync();
const files = await fetch(`${directServerUrl}/sync/list-user-files`, { method: 'GET', headers: { 'X-ACTUAL-TOKEN': token } }).then(r => r.json());
if (files.status !== 'ok' || files.data.length !== 1) throw new Error(`Expected one uploaded budget: ${JSON.stringify(files)}`);
const budget = files.data[0];

let fault = null;
const faultEvents = [];
proxy = createServer(async (incoming, outgoing) => {
  const chunks = [];
  for await (const chunk of incoming) chunks.push(chunk);
  const body = Buffer.concat(chunks);
  // Protobuf SyncRequest has repeated messages in field 1. Both clients encode
  // fields in order; an empty pull starts at fileId (field 2).
  const isSync = incoming.url === '/sync/sync';
  const isWrite = isSync && body[0] === 0x0a;
  const trigger = fault && isSync && (!fault.onlyWrites || isWrite) ? fault : null;
  if (trigger) {
    fault = null;
    faultEvents.push({ mode: trigger.mode, write: isWrite, at: new Date().toISOString() });
  }
  if (trigger?.mode === 'before-send') { incoming.socket.destroy(); return; }
  const upstream = request(`${directServerUrl}${incoming.url}`, {
    method: incoming.method, headers: { ...incoming.headers, host: `127.0.0.1:${port}` },
  }, response => {
    if (trigger?.mode === 'after-accept') {
      // Drain the actual server response before severing the client connection.
      response.resume();
      response.on('end', () => incoming.socket.destroy());
    } else {
      outgoing.writeHead(response.statusCode, response.headers);
      response.pipe(outgoing);
    }
  });
  upstream.on('error', error => { if (!outgoing.headersSent) outgoing.writeHead(502); outgoing.end(String(error)); });
  upstream.end(body);
});
const proxyPort = await listen(proxy);
let queue = Promise.resolve();
async function handleOracle(incoming, outgoing) {
  const chunks = [];
  for await (const chunk of incoming) chunks.push(chunk);
  const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : {};
  let result;
  const pathname = new URL(incoming.url, 'http://localhost').pathname;
  if (incoming.method === 'GET' && pathname === '/state') {
    await api.sync();
    result = { accounts: await api.getAccounts(), payees: await api.getPayees(), categories: await api.getCategories(), rules: await api.getRules(), transactions: await api.getTransactions(accountId), faultEvents };
  } else if (incoming.method === 'POST' && pathname === '/transactions') {
    await api.sync();
    const transactions = body.transactions.map(transaction => ({ ...transaction, id: transaction.id || randomUUID() }));
    await api.addTransactions(body.accountId || accountId, transactions);
    result = { ids: transactions.map(transaction => transaction.id) };
    await api.sync();
  } else if (incoming.method === 'PATCH' && pathname.startsWith('/transactions/')) {
    await api.sync();
    await api.updateTransaction(decodeURIComponent(pathname.slice('/transactions/'.length)), body);
    await api.sync();
    result = { updated: true };
  } else if (incoming.method === 'DELETE' && pathname.startsWith('/transactions/')) {
    await api.sync();
    await api.deleteTransaction(decodeURIComponent(pathname.slice('/transactions/'.length)));
    await api.sync();
    result = { deleted: true };
  } else if (incoming.method === 'POST' && pathname === '/fault') {
    if (!['before-send', 'after-accept'].includes(body.mode)) throw new Error('Unknown fault mode');
    fault = { mode: body.mode, onlyWrites: body.onlyWrites !== false };
    result = { armed: true };
  } else {
    outgoing.writeHead(404); outgoing.end(); return;
  }
  outgoing.writeHead(200, { 'content-type': 'application/json' });
  outgoing.end(JSON.stringify(result));
}
oracle = createServer((incoming, outgoing) => {
  queue = queue.then(() => handleOracle(incoming, outgoing)).catch(error => {
    outgoing.writeHead(500, { 'content-type': 'application/json' });
    outgoing.end(JSON.stringify({ error: String(error) }));
  });
});
const oraclePort = await listen(oracle);
const fixture = { version: '26.10.0', serverUrl: `http://127.0.0.1:${proxyPort}`, directServerUrl, oracleUrl: `http://127.0.0.1:${oraclePort}`, password, token, budgetId: budget.fileId, groupId: budget.groupId, accountId, payeeId, categoryId, categoryGroupId, root };
await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, { mode: 0o600 });
console.log(`FIXTURE_READY ${fixturePath}`);
