// Independent pinned public API oracle. No private engine mutation or database writes.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/paired-reconciliation-oracle-');
const steps=[]; let cash,card;
async function rows(){const result=[];for(const a of [cash,card])for(const t of await api.getTransactions(a))result.push({account:a===cash?'cash':'card',amount:t.amount,notes:t.notes??'',date:t.date,cleared:t.cleared,reconciled:t.reconciled});return result.sort((a,b)=>a.account.localeCompare(b.account));}
async function settled(predicate){for(let n=0;n<100;n++){if(await predicate())return;await new Promise(r=>setTimeout(r,20));}assert.ok(await predicate(),'public mutation did not settle');}
async function snap(step){steps.push({step,rows:await rows()});}
try{
 await api.init({dataDir:directory});await api.runImport('Paired reconciliation',async()=>{cash=await api.createAccount({name:'Cash'},0);card=await api.createAccount({name:'Card'},0);});
 const to=(await api.getPayees()).find(p=>p.transfer_acct===card).id;
 const id='15160000-0000-4000-8000-000000000001';
 await api.addTransactions(cash,[{id,amount:-2500,date:'2026-01-01',cleared:true,notes:'Settled transfer',payee:to}],{runTransfers:true});
 const peer=(await api.getTransactions(card))[0].id;
 await api.updateTransaction(peer,{cleared:true,reconciled:true});await api.updateTransaction(id,{reconciled:true});await settled(async()=>(await rows()).every(t=>t.reconciled));await snap('locked');
 await api.updateTransaction(id,{amount:-2700,notes:'Confirmed transfer'});await settled(async()=>(await rows()).every(t=>Math.abs(t.amount)===2700&&t.notes==='Confirmed transfer'));await snap('confirmed');
 await api.updateTransaction(peer,{reconciled:false});await settled(async()=>(await rows()).find(t=>t.account==='card').reconciled===false);await snap('peer-unlocked');
 // The baseline duplicate UI reads AQL grouped rows, copies their payee fields,
 // and resets cleared/reconciled. Capture the actual AQL projection after deletion.
 const merchant=await api.createPayee({name:'Retired merchant'});
 const split='15160000-0000-4000-8000-000000000002';
 await api.addTransactions(cash,[{id:split,amount:-300,date:'2026-01-01',payee:merchant,subtransactions:[{id:'15160000-0000-4000-8000-000000000003',amount:-100,payee:merchant},{id:'15160000-0000-4000-8000-000000000004',amount:-200,payee:merchant}]}]);
 await api.deletePayee(merchant);
 const projected=(await api.aqlQuery(api.q('transactions').filter({id:split}).select('*').options({splits:'grouped'}))).data[0];
 const deletedPayeeProjection={parentRetains:projected.payee===merchant,childRetains:projected.subtransactions.map(t=>t.payee===merchant),parentPayee:projected.payee===merchant?'retired':projected.payee,childPayees:projected.subtransactions.map(t=>t.payee===merchant?'retired':t.payee)};
 const fixture={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',duplicate_source:'packages/desktop-client/src/hooks/useTransactionBatchActions.ts:onBatchDuplicate',steps,deletedPayeeProjection};
 const path=new URL('../tests/fixtures/paired-reconciliation.json',import.meta.url),text=JSON.stringify(fixture,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(await readFile(path,'utf8'),text);else await writeFile(path,text);
 console.log('PASS pinned paired confirmation and removed-payee duplicate projection');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
