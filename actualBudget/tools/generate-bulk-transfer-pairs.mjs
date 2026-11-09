// Execute the desktop's public message boundary with its selected-row batch shape.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
const directory=await mkdtemp(tmpdir()+'/bulk-paired-oracle-');
try {
 const engine=await api.init({dataDir:directory}); let a,b;
 await api.runImport('Bulk pair selection',async()=>{a=await api.createAccount({name:'Cash'},0);b=await api.createAccount({name:'Savings'},0);});
 const payee=(await api.getPayees()).find(p=>p.transfer_acct===b).id;
 const id='13150000-0000-4000-8000-000000000001';
 await api.addTransactions(a,[{id,amount:-100,date:'2026-01-01',payee,notes:'Pair'}],{runTransfers:true});
 const ids=[id,(await api.getTransactions(b))[0].id];
 const selected=(await api.aqlQuery(api.q('transactions').filter({id:{$oneof:ids}}).select('*').options({splits:'grouped'}))).data;
 async function state(){return {cash:(await api.getTransactions(a)).map(t=>t.amount).sort((a,b)=>a-b),savings:(await api.getTransactions(b)).map(t=>t.amount).sort((a,b)=>a-b)};}
 const initial=await state();
 // useTransactionBatchActions:onBatchDuplicate / realizeTempTransactions:
 // fresh ID per selected grouped parent; retained transfer_id, reset flags.
 await engine.send('transactions-batch-update',{added:selected.map(({subtransactions,...t},i)=>({...t,id:`13150000-0000-4000-8000-${String(i+10).padStart(12,'0')}`,cleared:false,reconciled:false}))});
 const duplicated=await state();
 await engine.send('transactions-batch-update',{deleted:selected});
 const deleted=await state();
 const fixture={oracle:'@actual-app/api@26.10.0 desktop transactions-batch-update',source:'packages/desktop-client/src/hooks/useTransactionBatchActions.ts:onBatchDuplicate/onBatchDelete',initial,duplicated,deleted};
 const path=new URL('../tests/fixtures/bulk-transfer-pairs.json',import.meta.url),text=JSON.stringify(fixture,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(await readFile(path,'utf8'),text);else await writeFile(path,text);
 console.log(text);
} finally {await api.shutdown();await rm(directory,{recursive:true,force:true});}
