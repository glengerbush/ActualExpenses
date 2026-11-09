// Disposable public-API oracle, independent of replacement implementation.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const require = createRequire(process.env.ACTUAL_ORACLE_PACKAGE ?? new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('node_modules/@actual-app/api/package.json', new URL(process.env.ACTUAL_ORACLE_PACKAGE ? 'file://' + process.env.ACTUAL_ORACLE_PACKAGE : '../../actualExpenses/tools/compatibility/package.json', import.meta.url)))).version, '26.10.0');
const directory = await mkdtemp(join(tmpdir(), 'payees-tags-oracle-'));
const fixture = new URL('../tests/fixtures/payees-tags.json', import.meta.url);
const aliases = new Map();
const alias = (id, name) => (aliases.set(id, name), id);
const label = id => id == null ? null : aliases.get(id) ?? id;
let account, first, second, transfer;
const snapshots = [];
const ts=createRequire(new URL('../web/package.json',import.meta.url))('typescript');
const tagSource=await readFile(new URL('../reference/actual-26.10.0/packages/loot-core/src/shared/tags.ts',import.meta.url),'utf8');
const {renameTagInNotes}=await import('data:text/javascript;base64,'+Buffer.from(ts.transpileModule(tagSource,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText).toString('base64'));
const tag_renames=[
 {notes:'#Food ##Food #Foodie #food #Food#trip',old:'Food',next:'Meals'},
 {notes:'#a.b #aXb ##a.b',old:'a.b',next:'x+y'},
 {notes:'#Food\ufeff#Food\u0085tail',old:'Food',next:'Meals'},
 {notes:'##Food ###Food #Food#Food',old:'Food',next:'Meals'},
].map(test=>({...test,expected:renameTagInNotes(test.notes,test.old,test.next)}));
async function snapshot(step) {
  snapshots.push({ step,
    payees: (await api.getPayees()).map(p => ({ id: label(p.id), name: p.name, transfer: label(p.transfer_acct), favorite: !!p.favorite })).sort((a,b)=>a.id.localeCompare(b.id)),
    transactions: (await api.getTransactions(account)).map(t => ({id: label(t.id), payee: label(t.payee), imported_payee: t.imported_payee ?? null, amount:t.amount, notes:t.notes})).sort((a,b)=>a.id.localeCompare(b.id)),
    tags: (await api.getTags()).map(t=>({tag:t.tag, color:t.color ?? null, description:t.description ?? null, hidden:!!t.hidden})).sort((a,b)=>a.tag.localeCompare(b.tag))
  });
}
try {
 await api.init({dataDir:directory});
 await api.runImport('Payees and tags oracle', async()=>{
  account=alias(await api.createAccount({name:'Spending'},0),'account');
  first=alias(await api.createPayee({name:'CORNER SHOP 001'}),'first');
  second=alias(await api.createPayee({name:'Corner shop'}),'second');
  for (const p of await api.getPayees()) if(p.transfer_acct) transfer=alias(p.id,'transfer');
  await api.addTransactions(account,[{id:alias('11000000-0000-4000-8000-000000000001','tx1'),date:'2026-01-02',amount:-1250,payee:first,imported_payee:'CARD CORNER 001',notes:'#Food #Foodie ##Food #food #Food#trip'}, {id:alias('11000000-0000-4000-8000-000000000002','tx2'),date:'2026-01-03',amount:-275,payee:second,notes:'#Food lunch'}]);
 });
 await snapshot('initial');
 await api.updatePayee(first,{name:'Corner Shop',favorite:true});
 await snapshot('renamed');
 await api.mergePayees(first,[second]);
 await snapshot('merged');
 await api.deletePayee(first);
 await api.deletePayee(transfer);
 await snapshot('removed');
 const tag=await api.createTag({tag:'Food',color:'#ff0000',description:'Meals'});
 await api.updateTag(tag,{hidden:true});
 await snapshot('tag-hidden');
 if(!process.argv.includes('--check')) {await writeFile(new URL('../tests/fixtures/payees-tags-import.zip',import.meta.url),await api.exportBudget());await writeFile(new URL('../tests/fixtures/payees-tags-import.json',import.meta.url),JSON.stringify({account,first,second,transfer,tag},null,2)+'\n');}
 await api.deleteTag(tag);
 await snapshot('tag-deleted-notes-retained');
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',tag_renames,snapshots};
 if(process.argv.includes('--check')) assert.deepEqual(result,JSON.parse(await readFile(fixture,'utf8'))); else await writeFile(fixture,JSON.stringify(result,null,2)+'\n');
 console.log('PASS public payee/tag oracle');
} finally {await api.shutdown();await rm(directory,{recursive:true,force:true});}
