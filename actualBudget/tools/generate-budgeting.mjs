// Independent Actual26.10 public API/desktop message oracle, with genuine exports.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp,readFile,writeFile,rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/budgeting-oracle-');
const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',cases:[]};
const pendingFiles=new Map();
const save=async(name,value)=>{const path=new URL('../tests/fixtures/'+name,import.meta.url);if(process.argv.includes('--check')) {if(name.endsWith('.json')) {const normalize=x=>({...x,cases:x.cases.map(({ids,...c})=>c)});assert.deepEqual(normalize(JSON.parse(await readFile(path,'utf8'))),normalize(JSON.parse(value)));}} else pendingFiles.set(path,value);};
try {
 const engine=await api.init({dataDir:directory});
 for(const method of ['tracking','envelope']) {
  const ids={};
  await api.runImport('Budgeting '+method,async()=>{
   await engine.send('preferences/save',{id:'budgetType',value:method});
   ids.cash=await api.createAccount({name:'Cash'},0);
   ids.card=await api.createAccount({name:'Credit card'},0);
   ids.off=await api.createAccount({name:'Off budget',offbudget:true},0);
   const expenses=await api.createCategoryGroup({name:'Everyday',is_income:false});ids.expenses=expenses;
   const income=(await api.getCategoryGroups()).find(g=>g.is_income).id;
   for(const [name,group,is_income] of [['Food',expenses,false],['Travel',expenses,false],['Salary',income,true]])ids[name]=await api.createCategory({name,group_id:group,is_income});
   await api.addTransactions(ids.cash,[{date:'2026-12-01',amount:50000,category:ids.Salary},{date:'2026-12-02',amount:-12000,category:ids.Food},{date:'2026-12-03',amount:2000,category:ids.Food},{date:'2026-12-04',amount:-8000,category:ids.Travel},{date:'2027-01-02',amount:30000,category:ids.Salary},{date:'2027-01-03',amount:-5000,category:ids.Food}]);
   await api.addTransactions(ids.card,[{date:'2026-12-01',amount:-20000,category:ids.Salary},{date:'2026-12-06',amount:-1000,category:ids.Food}]);
   await api.addTransactions(ids.off,[{date:'2026-12-07',amount:99999,category:ids.Salary}]);
   const payees=await api.getPayees();
   await api.addTransactions(ids.cash,[{date:'2026-12-08',amount:-4000,payee:payees.find(p=>p.transfer_acct===ids.card).id}],{runTransfers:true});
   await api.addTransactions(ids.cash,[{date:'2026-12-09',amount:-1500,payee:payees.find(p=>p.transfer_acct===ids.off).id,category:ids.Travel}],{runTransfers:true});
   await api.addTransactions(ids.cash,[{date:'2026-12-10',amount:-1000,subtransactions:[{amount:-600,category:ids.Food},{amount:-400,category:ids.Travel}]}]);
  });
  // Ensure the year-boundary spreadsheet exists through the public bounds call.
  await api.getBudgetMonths();
  const steps=[];let activeMethod=method;
  async function snap(action) {
   const months={};
   for(const month of ['2026-11','2026-12','2027-01','2027-02']) {
    const raw=await api.getBudgetMonth(month);
    const cells=await engine.send(activeMethod+'-budget-month',{month});
    const values=Object.fromEntries(cells.map(c=>[c.name.split('!')[1],c.value]));
    if(activeMethod==='envelope') {
     values['buffered-selected']=(await engine.send('get-cell',{sheetName:'budget'+month.replace('-',''),name:'buffered-selected'})).value;
     values.salaryCarry=(await engine.send('get-cell',{sheetName:'budget'+month.replace('-',''),name:'carryover-'+ids.Salary})).value;
    }
    months[month]={summary:Object.fromEntries((activeMethod==='tracking'?['total-budgeted','total-budget-income','total-saved','total-income','total-spent','real-saved','total-leftover']:['available-funds','last-month-overspent','buffered','buffered-selected','total-budgeted','to-budget','from-last-month','total-income','total-spent','total-leftover']).map(k=>{assert.ok(Object.hasOwn(values,k),'missing pinned cell '+k);assert.equal(typeof values[k],'number','nonnumeric pinned cell '+k);return [k,values[k]]})),categories:Object.fromEntries(Object.entries(ids).filter(([k])=>['Food','Travel','Salary'].includes(k)).map(([name,id])=>{const c=raw.categoryGroups.flatMap(g=>g.categories).find(c=>c.id===id);assert.equal(typeof (c.received??c.spent),'number',name+' actual missing');if(activeMethod!=='envelope'||name!=='Salary'){assert.equal(typeof c.budgeted,'number');assert.equal(typeof c.balance,'number');}return [name,{budgeted:activeMethod==='envelope'&&name==='Salary'?0:c.budgeted,actual:c.received??c.spent,balance:activeMethod==='envelope'&&name==='Salary'?0:c.balance,carryover:activeMethod==='envelope'&&name==='Salary'?!!values.salaryCarry:!!c.carryover}]}))};
   }
   steps.push({action,method:activeMethod,months});
  }
  await save('budgeting-'+method+'-base.zip',await api.exportBudget());
  await snap(null);
  const actions=[
   {action:'set',month:'2026-12',category:'Food',amount:10000},
   {action:'set',month:'2026-12',category:'Travel',amount:5000},
   ...(method==='tracking'?[{action:'set',month:'2026-12',category:'Salary',amount:35000}]:[]),
   {action:'carryover',month:'2026-12',category:'Food',enabled:true},
   {action:'copy_previous',month:'2027-01'},
   ...(method==='envelope'?[{action:'hold',month:'2026-12',amount:5000},{action:'move',month:'2027-01',from:'Food',to:'Travel',amount:1000},{action:'cover',month:'2026-12',from:'to-budget',to:'Travel',amount:3000},{action:'cover',month:'2027-01',from:'Food',to:'Travel',amount:500},{action:'reset_hold',month:'2026-12'}]:[]),
   {action:'copy_year',month:'2027-01',category:'Food'},
   ...(method==='envelope'?[
    {action:'hold',month:'2026-12',amount:1000},
    {action:'carryover',month:'2026-12',category:'Salary',enabled:true},
    {action:'reset_hold',month:'2026-12'},
    {action:'hold',month:'2026-12',amount:-1000},
    {action:'carryover',month:'2026-12',category:'Salary',enabled:false},
   ]:[]),
   {action:'set',month:'2026-11',category:'Food',amount:6000},
   {action:'copy_year',month:'2026-11',category:'Food'},
   {action:'hide_category',month:'2026-12',category:'Travel',enabled:true},
   {action:'hide_group',month:'2026-12',group:'expenses',enabled:true},
   {action:'hide_group',month:'2026-12',group:'expenses',enabled:false},
   {action:'hide_category',month:'2026-12',category:'Travel',enabled:false},
   {action:'method',month:'2026-12',method:method==='tracking'?'envelope':'tracking'},
   {action:'set',month:'2026-12',category:'Food',amount:777},
   {action:'method',month:'2026-12',method},
  ];
  for(const a of actions) {
   const category=ids[a.category];
   const copyAmount=a.action==='copy_year'?(await api.getBudgetMonth(a.month)).categoryGroups.flatMap(g=>g.categories).find(c=>c.id===category).budgeted:null;
   if(a.action==='method'){await engine.send('preferences/save',{id:'budgetType',value:a.method});activeMethod=a.method;}
   else if(a.action==='hide_category')await api.updateCategory(category,{name:a.category,hidden:a.enabled});
   else if(a.action==='hide_group')await api.updateCategoryGroup(ids[a.group],{name:"Everyday",hidden:a.enabled});
   else if(a.action==='set')await api.setBudgetAmount(a.month,category,a.amount);
   else if(a.action==='carryover')await engine.send('budget/set-carryover',{startMonth:a.month,category,flag:a.enabled});
   else if(a.action==='copy_previous')await engine.send('budget/copy-previous-month',{month:a.month});
   else if(a.action==='copy_year')await engine.send('budget/copy-until-year-end',{month:a.month,category});
   else if(a.action==='hold')await engine.send('budget/hold-for-next-month',{month:a.month,amount:a.amount});
   else if(a.action==='reset_hold')await api.resetBudgetHold(a.month);
   else if(a.action==='move')await engine.send('budget/transfer-category',{month:a.month,from:ids[a.from],to:ids[a.to],amount:a.amount,currencyCode:'USD'});
   else if(a.action==='cover')await engine.send('budget/cover-overspending',{month:a.month,from:a.from==='to-budget'?a.from:ids[a.from],to:ids[a.to],amount:a.amount,currencyCode:'USD'});
   if(a.action==='copy_year'&&a.month==='2027-01') {
    const available=(await api.getBudgetMonths()).filter(m=>m>'2027-01'&&m<='2027-12').sort();
    if(available.length) {const last=await api.getBudgetMonth(available.at(-1));assert.equal(last.categoryGroups.flatMap(g=>g.categories).find(c=>c.id===ids.Food).budgeted,copyAmount);}
   }
   await snap(a);
  }
  await save('budgeting-'+method+'-allocated.zip',await api.exportBudget());
  result.cases.push({method,ids,steps});
 }
 await save('budgeting-26.10.0.json',JSON.stringify(result,null,2)+'\n');
 for(const [path,value] of pendingFiles)await writeFile(path,value);
 console.log('PASS independent tracking/envelope engine, debt/refunds/splits/transfers, year boundary/carry/copy/holds/moves with genuine exports');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
