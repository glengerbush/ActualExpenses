// Observe the pinned engine's registered parser, never the replacement parser.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';

process.env.TZ = 'UTC';
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json'), 'utf8')).version, '26.10.0');
const root = new URL('../reference/actual-26.10.0/', import.meta.url);
const fixture = new URL('../tests/fixtures/transaction-parser-source.json', import.meta.url);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const cases = [];
const add = (id, filename, bytes, options = {}, source = null) => cases.push({ id, filename, bytes: Buffer.from(bytes), options, source });
const files = ['data.ofx', 'data.qfx', 'credit-card.ofx', 'data-multi-decimal.ofx', 'data-payee-memo.ofx', 'data.qif', 'data-payee-memo.qif', 'qif-category.qif', '8859-1.qfx', '1252.qfx', 'utf-8.qfx', 'html-vals.qfx', 'camt/camt.053.xml', 'camt/camt.053.payee-memo.xml', 'camt/camt.latin1.xml'];
for (const name of files) {
  const path = `packages/loot-core/src/mocks/files/${name}`;
  const bytes = await readFile(new URL(path, root));
  const filename = name.split('/').at(-1);
  add(name, filename, bytes, { importNotes: true }, path);
  if (/payee-memo/.test(name)) {
    add(`${name}-swap`, filename, bytes, { importNotes: true, swapPayeeAndMemo: true }, path);
    add(`${name}-no-notes`, filename, bytes, { importNotes: false }, path);
    if (name.endsWith('.ofx')) add(`${name}-fallback`, filename, bytes, { importNotes: true, fallbackMissingPayeeToMemo: true }, path);
  }
}
const csv = 'Date,Payee,Amount,Memo\n2026-11-09,Café,-12.34,"quoted, memo"\n';
add('csv-header', 'bank.csv', csv, { hasHeaderRow: true });
add('csv-no-header', 'bank.csv', '2026-11-09,Cafe,-12.34\n2026-11-10,Shop,3.00\n', { hasHeaderRow: false });
add('csv-quotes-multiline', 'bank.csv', 'Date,Payee,Amount,Memo\r\n2026-11-09,"Cafe ""A""",-12.34,"first\r\nsecond"\r\n', { hasHeaderRow: true });
add('csv-relaxed-columns', 'bank.csv', 'Date,Payee,Amount\n2026-11-09,Cafe\n2026-11-10,Shop,-2,extra\n', { hasHeaderRow: true });
add('csv-trim-empty', 'bank.csv', 'Date,Payee,Amount\n\n 2026-11-09 , Cafe , -12.34 \n', { hasHeaderRow: true });
add('csv-quoted-edge-spaces', 'bank.csv', 'Date,Payee,Amount\n2026-11-09," Cafe ",-12.34\n', { hasHeaderRow: true });
add('csv-quoted-header-spaces', 'bank.csv', '" Date ",Payee,Amount\n2026-11-09," Cafe ",-12.34\n', { hasHeaderRow: true });
add('tsv-header', 'bank.tsv', csv.replaceAll(',', '\t'), { hasHeaderRow: true, delimiter: '\t' });
add('csv-semicolon', 'bank.csv', 'Date;Payee;Amount\n2026-11-09;Café;-12,34\n', { hasHeaderRow: true, delimiter: ';' });
add('csv-physical-skip', 'bank.csv', `bank header\n${csv}footer\n`, { hasHeaderRow: true, skipStartLines: 1, skipEndLines: 2 });
add('csv-skip-all', 'bank.csv', csv, { hasHeaderRow: true, skipStartLines: 100 });
add('csv-negative-skip', 'bank.csv', csv, { hasHeaderRow: true, skipStartLines: -1, skipEndLines: -2 });
add('csv-bad-quote', 'bank.csv', 'Date,Payee,Amount\n2026-11-09,"Cafe,-12.34\n', { hasHeaderRow: true });
add('csv-bad-encoding', 'bank.csv', csv, { hasHeaderRow: true, encoding: 'not-an-encoding' });
add('csv-utf8-bom', 'bank.csv', Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(csv)]), { hasHeaderRow: true });
const le = Buffer.from(csv, 'utf16le');
add('csv-utf16le-bom', 'bank.csv', Buffer.concat([Buffer.from([0xff, 0xfe]), le]), { hasHeaderRow: true });
add('csv-utf16be-bom', 'bank.csv', Buffer.concat([Buffer.from([0xfe, 0xff]), Buffer.from(le).swap16()]), { hasHeaderRow: true });
add('csv-explicit-utf16le', 'bank.csv', le, { hasHeaderRow: true, encoding: 'utf-16le' });
const legacy = Buffer.from(csv.replace('Café', 'Café €').replace('€', '\x80'), 'latin1');
add('csv-windows1252', 'bank.csv', legacy, { hasHeaderRow: true, encoding: 'windows-1252' });
add('csv-latin1-label', 'bank.csv', legacy, { hasHeaderRow: true, encoding: 'iso-8859-1' });
add('csv-explicit-encoding-wins', 'bank.csv', Buffer.concat([Buffer.from([0xff, 0xfe]), le]), { hasHeaderRow: true, encoding: 'windows-1252' });
for (const ext of ['ofx', 'qfx', 'qif', 'xml', 'txt']) add(`malformed-${ext}`, `bad.${ext}`, 'this is not a financial file');

const directory = await mkdtemp(`${tmpdir()}/transaction-parser-oracle-`);
try {
  const engine = await api.init({ dataDir: directory });
  const observations = [];
  for (const item of cases) {
    const path = `${directory}/${item.filename}`;
    await writeFile(path, item.bytes);
    const result = JSON.parse(JSON.stringify(await engine.send('transactions-parse-file', { filepath: path, options: item.options })));
    // Error messages and first-line diagnostics are behavior. Runtime stack paths
    // and line offsets are not; exclude only those from reproducibility checks.
    for (const error of result.errors) if (error.internal) error.internal = error.internal.split('\n')[0];
    observations.push({ id: item.id, filename: item.filename, source_file: item.source, sha256: hash(item.bytes), bytes_base64: item.bytes.toString('base64'), options: item.options, result });
  }
  const sourcePaths = ['parse-file.ts', 'ofx2json.ts', 'qif2json.ts', 'xmlcamt2json.ts'].map(name => `packages/loot-core/src/server/transactions/import/${name}`);
  const output = {
    oracle: '@actual-app/api@26.10.0', source_commit: '2bebdbaae0b701120a8cad9d56aafd64fa04c577',
    source_files: Object.fromEntries(await Promise.all(sourcePaths.map(async path => [path, hash(await readFile(new URL(path, root)))]))),
    diagnostic_normalization: 'Only error.internal stack frames after its first line are omitted.', cases: observations,
  };
  assert.equal(new Set(observations.map(item => item.id)).size, observations.length);
  if (process.argv.includes('--check')) assert.deepEqual(output, JSON.parse(await readFile(fixture, 'utf8')));
  else if(process.argv.includes('--extend')) {
    const previous=JSON.parse(await readFile(fixture,'utf8'));
    for(const prior of previous.cases) assert.deepEqual(output.cases.find(item=>item.id===prior.id),prior,`Existing observation changed: ${prior.id}`);
    assert.deepEqual({...output,cases:previous.cases},previous,'Provenance must remain unchanged');
    await writeFile(fixture, `${JSON.stringify(output, null, 2)}\n`);
  } else await writeFile(fixture, `${JSON.stringify(output, null, 2)}\n`, { flag: 'wx' });
  console.log(`PASS ${observations.length} pinned parser observations`);
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}
