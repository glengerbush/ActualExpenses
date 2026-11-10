// Independent financial observations from the real pinned Actual engine.
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
 {name:'apply-skip-overwrite-selected',notes:{A:'#template 72.99',B:'#template 10'},current:{A:2222},steps:[{action:'apply'},{action:'overwrite'},{action:'single',category:'A'},{action:'multiple',categories:['A','B']}]},
 {name:'multiline-long-goal',notes:{A:'Netflix\n#template 24.99\nDisney\n#template 9.99\nAmazon\n#template 7.99\n#goal 1000'}},
 {name:'goal-only',notes:{A:'#goal 1000'},current:{A:2222}},
 {name:'monthly-cap',notes:{A:'#template 40 up to 100',B:'#template 15 up to 2 hold'}},
 {name:'refill-and-held-refill',notes:{A:'#template up to 100',B:'#template up to 30 hold'}},
 {name:'daily-weekly-caps',notes:{A:'#template 200 up to 3 per day',B:'#template 200 up to 25 per week starting 2026-11-02'}},
 {name:'save-by-date',notes:{A:'#template 1000 by 2027-02',B:'#template 99.99 by 2026-11'}},
 {name:'repeating-target',notes:{A:'#template 1000 by 2026-09 repeat every 3 months',B:'#template 500 by 2025-11 repeat every year'}},
 {name:'spend-from-history',notes:{A:'#template 1000 by 2027-02 spend from 2026-09'}},
 {name:'periodic-day-week-month',notes:{A:'#template 1.23 repeat every 3 days starting 2026-10-30\n#template 10 repeat every 2 weeks starting 2026-10-19',B:'#template 99 repeat every 2 months starting 2026-01-31',C:'#template 25 repeat every year starting 2025-11-15'}},
 {name:'average-and-adjustments',notes:{A:'#template average 3 months [increase 12.5%]',B:'#template average 2 months [decrease 1.25]'}},
 {name:'copy-history',notes:{A:'#template copy from 2 months ago',B:'#template copy from 1 months ago up to 30'}},
 {name:'income-percentages',notes:{A:'#template 12.5% of Salary',B:'#template 10% of previous Salary',C:'#template 2.5% of all income'}},
 {name:'available-per-priority',notes:{A:'#template-1 10% of available funds',B:'#template-1 10% of available funds',C:'#template-2 10% of available funds'}},
 {name:'priority-demand-and-clamp',notes:{A:'#template-1 100',B:'#template-2 200',C:'#template 1'},income:5000,noHistory:true},
 {name:'weighted-remainder-pennies',notes:{A:'#template remainder',B:'#template remainder 2',C:'#template remainder 3'},income:5,noHistory:true},
 {name:'capped-remainder-redistribution',notes:{A:'#template remainder 2 up to 0.02',B:'#template remainder',C:'#template remainder 3'},income:11,noHistory:true},
 {name:'schedule-demand-adjustments',notes:{A:'#template schedule Rent',B:'#template schedule full Rent [increase 10%]',C:'#template schedule Rent [decrease 5.25]'},schedule:true},
 {name:'hidden-and-selected',notes:{A:'#template 100',B:'#template 200',C:'#template 300'},hidden:['B'],steps:[{action:'overwrite'},{action:'multiple',categories:['B']}]},
 {name:'invalid-goals-atomic',notes:{A:'#template 10',B:'#goal 100\n#goal 200'},current:{A:777,B:888}},
 {name:'invalid-schedule-atomic',notes:{A:'#template 10',B:'#template schedule Missing plan'},current:{A:777,B:888}},
 {name:'invalid-income-category',notes:{A:'#template 10% of Missing salary',B:'#template 20'},current:{A:777,B:888}},
 {name:'fraction-hidden',notes:{A:'#template 12.49\n#template 0.02',B:'#template 12.50'},hideFraction:true},
 {name:'jpy-minor-units',notes:{A:'#template 72.99',B:'#template 12.50'},currency:'JPY'},
 {name:'kwd-minor-units',notes:{A:'#template 72.99',B:'#template 12.50'},currency:'KWD'},
 {name:'tracking-income-category',methods:['tracking'],notes:{Salary:'#template 2000',A:'#template-1 100'}},
];
const expressions=process.argv.includes('--expressions');
if(expressions)matrix.splice(0,matrix.length,...matrix.filter(c=>c.schedule));
const directory=await mkdtemp(tmpdir()+'/budget-template-financial-'),check=process.argv.includes('--check');
const stem=expressions?'budget-template-expressions':'budget-template-financial';
const root=new URL('../tests/fixtures/'+stem+'/',import.meta.url);
const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',cases:[]};
const archives=[];
try{
 const engine=await api.init({dataDir:directory}),send=(name,body)=>engine.send(name,body);
 for(const cfg of matrix)for(const method of cfg.methods??['envelope','tracking']){
  const ids={},name=method+'-'+cfg.name,month='2026-11';
  await api.runImport(name,async()=>{
   await send('preferences/save',{id:'budgetType',value:method});
   await send('preferences/save',{id:'defaultCurrencyCode',value:cfg.currency??'USD'});
   await send('preferences/save',{id:'hideFraction',value:String(cfg.hideFraction??false)});
   ids.Checking=await api.createAccount({name:'Checking'},0);
   ids.Plans=await api.createCategoryGroup({name:'Plans',is_income:false});
   for(const key of ['A','B','C'])ids[key]=await api.createCategory({name:key,group_id:ids.Plans,is_income:false});
   const income=(await api.getCategoryGroups()).find(g=>g.is_income).id;
   ids.Salary=await api.createCategory({name:'Salary',group_id:income,is_income:true});
   const rows=[{date:'2026-11-01',amount:cfg.income??100001,category:ids.Salary}];
   if(!cfg.noHistory)rows.push({date:'2026-09-03',amount:-2000,category:ids.A},{date:'2026-10-01',amount:100000,category:ids.Salary},{date:'2026-10-03',amount:-6000,category:ids.A},{date:'2026-11-03',amount:-1500,category:ids.A},{date:'2026-10-04',amount:-1000,category:ids.B});
   await api.addTransactions(ids.Checking,rows.map((row,index)=>({...row,id:`23240000-0000-4000-8000-${String(index+1).padStart(12,'0')}`,notes:`Seed ${index}`,cleared:true})));
   if(!cfg.noHistory){
    await api.setBudgetAmount('2026-09',ids.A,5000);
    await api.setBudgetAmount('2026-10',ids.A,10000);
    await api.setBudgetAmount('2026-10',ids.B,2000);
    await send('budget/set-carryover',{startMonth:'2026-10',category:ids.A,flag:true});
   }
   for(const [key,amount] of Object.entries(cfg.current??{}))await api.setBudgetAmount(month,ids[key],amount);
   if(cfg.schedule){
    ids.Rent='23240000-0000-4000-8000-000000000999';
    await send('schedule/create',{schedule:{id:ids.Rent,name:'Rent',posts_transaction:false},conditions:[{field:'account',op:'is',value:ids.Checking},{field:'amount',op:'is',value:-12345},{field:'date',op:'is',value:{frequency:'monthly',start:'2026-01-15'}}]});
   }
   for(const [key,note] of Object.entries(cfg.notes))await send('notes-save',{id:ids[key],note});
   for(const key of cfg.hidden??[])await api.updateCategory(ids[key],{name:key,hidden:true});
  });
  if(expressions){
   const schedule=(await api.aqlQuery(api.q('schedules').filter({id:ids.Rent}).select('*'))).data[0];
   const rule=await send('rule-get',{id:schedule.rule});
   const saved=await send('rule-update',{...rule,actions:[...rule.actions,
    {op:'set-split-amount',value:null,options:{method:'formula',splitIndex:1,formula:'=parent_amount/200'}},
    {op:'set',field:'category',value:ids.A,options:{splitIndex:1}},
    {op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:2}},
    {op:'set',field:'category',value:ids.B,options:{splitIndex:2}}
   ]});assert.ok(!saved?.error,JSON.stringify(saved));
  }
  await send('budget/store-note-templates');
  const label=id=>Object.entries(ids).find(([,value])=>value===id)?.[0]??id;
  const normalized=value=>Array.isArray(value)?value.map(normalized):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[label(k),normalized(v)])):typeof value==='string'?label(value):value;
  const templates={};
  for(const key of ['A','B','C','Salary'])templates[key]=normalized((await send('budget/get-category-automations',ids[key]))[ids[key]]??[]);
  const ledger=async()=>(await api.getTransactions(ids.Checking)).map(row=>({id:row.id,date:row.date,amount:row.amount,category:label(row.category),notes:row.notes??'',cleared:!!row.cleared,reconciled:!!row.reconciled})).sort((a,b)=>a.id.localeCompare(b.id));
  const read=async()=>{
   const categories={};
   for(const key of ['A','B','C','Salary']){
    categories[key]={};
    for(const field of ['budget','goal','long-goal','leftover']){
     const cell=await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids[key]});
     assert.ok(Object.hasOwn(cell,'value'),`${name} ${key} ${field} source cell missing`);categories[key][field]=cell.value;
    }
   }
   const field=method==='tracking'?'total-saved':'to-budget';
   return {categories,available:(await send('get-cell',{sheetName:'budget202611',name:field})).value};
  };
  const seed=await read(),sourceLedger=await ledger();
  const budgetMonth=await api.getBudgetMonth(month);
  const category_order=budgetMonth.categoryGroups.flatMap(g=>g.categories).map(c=>label(c.id));
  const generated=await api.exportBudget(),filename=name+'.zip',archivePath=new URL(filename,root);
  const bytes=check?await readFile(archivePath):generated;
  if(!check)archives.push([archivePath,bytes]);
  const projections={};
  for(const key of Object.keys(cfg.notes)){
   try{projections[key]=await send('budget/dry-run-category-template',{month,categoryId:ids[key],templates:templates[key]});}
   catch(error){projections[key]={error:error.message};}
  }
  assert.deepEqual(await read(),seed,`${name} dry run changed financial state`);
  const steps=[];
  for(const action of cfg.steps??[{action:'overwrite'}]){
   let notification;
   const route={apply:'budget/apply-goal-template',overwrite:'budget/overwrite-goal-template',single:'budget/apply-single-template',multiple:'budget/apply-multiple-templates'}[action.action];
   try{notification=await send(route,{month,...(action.category?{category:ids[action.category]}:{}),...(action.categories?{categoryIds:action.categories.map(c=>ids[c])}:{})});}
   catch(error){notification={error:error.message};}
   steps.push({action,notification,result:await read()});
   assert.deepEqual(await ledger(),sourceLedger,`${name} template application changed transactions`);
   for(const [key,note] of Object.entries(cfg.notes))assert.equal((await send('notes-get',{id:ids[key]})).note,note);
  }
  output.cases.push({name,method,month,configuration:cfg,ids,category_order,templates,seed,ledger:sourceLedger,projections,steps,archive:{filename,sha256:createHash('sha256').update(bytes).digest('hex')}});
 }
 const file=new URL('../tests/fixtures/'+stem+'.json',import.meta.url);
 if(check){const original=JSON.parse(await readFile(file,'utf8'));const stable=v=>({...v,cases:v.cases.map(({ids,...record})=>record)});assert.deepEqual(stable(output),stable(original));}
 else{await mkdir(root,{recursive:true});for(const [path,bytes] of archives)await writeFile(path,bytes,{flag:'wx'});await writeFile(file,JSON.stringify(output,null,2)+'\n',{flag:'wx'});}
 console.log(`PASS ${output.cases.length} independent budget-template financial cases; original ledgers and notes retained`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
