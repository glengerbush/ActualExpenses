// Public Actual 26.10 API: grouped protection and parent-only statement totals.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
const require = createRequire(process.env.ACTUAL_ORACLE_PACKAGE ?? new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/, 'package.json'), 'utf8')).version, '26.10.0');
const dir = await mkdtemp(tmpdir() + '/reconciliation-splits-oracle-');
const id = n => '16140000-0000-4000-8000-' + String(n).padStart(12, '0');
const steps = [];
let account;
async function rows() { return (await api.aqlQuery(api.q('transactions').filter({account}).options({splits:'all'}).select('*'))).data; }
async function waitFor(predicate) { for(let n=0;n<100;n++){if(predicate(await rows())) return;await new Promise(r=>setTimeout(r,20));} assert.ok(predicate(await rows()),'Public update did not settle'); }
async function snapshot(step) {
 const data = await rows();
 const entries = data.map(t=>({amount:t.amount,notes:t.notes??'',cleared:t.cleared,reconciled:t.reconciled,parent:!!t.is_parent,child:!!t.parent_id})).sort((a,b)=>a.amount-b.amount);
 const cleared = (await api.aqlQuery(api.q('transactions').filter({account,cleared:true}).options({splits:'none'}).calculate({$sum:'$amount'}))).data;
 steps.push({step,entries,cleared});
}
try {
 await api.init({dataDir:dir});
 await api.runImport('Split reconciliation oracle',async()=>{account=await api.createAccount({name:'Checking'},0);});
 await api.addTransactions(account,[{id:id(1),amount:-300,date:'2026-02-28',cleared:true,notes:'Settled split',subtransactions:[{id:id(2),amount:-100,notes:'First allocation'},{id:id(3),amount:-200,notes:'Second allocation'}]}]);
 await snapshot('initial');
 await api.updateTransaction(id(1),{reconciled:true});
 await waitFor(rows=>rows.every(t=>t.reconciled));
 await snapshot('locked');
 await api.updateTransaction(id(2),{amount:-100,notes:'Confirmed allocation'});
 await waitFor(rows=>rows.find(t=>t.id===id(2))?.notes==='Confirmed allocation');
 await snapshot('confirmed');
 await api.updateTransaction(id(1),{reconciled:false});
 await waitFor(rows=>rows.every(t=>!t.reconciled));
 await snapshot('unlocked');
 const fixture={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',steps};
 const target=new URL('../tests/fixtures/reconciliation-splits.json',import.meta.url),text=JSON.stringify(fixture,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(await readFile(target,'utf8'),text);else await writeFile(target,text);
 console.log('PASS public split reconciliation oracle');
} finally {await api.shutdown();await rm(dir,{recursive:true,force:true});}
