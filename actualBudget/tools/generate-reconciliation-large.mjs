// Public Actual API export, preserving accepted imported aggregates larger than UI limits.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
const require=createRequire(process.env.ACTUAL_ORACLE_PACKAGE??new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const dir=await mkdtemp(tmpdir()+'/reconciliation-large-oracle-');
const id=n=>'16160000-0000-4000-8000-'+String(n).padStart(12,'0');
const accounts=[];
try {
 await api.init({dataDir:dir});
 await api.runImport('Large reconciled imports',async()=>{
  for(const count of [101,1001]){
   const account=await api.createAccount({name:`${count} allocations`},0),parent=id(count*10000);
   await api.addTransactions(account,[{id:parent,amount:-count,date:'2026-02-28',cleared:true,notes:`Imported ${count} allocations`,subtransactions:Array.from({length:count},(_,i)=>({id:id(count*10000+i+1),amount:-1,notes:`Allocation ${i+1}`}))}]);
   accounts.push({account,parent,count});
  }
 });
 for(const {account,count} of accounts){
  const {data}=await api.aqlQuery(api.q('transactions').filter({account}).options({splits:'all'}).select('*'));
  assert.equal(data.length,count+1);
  assert.ok(data.every(row=>row.cleared));
  const total=(await api.aqlQuery(api.q('transactions').filter({account}).options({splits:'none'}).calculate({$sum:'$amount'}))).data;
  assert.equal(total,-count);
 }
 const base=new URL('../tests/fixtures/',import.meta.url);
 await writeFile(new URL('reconciliation-large.zip',base),await api.exportBudget());
 await writeFile(new URL('reconciliation-large.json',base),JSON.stringify({oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',accounts},null,2)+'\n');
 console.log('PASS public Actual101/1001-child reconciliation import fixture');
}finally{await api.shutdown();await rm(dir,{recursive:true,force:true});}
