// Independent baseline: execute the published pinned importBudget public API.
// No replacement converter or copied import formulas are loaded.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,copyFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';const OriginalDate=Date,clock='2026-11-09T12:00:00.000Z';
globalThis.Date=class extends OriginalDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return OriginalDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'))).version,'26.10.0');
const sourceCommit='2bebdbaae0b701120a8cad9d56aafd64fa04c577',check=process.argv.includes('--check'),hash=bytes=>createHash('sha256').update(bytes).digest('hex'),directory=await mkdtemp(tmpdir()+'/ynab-source-');
const destination=new URL('../tests/fixtures/ynab/',import.meta.url),recordPath=new URL('../tests/fixtures/ynab-source.json',import.meta.url);
const inputs=[{id:'ynab4-demo',type:'ynab4',file:'ynab4-demo-budget.zip',months:['2021-11','2022-01','2022-10','2022-12','2023-01','2024-01']},{id:'nynab-demo',type:'ynab5',file:'ynab5-demo-budget.json',months:['2023-08','2023-09']}];
function canonical(record){
 const ids=new Map();let sequence=0;
 for(const a of record.accounts)ids.set(a.id,'account:'+a.name);
 for(const g of record.groups)ids.set(g.id,'group:'+g.name);
 for(const c of record.categories)ids.set(c.id,'category:'+(ids.get(c.group_id)??'none')+'/'+c.name);
 for(const p of record.payees)ids.set(p.id,p.transfer_acct?'transfer:'+ids.get(p.transfer_acct):'payee:'+p.name);
 for(const entry of record.ledgers)for(const [i,row]of entry.transactions.entries()){
  ids.set(row.id,`${ids.get(entry.account_id)}:transaction:${i}`);
  for(const [j,child]of (row.subtransactions??[]).entries())ids.set(child.id,`${ids.get(row.id)}:child:${j}`);
 }
 const walk=value=>Array.isArray(value)?value.map(walk):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).filter(([k])=>k!=='sort_order').map(([k,v])=>[ids.get(k)??k,walk(v)])):typeof value==='string'&&/^[a-f0-9]{8}-[a-f0-9-]{27}$/i.test(value)?(ids.get(value)??(ids.set(value,'generated:'+(++sequence)),ids.get(value))):value;
 return JSON.parse(JSON.stringify(walk(record)));
}
try{
 const engine=await api.init({dataDir:directory});
 const records=[];
 for(const item of inputs){
  const sourcePath=new URL('../reference/actual-26.10.0/packages/desktop-client/e2e/data/'+item.file,import.meta.url),bytes=await readFile(sourcePath),inputName=item.id+(item.type==='ynab4'?'.zip':'.json');
  if(!check){try{await copyFile(sourcePath,new URL(inputName,destination),1);}catch(e){if(e.code!=="EEXIST")throw e;}}
  assert.equal(hash(await readFile(new URL(inputName,destination))),hash(bytes));
  const imported=await api.importBudget(bytes,{type:item.type,filename:item.file});
  const accounts=await api.getAccounts(),groups=await api.getCategoryGroups(),categories=await api.getCategories(),payees=await api.getPayees(),ledgers=[];
  for(const account of accounts)ledgers.push({account_id:account.id,balance:await api.getAccountBalance(account.id),transactions:await api.getTransactions(account.id,'1900-01-01','2099-12-31')});
  const months=[];for(const month of item.months){try{months.push({month,data:await api.getBudgetMonth(month)});}catch(error){months.push({month,error:JSON.parse(JSON.stringify(error))});}}
  const schedules=await api.getSchedules(),tags=await engine.send('tags-get');
  const snapshot={accounts,groups,categories,payees,ledgers,months,schedules,tags};
  assert.ok(accounts.length&&categories.length&&ledgers.some(l=>l.transactions.length));
  const actualName=item.id+'-actual-26.10.zip',actual=check?await readFile(new URL(actualName,destination)):await api.exportBudget();
  if(!check)await writeFile(new URL(actualName,destination),actual,{flag:'wx'});
  records.push({id:item.id,type:item.type,filename:item.file,input:{filename:inputName,sha256:hash(bytes),bytes:bytes.length},actual:{filename:actualName,sha256:hash(actual)},snapshot:canonical(snapshot)});
  console.log('PASS '+item.id+' real import, historical budget cells, ledger and exported Actual archive');
 }
 const source_files=[];for(const name of ['ynab4.ts','ynab4-types.ts','ynab5.ts','ynab5-types.ts']){const path='packages/loot-core/src/server/importers/'+name;source_files.push({path,sha256:hash(await readFile(new URL('../reference/actual-26.10.0/'+path,import.meta.url)))});}
 const output={source_files,oracle:'@actual-app/api@26.10.0',source_commit:sourceCommit,clock,normalization:'Generated identities are labeled by public reference names and ledger/child order; sort_order numbers alone are omitted, preserving array order, multiplicity and every graph reference.',cases:records};
 if(check)assert.deepEqual(output,JSON.parse(await readFile(recordPath)));else await writeFile(recordPath,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
 console.log('PASS YNAB source imports');
}finally{await api.shutdown().catch(()=>{});await rm(directory,{recursive:true,force:true});}
