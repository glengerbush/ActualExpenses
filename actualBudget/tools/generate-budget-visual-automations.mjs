// Independent UI-definition and rendered-note observations from Actual26.10.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm,mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';const NativeDate=Date;
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:['2026-11-09T12:00:00Z']));}static now(){return NativeDate.parse('2026-11-09T12:00:00Z');}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const template=(type,fields={})=>({type,directive:'template',priority:0,...fields});
const limit=fields=>template('limit',{priority:null,amount:100,period:'monthly',hold:false,...fields});
const fixed=(amount,fields={})=>template('periodic',{amount,period:{period:'month',amount:1},starting:'2026-11-01',...fields});
const matrix=[
 {name:'fixed-with-standalone-cap',templates:[limit(),fixed(40,{description:'Monthly reserve'})]},
 {name:'refill-with-standalone-cap',templates:[limit(),template('refill')]},
 {name:'held-weekly-cap',templates:[limit({amount:25,period:'weekly',start:'2026-11-02',hold:true}),fixed(200)]},
 {name:'fixed-plus-long-goal',templates:[fixed(24.99,{description:'First service'}),fixed(9.99,{description:'Second service'}),{type:'goal',directive:'goal',amount:1000}]},
 {name:'save-target',templates:[template('by',{amount:1000,month:'2027-02',annual:false,repeat:3})]},
 {name:'spend-target',templates:[template('spend',{amount:1000,month:'2027-02',from:'2026-09'})]},
 {name:'history-adjusted',templates:[template('average',{numMonths:3,adjustment:12.5,adjustmentType:'percent'}),template('copy',{lookBack:1})]},
 {name:'income-category-id',templates:[template('percentage',{category:'Salary',percent:12.5,previous:false})],incomeId:true},
 {name:'schedule-id-and-adjustment',templates:[template('schedule',{scheduleId:'Rent',name:'Rent',full:true,adjustment:10,adjustmentType:'percent'})],schedule:true},
 {name:'weighted-remainder-cap',templates:[limit({amount:50}),template('remainder',{weight:2,priority:null})]},
 {name:'goal-only-cap',templates:[limit({amount:50}),{type:'goal',directive:'goal',amount:1000}],current:{A:2222}},
 {name:'orphan-refill',templates:[template('refill')],current:{A:2222}},
];
const selected=process.argv.find(arg=>arg.startsWith('--case='))?.slice(7),append=process.argv.includes('--append');
if(selected)assert.ok(matrix.some(c=>c.name===selected),'Unknown source case');
const directory=await mkdtemp(tmpdir()+'/budget-visual-'),check=process.argv.includes('--check'),root=new URL('../tests/fixtures/budget-visual-automations/',import.meta.url),archives=[];
const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',cases:[]};
try{
 const engine=await api.init({dataDir:directory}),send=(name,body)=>engine.send(name,body);
 for(const cfg of matrix.filter(c=>!selected||c.name===selected))for(const method of ['envelope','tracking']){
  let uiResult;
  for(const mode of ['ui','rendered-notes']){
   const name=method+'-'+cfg.name+'-'+mode,ids={},month='2026-11';
   await api.runImport(name,async()=>{
    await send('preferences/save',{id:'budgetType',value:method});
    ids.Checking=await api.createAccount({name:'Checking'},0);ids.Plans=await api.createCategoryGroup({name:'Plans',is_income:false});
    for(const key of ['A','B','C'])ids[key]=await api.createCategory({name:key,group_id:ids.Plans,is_income:false});
    ids.Salary=await api.createCategory({name:'Salary',group_id:(await api.getCategoryGroups()).find(g=>g.is_income).id,is_income:true});
    await api.addTransactions(ids.Checking,[{date:'2026-10-01',amount:100000,category:ids.Salary},{date:'2026-11-01',amount:100000,category:ids.Salary},{date:'2026-10-02',amount:-6000,category:ids.A},{date:'2026-11-02',amount:-1500,category:ids.A}].map((row,i)=>({...row,id:`24250000-0000-4000-8000-${String(i+1).padStart(12,'0')}`,notes:`Seed ${i}`,cleared:true})));
    await api.setBudgetAmount('2026-10',ids.A,10000);await send('budget/set-carryover',{startMonth:'2026-10',category:ids.A,flag:true});
    for(const[key,amount]of Object.entries(cfg.current??{}))await api.setBudgetAmount(month,ids[key],amount);
    if(cfg.schedule){ids.Rent='24250000-0000-4000-8000-000000000999';await send('schedule/create',{schedule:{id:ids.Rent,name:'Rent',posts_transaction:false},conditions:[{field:'account',op:'is',value:ids.Checking},{field:'amount',op:'is',value:-12345},{field:'date',op:'is',value:{frequency:'monthly',start:'2026-01-15'}}]});}
   });
   const templates=structuredClone(cfg.templates);for(const t of templates){if(t.scheduleId)t.scheduleId=ids[t.scheduleId];if(cfg.incomeId&&t.type==='percentage')t.category=ids[t.category];}
   // Match source UI's ID-to-name sanitation before invoking the real renderer.
   const renderInput=templates.map(t=>t.type==='percentage'&&t.category===ids.Salary?{...t,category:'Salary'}:t);
   const rendered=await send('budget/render-note-templates',renderInput);
   assert.equal(typeof rendered,'string');
   const notes=mode==='ui'?'Original description\n#template 999':`Original description\n${rendered}`;
   await send('notes-save',{id:ids.A,note:notes});
   if(mode==='ui')await send('budget/set-category-automations',{source:'ui',categoriesWithTemplates:[{id:ids.A,templates}]});
   else await send('budget/store-note-templates');
   const label=id=>Object.entries(ids).find(([,v])=>v===id)?.[0]??id;
   const normalize=v=>Array.isArray(v)?v.map(normalize):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,a])=>[label(k),normalize(a)])):typeof v==='string'?label(v):v;
   const read=async()=>{const categories={};for(const key of ['A','B','C','Salary']){categories[key]={};for(const field of ['budget','goal','long-goal','leftover'])categories[key][field]=(await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids[key]})).value;}return{categories,available:(await send('get-cell',{sheetName:'budget202611',name:method==='tracking'?'total-saved':'to-budget'})).value};};
   const ledger=async()=>(await api.getTransactions(ids.Checking)).map(row=>({id:row.id,date:row.date,amount:row.amount,category:label(row.category),notes:row.notes??'',cleared:!!row.cleared,reconciled:!!row.reconciled})).sort((a,b)=>a.id.localeCompare(b.id));
   const seed=await read(),sourceLedger=await ledger(),effective=normalize((await send('budget/get-category-automations',ids.A))[ids.A]);
   const filename=name+'.zip',path=new URL(filename,root),generated=await api.exportBudget(),bytes=check?await readFile(path):generated;if(!check)archives.push([path,bytes]);
   const notification=await send('budget/overwrite-goal-template',{month}),result=await read();assert.deepEqual(await ledger(),sourceLedger);assert.equal((await send('notes-get',{id:ids.A})).note,notes);
   if(mode==='ui')uiResult=result;
   output.cases.push({name,method,mode,month,ids,configuration:{notes:{A:notes}},templates:normalize(templates),rendered,effective,seed,ledger:sourceLedger,notification,result,roundtrip_equal:mode==='ui'?null:JSON.stringify(result)===JSON.stringify(uiResult),archive:{filename,sha256:createHash('sha256').update(bytes).digest('hex')}});
  }
 }
 const file=new URL('../tests/fixtures/budget-visual-automations.json',import.meta.url);
 if(check){const original=JSON.parse(await readFile(file,'utf8'));if(selected){const names=new Set(output.cases.map(c=>c.name));original.cases=original.cases.filter(c=>names.has(c.name));}const stable=v=>JSON.parse(JSON.stringify({...v,cases:v.cases.map(({ids,...row})=>row)}));assert.deepEqual(stable(output),stable(original));}
 else{let combined=output;if(append){const original=JSON.parse(await readFile(file,'utf8'));assert.ok(output.cases.every(c=>!original.cases.some(old=>old.name===c.name)),'Original observations cannot be replaced');combined={...original,cases:[...original.cases,...output.cases]};}await mkdir(root,{recursive:true});for(const[path,bytes]of archives)await writeFile(path,bytes,{flag:'wx'});await writeFile(file,JSON.stringify(combined,null,2)+'\n',{flag:append?'w':'wx'});}
 console.log(`PASS ${output.cases.length} actual UI/rendered-note observations; ${output.cases.filter(c=>c.roundtrip_equal===false).length} source round-trip differences retained`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
