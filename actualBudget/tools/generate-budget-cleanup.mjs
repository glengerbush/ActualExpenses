// Source observations: only the real pinned engine mutates or calculates budgets.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm,mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';
const NativeDate=Date;
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:['2026-11-09T12:00:00Z']));}static now(){return NativeDate.parse('2026-11-09T12:00:00Z');}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const matrix=[
 {name:'global-weighted',notes:{A:'#cleanup source',B:'#cleanup sink',C:'#cleanup sink 2'},budget:{A:10000},spend:{A:4000},income:10000},
 {name:'global-unallocated-and-pennies',notes:{A:'#cleanup source',B:'#cleanup sink',C:'#cleanup sink 2'},budget:{A:3},income:5},
 {name:'source-without-sink',notes:{A:'#cleanup source'},budget:{A:10000},spend:{A:2500},income:10000},
 {name:'sink-without-source',notes:{B:'#cleanup sink'},income:12345},
 {name:'overspending-no-directives',notes:{},spend:{A:1200,B:300},income:1000},
 {name:'global-overspending-first',notes:{A:'#cleanup source',C:'#cleanup sink'},budget:{A:10000},spend:{A:1000,B:2500},income:10000},
 {name:'rollover-overspending',notes:{A:'#cleanup source',C:'#cleanup sink'},budget:{A:10000},spend:{B:2500},carry:['B'],income:10000},
 {name:'negative-source',notes:{A:'#cleanup source',B:'#cleanup sink'},spend:{A:1500},income:1000},
 {name:'negative-global-available',notes:{B:'#cleanup sink',C:'#cleanup sink 2'},budget:{A:1500},income:1000},
 {name:'buffer-source-and-sink',notes:{A:'#cleanup source\n#cleanup sink'},budget:{A:10000},spend:{B:2500},income:10000},
 {name:'pool-weighted-isolated',notes:{A:'#cleanup Pool source',B:'#cleanup Pool sink',C:'#cleanup Pool sink 2'},budget:{A:9000},spend:{A:3000},income:20000},
 {name:'pool-rounding-pennies',notes:{A:'#cleanup Pool source',B:'#cleanup Pool sink',C:'#cleanup Pool sink'},budget:{A:5},income:5},
 {name:'pool-overspend-before-sink',notes:{A:'#cleanup Pool source',B:'#cleanup Pool',C:'#cleanup Pool sink'},budget:{A:9000},spend:{A:3000,B:2000},income:9000},
 {name:'pool-partial-overspend',notes:{A:'#cleanup Pool source',B:'#cleanup Pool',C:'#cleanup Pool sink'},budget:{A:3000},spend:{B:5000},income:3000},
 {name:'pool-rollover',notes:{A:'#cleanup Pool source',B:'#cleanup Pool',C:'#cleanup Pool sink'},budget:{A:9000},spend:{B:2000},carry:['B'],income:9000},
 {name:'pool-missing-sink',notes:{A:'#cleanup Pool source'},budget:{A:9000},income:9000},
 {name:'pool-negative-source',notes:{A:'#cleanup Pool source',B:'#cleanup Pool sink'},spend:{A:1500},income:1000},
 {name:'pool-case-insensitive',notes:{A:'#cleanup Pool source',B:'#cleanup pOoL sink',C:'#cleanup POOL sink 2'},budget:{A:9000},income:9000},
 {name:'pool-then-global',notes:{A:'#cleanup Pool source',B:'#cleanup Pool sink\n#cleanup source',C:'#cleanup sink'},budget:{A:9000},income:12000},
 {name:'grammar-and-original-notes',notes:{A:'Electricity\n#template 100\n#cleanup source',B:'#CLEANUP sink\n#cleanup sink 0',C:'   #cleanup sink 2   '},budget:{A:9000},income:9000},
 {name:'ui-source-ignores-stale-notes',notes:{A:'#cleanup sink',B:'#cleanup source',C:'#cleanup sink 99'},budget:{A:9000},income:9000,ui:{A:[{role:'source',groupId:null}],B:[{role:'sink',groupId:null,weight:1}],C:[{role:'sink',groupId:null,weight:2}]}},
 {name:'ui-pool-ignores-stale-notes',notes:{A:'#cleanup source',B:'#cleanup sink',C:'#cleanup sink 99'},budget:{A:9000},income:12000,ui:{A:[{role:'source',groupId:'Pool'}],B:[{role:'sink',groupId:'Pool',weight:1}],C:[{role:'sink',groupId:'Pool',weight:2}]}},
];
const directory=await mkdtemp(tmpdir()+'/budget-cleanup-'),check=process.argv.includes('--check');
const root=new URL('../tests/fixtures/budget-cleanup/',import.meta.url);
const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',cases:[]},archives=[];
try{
 const engine=await api.init({dataDir:directory}),send=(name,body)=>engine.send(name,body);
 for(const cfg of matrix)for(const method of ['envelope','tracking']){
  const ids={},name=method+'-'+cfg.name,month='2026-11';
  await api.runImport(name,async()=>{
   await send('preferences/save',{id:'budgetType',value:method});
   ids.Checking=await api.createAccount({name:'Checking'},0);
   ids.Plans=await api.createCategoryGroup({name:'Plans',is_income:false});
   for(const key of ['A','B','C'])ids[key]=await api.createCategory({name:key,group_id:ids.Plans,is_income:false});
   ids.Salary=await api.createCategory({name:'Salary',group_id:(await api.getCategoryGroups()).find(g=>g.is_income).id,is_income:true});
   const rows=[{amount:cfg.income,category:ids.Salary},...Object.entries(cfg.spend??{}).map(([key,value])=>({amount:-value,category:ids[key]}))];
   await api.addTransactions(ids.Checking,rows.map((row,index)=>({...row,id:`24240000-0000-4000-8000-${String(index+1).padStart(12,'0')}`,date:'2026-11-03',notes:`Seed ${index}`,cleared:true})));
   for(const [key,amount] of Object.entries(cfg.budget??{}))await api.setBudgetAmount(month,ids[key],amount);
   for(const key of cfg.carry??[])await send('budget/set-carryover',{startMonth:month,category:ids[key],flag:true});
   for(const [key,note] of Object.entries(cfg.notes))await send('notes-save',{id:ids[key],note});
  });
  if(Object.values(cfg.notes).some(note=>/pool/i.test(note))||cfg.ui&&Object.values(cfg.ui).flat().some(row=>row.groupId))ids.Pool=(await send('budget/create-cleanup-group',{name:'Pool'})).id;
  if(cfg.ui)await send('budget/set-category-automations',{source:'ui',categoriesWithTemplates:Object.entries(cfg.ui).map(([key,cleanup])=>({id:ids[key],templates:[],cleanup:cleanup.map(row=>({...row,groupId:row.groupId?ids[row.groupId]:null}))}))});
  await send('budget/store-note-cleanups');
  const label=id=>Object.entries(ids).find(([,value])=>value===id)?.[0]??id;
  const normalize=value=>Array.isArray(value)?value.map(normalize):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[label(k),normalize(v)])):typeof value==='string'?label(value):value;
  const definitions={};
  const rows=(await api.runQuery(api.q('categories').select('*'))).data;
  for(const key of ['A','B','C']){
   const row=rows.find(row=>row.id===ids[key]);
   definitions[key]={cleanup:normalize(row.cleanup_def?JSON.parse(row.cleanup_def):null),template_settings:row.template_settings??null};
  }
  const read=async()=>{
   const categories={};
   for(const key of ['A','B','C','Salary']){
    categories[key]={};
    for(const field of ['budget','goal','long-goal','leftover']){
     const cell=await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids[key]});
     assert.ok(Object.hasOwn(cell,'value'),name+' missing '+field);categories[key][field]=cell.value;
    }
   }
   return {categories,available:(await send('get-cell',{sheetName:'budget202611',name:method==='tracking'?'total-saved':'to-budget'})).value};
  };
  const ledger=async()=>(await api.getTransactions(ids.Checking)).map(row=>({id:row.id,date:row.date,amount:row.amount,category:label(row.category),notes:row.notes??'',cleared:!!row.cleared,reconciled:!!row.reconciled})).sort((a,b)=>a.id.localeCompare(b.id));
  const seed=await read(),sourceLedger=await ledger();
  const filename=name+'.zip',path=new URL(filename,root),generated=await api.exportBudget(),bytes=check?await readFile(path):generated;
  if(!check)archives.push([path,bytes]);
  const steps=[];
  for(let attempt=0;attempt<2;attempt++){
   let notification;
   try{notification=await send('budget/cleanup-goal-template',{month});}catch(error){notification={error:error.message};}
   steps.push({notification,result:await read()});
   assert.deepEqual(await ledger(),sourceLedger,name+' changed ledger');
   for(const [key,note] of Object.entries(cfg.notes))assert.equal((await send('notes-get',{id:ids[key]})).note,note);
  }
  output.cases.push({name,method,month,configuration:cfg,ids,definitions,seed,ledger:sourceLedger,steps,archive:{filename,sha256:createHash('sha256').update(bytes).digest('hex')}});
 }
 const file=new URL('../tests/fixtures/budget-cleanup.json',import.meta.url);
 if(check){const original=JSON.parse(await readFile(file,'utf8'));const stable=v=>({...v,cases:v.cases.map(({ids,...record})=>record)});assert.deepEqual(stable(output),stable(original));}
 else{await mkdir(root,{recursive:true});for(const [path,bytes] of archives)await writeFile(path,bytes,{flag:'wx'});await writeFile(file,JSON.stringify(output,null,2)+'\n',{flag:'wx'});}
 console.log(`PASS ${output.cases.length} independent cleanup cases, two explicit executions each; source notes and ledgers unchanged`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
