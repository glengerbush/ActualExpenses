// Independent pinned engine oracle. Explicit source sort_order models equal-ms and
// backward-clock captures without editing a SavedState or SQLite schema/version.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json',import.meta.url),'utf8')).version,'26.10.0');
const root=await mkdtemp(join(tmpdir(),'android-order-'));
const id=n=>`20000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const snapshots=[];const capture=async step=>{const rows=await api.getTransactions(account);snapshots.push({step,ids:rows.map(t=>t.id),amounts:rows.map(t=>t.amount)});};
let account;
try{
 const oracle=process.env.ACTUAL_ORDER_ORACLE ? JSON.parse(await readFile(process.env.ACTUAL_ORDER_ORACLE,'utf8')) : null;
 await api.init({dataDir:root,...(oracle ? {serverURL:oracle.directServerUrl,password:oracle.password} : {})});
 await api.runImport('Android ordering oracle',async()=>{
  account=await api.createAccount({name:'Ordering',offbudget:false},0);
  await api.addTransactions(account,[{id:id(1),date:'2026-01-01',amount:-100,sort_order:1000,notes:'source lower'},{id:id(3),date:'2026-01-01',amount:-300,sort_order:3000,notes:'source upper'}]);
 });
 if(oracle) await api.sync();
 await capture('source');
 if(!process.argv.includes('--check'))await writeFile(new URL('../tests/fixtures/android-order-source.zip',import.meta.url),await api.exportBudget());
 // A post-cutover native anchor has no retained upstream sort_order; model its
 // known relative position above existing source rows, preserving that anchor.
 await api.addTransactions(account,[{id:id(4),date:'2026-01-01',amount:-400,sort_order:4000,notes:'post-cutover anchor'}]);await capture('anchor');
 for(const [n,order] of [[2,2000],[5,2000],[6,500],[7,3500],[9,1500]]){
  await api.addTransactions(account,[{id:id(n),date:'2026-01-01',amount:-n*100,sort_order:order,notes:`pending ${n}`}]);await capture(`pending-${n}`);
 }
 // Backdated additions must not displace the newest ten when delivered late.
 await api.addTransactions(account,Array.from({length:12},(_,i)=>({id:id(20+i),date:'2026-01-02',amount:-1,sort_order:5000+i,notes:'recent'})));
 await api.addTransactions(account,[{id:id(8),date:'2026-01-01',amount:-800,sort_order:2500,notes:'late old purchase'}]);await capture('below-latest-ten');
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',account,snapshots};
 // Account IDs are generated; expected financial ordering is stable across runs.
 const fixture=new URL('../tests/fixtures/android-order.json',import.meta.url);
 if(process.argv.includes('--check')){const expected=JSON.parse(await readFile(fixture,'utf8'));assert.deepEqual(result.snapshots,expected.snapshots);}else await writeFile(fixture,JSON.stringify(result,null,2)+'\n');
 console.log(`PASS ${snapshots.length} independent Android ordering snapshots`);
}finally{await api.shutdown();await rm(root,{recursive:true,force:true});}
