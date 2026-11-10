// Genuine manual merge behavior, including split and transfer graphs.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';
const NativeDate=Date,clock='2026-11-09T12:00:00Z';
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const cases=[
  {id:'earlier-kept',a:{date:'2026-11-08'},b:{}},
  {id:'equal-date-second-kept',a:{},b:{}},
  {id:'bank-id-kept',a:{date:'2026-11-08',imported_payee:'Manual import'},b:{imported_id:'bank-1'}},
  {id:'imported-payee-kept',a:{date:'2026-11-08'},b:{imported_payee:'Bank Cafe'}},
  {id:'fill-empty-and-or-status',a:{date:'2026-11-08',notes:'',payee:null,category:null},b:{cleared:true,reconciled:true}},
  {id:'split-reparented',a:{imported_id:'bank-1'},b:{split:true}},
  {id:'both-splits-keeper-children',a:{imported_id:'bank-1',split:true},b:{split:true}},
  {id:'transfer-and-manual-onbudget',a:{transfer:'card'},b:{imported_id:'bank-1'}},
  {id:'transfer-and-manual-offbudget',a:{transfer:'off'},b:{imported_id:'bank-1'}},
  {id:'two-transfers-same-peer',a:{transfer:'card',date:'2026-11-08'},b:{transfer:'card'}},
  {id:'different-amounts-rejected',a:{},b:{amount:-999},reject:true},
  {id:'different-accounts-rejected',a:{},b:{account:'card'},reject:true},
  {id:'different-transfer-peers-rejected',a:{transfer:'card'},b:{transfer:'other'},reject:true},
  {id:'duplicate-selection-rejected',a:{},b:{},duplicate:true,reject:true},
];
const directory=await mkdtemp(tmpdir()+'/transaction-merge-oracle-');
const fixture=new URL('../tests/fixtures/transaction-merge-source.json',import.meta.url);
const check=process.argv.includes('--check'),hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const plain=value=>JSON.parse(JSON.stringify(value));
try{
  await api.init({dataDir:directory});
  const observations=[];
  for(const [caseIndex,item] of cases.entries()){
    const ids={};
    const fixed=n=>`25270000-0000-4000-8000-${String(caseIndex*100+n).padStart(12,'0')}`;
    await api.runImport(item.id,async()=>{
      for(const name of ['cash','card','off','other'])ids[name]=await api.createAccount({name,offbudget:name==='off'},0);
      ids.cafe=await api.createPayee({name:'Cafe'});
      ids.group=await api.createCategoryGroup({name:'Expenses',is_income:false});
      ids.category=await api.createCategory({name:'Food',group_id:ids.group,is_income:false});
    });
    const payees=await api.getPayees();
    for(const [index,label] of ['a','b'].entries()){
      const {split,transfer,account,...fields}=item[label];
      ids[label]=fixed(index+1);
      const transaction={id:ids[label],date:'2026-11-09',amount:-1234,payee:ids.cafe,category:ids.category,notes:label,cleared:false,...fields};
      if(transfer)transaction.payee=payees.find(row=>row.transfer_acct===ids[transfer]).id;
      if(split){
        transaction.category=null;
        transaction.subtransactions=[{id:fixed(10+index*10),amount:-500,category:ids.category,notes:`${label} first`},{id:fixed(11+index*10),amount:-734,category:ids.category,notes:`${label} second`}];
      }
      await api.addTransactions(ids[account??'cash'],[transaction],{runTransfers:true});
    }
    const snapshot=async()=>plain({
      transactions:(await api.aqlQuery(api.q('transactions').options({splits:'all'}).select('*'))).data,
      accounts:await api.getAccounts(),payees:await api.getPayees(),categories:await api.getCategories(),
    });
    const before=await snapshot();
    const archiveName=`transaction-merge-${item.id}-26.10.zip`,archivePath=new URL('../tests/fixtures/'+archiveName,import.meta.url);
    const archive=check?await readFile(archivePath):await api.exportBudget();
    if(!check)await writeFile(archivePath,archive,{flag:'wx'});
    let kept=null,error=null;
    try{kept=await api.mergeTransactions([ids.a,item.duplicate?ids.a:ids.b]);}catch(e){error=e.message;}
    assert.equal(!!error,!!item.reject,item.id);
    const after=await snapshot();
    if(item.reject)assert.deepEqual(after,before,`${item.id}: rejected merge changes nothing`);
    else assert.ok(after.transactions.some(row=>row.id===kept),`${item.id}: keeper still exists`);
    observations.push({id:item.id,input:item,ids,archive:{filename:archiveName,sha256:hash(archive)},before,kept,error,after});
  }
  const sourcePaths=['packages/loot-core/src/server/transactions/merge.ts','packages/loot-core/src/shared/merge.ts'];
  const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,source_files:Object.fromEntries(await Promise.all(sourcePaths.map(async path=>[path,hash(await readFile(new URL('../reference/actual-26.10.0/'+path,import.meta.url)))]))),cases:observations};
  function stable(value){
    return {...value,cases:value.cases.map(record=>{
      const ids=new Map(Object.entries(record.ids).map(([label,id])=>[id,label]));
      for(const row of record.before.payees)ids.set(row.id,`payee:${row.name}:${ids.get(row.transfer_acct)??''}`);
      for(const row of record.before.categories){
        if(!ids.has(row.id))ids.set(row.id,`category:${row.name}`);
        if(row.group_id&&!ids.has(row.group_id))ids.set(row.group_id,`category-group:${row.name}`);
      }
      for(const row of record.before.transactions)if(row.id.startsWith('25270000-')&&row.transfer_id)ids.set(row.transfer_id,`peer-of:${row.id}`);
      function walk(v){
        if(typeof v==='string')return v.startsWith('25270000-')?v:ids.get(v)??v;
        if(Array.isArray(v))return v.map(walk);
        if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,item])=>[k,walk(item)]));
        return v;
      }
      const normalized=walk(record);
      // AQL ties use generated IDs: compare this merge graph as an ID-keyed
      // set, preserving every field and relationship, not incidental row order.
      for(const state of ['before','after'])for(const kind of ['transactions','accounts','payees','categories'])normalized[state][kind].sort((a,b)=>a.id.localeCompare(b.id));
      return normalized;
    })};
  }
  if(check)assert.deepEqual(stable(output),stable(JSON.parse(await readFile(fixture,'utf8'))));
  else await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
  console.log(`PASS ${observations.length} genuine manual merge/graph observations`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
