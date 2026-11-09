// Public Actual API operations on disposable data. UI orchestration is pinned source evidence.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
const require=createRequire(process.env.ACTUAL_ORACLE_PACKAGE??new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('node_modules/@actual-app/api/package.json',new URL(process.env.ACTUAL_ORACLE_PACKAGE?'file://'+process.env.ACTUAL_ORACLE_PACKAGE:'../../actualExpenses/tools/compatibility/package.json',import.meta.url)))).version,'26.10.0');
const dir=await mkdtemp(join(tmpdir(),'reconciliation-oracle-'));
const fixture=new URL('../tests/fixtures/reconciliation.json',import.meta.url);
const snapshots=[];
let debt,asset,debtAdjustment;
async function snapshot(step,account){const rows=(await api.getTransactions(account)).map(t=>({amount:t.amount,date:t.date,cleared:t.cleared,reconciled:t.reconciled,notes:t.notes??''})).sort((a,b)=>a.amount-b.amount);snapshots.push({step,transactions:rows,total:rows.reduce((n,t)=>n+t.amount,0),cleared:rows.filter(t=>t.cleared).reduce((n,t)=>n+t.amount,0),uncleared:rows.filter(t=>!t.cleared).reduce((n,t)=>n+t.amount,0)});}
try{
 await api.init({dataDir:dir});
 await api.runImport('Reconciliation oracle',async()=>{
  debt=await api.createAccount({name:'Credit card'},0);asset=await api.createAccount({name:'House',offbudget:true},0);
  await api.addTransactions(debt,[{id:'16000000-0000-4000-8000-000000000001',date:'2026-01-01',amount:-10000,cleared:true},{id:'16000000-0000-4000-8000-000000000002',date:'2026-02-28',amount:-300,cleared:false}]);
  await api.addTransactions(asset,[{id:'16000000-0000-4000-8000-000000000003',date:'2026-01-01',amount:23110000,cleared:true}]);
 });
 await snapshot('debt-initial',debt);
 const cleared=(await api.aqlQuery(api.q('transactions').filter({account:debt,cleared:true}).calculate({$sum:'$amount'}))).data;
 debtAdjustment=(await api.addTransactions(debt,[{id:'16000000-0000-4000-8000-000000000004',date:'2026-02-28',amount:-12500-cleared,cleared:true,notes:'Reconciliation balance adjustment'}]))[0];
 debtAdjustment=(await api.getTransactions(debt)).find(t=>t.amount===-2500).id;
 await snapshot('debt-adjusted',debt);
 for(const t of await api.getTransactions(debt))if(t.cleared)await api.updateTransaction(t.id,{reconciled:true});
 await api.updateAccount(debt,{last_reconciled:'1772236800000'});
 await snapshot('debt-locked',debt);
 await api.updateTransaction(debtAdjustment,{notes:'Confirmed'});
 for(let n=0;n<50;n++){if((await api.getTransactions(debt)).find(t=>t.id===debtAdjustment)?.notes==='Confirmed')break;await new Promise(r=>setTimeout(r,20));}
 assert.equal((await api.getTransactions(debt)).find(t=>t.id===debtAdjustment)?.notes,'Confirmed');
 await snapshot('debt-confirmed-edit',debt);
 await api.updateTransaction(debtAdjustment,{reconciled:false});
 for(let n=0;n<50;n++){if((await api.getTransactions(debt)).find(t=>t.id===debtAdjustment)?.reconciled===false)break;await new Promise(r=>setTimeout(r,20));}
 assert.equal((await api.getTransactions(debt)).find(t=>t.id===debtAdjustment)?.reconciled,false);
 await snapshot('debt-unlocked',debt);
 await api.addTransactions(asset,[{id:'16000000-0000-4000-8000-000000000005',date:'2026-02-28',amount:350000,cleared:true,notes:'Reconciliation balance adjustment'}]);
 for(const t of await api.getTransactions(asset))if(t.cleared)await api.updateTransaction(t.id,{reconciled:true});
 await snapshot('asset-revalued',asset);
 const source=await readFile(new URL('../reference/actual-26.10.0/packages/desktop-client/src/accounts/reconciliation.ts',import.meta.url));
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',orchestration_sha256:createHash('sha256').update(source).digest('hex'),snapshots};
 if(process.argv.includes('--check'))assert.deepEqual(result,JSON.parse(await readFile(fixture,'utf8')));
 else {await writeFile(fixture,JSON.stringify(result,null,2)+'\n');await writeFile(new URL('../tests/fixtures/reconciliation-import.zip',import.meta.url),await api.exportBudget());await writeFile(new URL('../tests/fixtures/reconciliation-import.json',import.meta.url),JSON.stringify({debt,asset,last_reconciled:'1772236800000'},null,2)+'\n');}
 console.log('PASS public reconciliation oracle');
}finally{await api.shutdown();await rm(dir,{recursive:true,force:true});}
