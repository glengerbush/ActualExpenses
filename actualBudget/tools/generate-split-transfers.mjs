// Public pinned oracle for the ticket14/15 integration gate.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setImmediate as yieldLoop } from 'node:timers/promises';
async function settled(check){const deadline=performance.now()+5000;while(!await check()){assert.ok(performance.now()<deadline,'Public transfer mutation did not settle');await yieldLoop();}}
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json',import.meta.url))).version,'26.10.0');
const directory=await mkdtemp(join(tmpdir(),'actual-split-transfer-'));const aliases=new Map();const label=id=>id==null?null:aliases.get(id)??id;const alias=(id,name)=>{aliases.set(id,name);return id;};const snapshots=[];let started=false;
const parent=alias('15000000-0000-4000-8000-000000000050','parent'),child=alias('15000000-0000-4000-8000-000000000051','child'),ordinary=alias('15000000-0000-4000-8000-000000000052','ordinary');
async function snapshot(step){const rows=[];for(const a of await api.getAccounts())for(const row of await api.getTransactions(a.id)){rows.push(row,...(row.subtransactions??[]));}for(const row of rows)if(row.transfer_id&&!aliases.has(row.transfer_id))alias(row.transfer_id,`${label(row.id)}-other`);snapshots.push({step,rows:rows.map(t=>({id:label(t.id),account_id:label(t.account),amount:t.amount,date:t.date,notes:t.notes??'',transfer_id:label(t.transfer_id),parent_id:label(t.parent_id),is_parent:!!t.is_parent,category_id:label(t.category),error:t.error})).sort((a,b)=>a.id.localeCompare(b.id))});}
try{await api.init({dataDir:directory});started=true;let cash,card,category;await api.runImport('Disposable split transfer oracle',async()=>{cash=alias(await api.createAccount({name:'Cash',offbudget:false},0),'cash');card=alias(await api.createAccount({name:'Card',offbudget:false},0),'card');const group=await api.createCategoryGroup({name:'Expenses',is_income:false});category=alias(await api.createCategory({name:'Spending',group_id:group}),'category');});
const payee=(await api.getPayees()).find(p=>p.transfer_acct===card).id;
await api.addTransactions(cash,[{id:parent,date:'2026-01-01',amount:-1000,notes:'Split payment',subtransactions:[{id:child,amount:-600,payee,notes:'Transfer allocation'},{id:ordinary,amount:-400,category,notes:'Ordinary allocation'}]}],{runTransfers:true});
await snapshot('split-with-transfer');
if(!process.argv.includes('--check'))await writeFile(new URL('../tests/fixtures/split-transfers-26.10.0.zip',import.meta.url),await api.exportBudget());
const other=(await api.getTransactions(card))[0].id;await api.updateTransaction(other,{amount:700,notes:'Changed from other account'});await settled(async()=> (await api.getTransactions(cash))[0]?.subtransactions?.find(t=>t.id===child)?.amount===-700);await snapshot('edit-other-leg-amount');
await api.updateTransaction(other,{date:'2026-02-01'});await snapshot('independent-other-date');
await api.deleteTransaction(other);await settled(async()=> !(await api.getTransactions(cash))[0]?.subtransactions?.find(t=>t.id===child)?.transfer_id);await snapshot('delete-other-detaches-child');
const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',snapshots};const path=new URL('../tests/fixtures/split-transfers.json',import.meta.url);if(process.argv.includes('--check'))assert.deepEqual(result,JSON.parse(await readFile(path,'utf8')));else await writeFile(path,JSON.stringify(result,null,2)+'\n');console.log('PASS pinned split-transfer integration oracle');
}finally{if(started)await api.shutdown();await rm(directory,{recursive:true,force:true});}
