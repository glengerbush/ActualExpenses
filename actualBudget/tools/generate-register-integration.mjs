// Public Actual 26.10 grouped AQL is the oracle for whole-row split predicates.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/register-integration-oracle-');
const id=n=>'12140000-0000-4000-8000-'+String(n).padStart(12,'0');
const groups=[{id:id(1),name:'First group'},{id:id(2),name:'Second group'}];
const categories=groups.map((g,i)=>({id:id(3+i),name:i?'Second category':'First category',group_id:g.id}));
const payees=[{id:id(5),name:'First payee'},{id:id(6),name:'Second payee'}];
const inputs=[
 {id:id(10),amount:-300,notes:'Parent marker',children:[{id:id(11),amount:-100,category_id:id(3),payee_id:id(5),notes:'First allocation #First'},{id:id(12),amount:-200,category_id:id(4),payee_id:id(6),notes:'Second allocation #Second'}]},
 {id:id(20),amount:-40,notes:'Null ordinary'},
 {id:id(21),amount:-50,notes:'First ordinary #First',category_id:id(3),payee_id:id(5)},
 {id:id(22),amount:-60,notes:'Second ordinary #Second',category_id:id(4),payee_id:id(6)},
];
const condition=(field,op,value)=>({field,op,value});
const cases=[
 ...['category','payee','category_group'].map((field,i)=>({name:field+' not equal includes null',conditions:[condition(field,'ne',id([3,5,1][i]))]})),
 {name:'same child AND',conditions:[condition('category','eq',id(3)),condition('payee','eq',id(5))]},
 {name:'different children AND fails',conditions:[condition('category','eq',id(3)),condition('payee','eq',id(6))]},
 {name:'parent and child AND fails',conditions:[condition('notes','contains','Parent marker'),condition('category','eq',id(3))]},
 {name:'different children OR',op:'or',conditions:[condition('category','eq',id(3)),condition('payee','eq',id(6))]},
 {name:'child amount',conditions:[condition('amount','eq',-200)]},
 {name:'parent true',conditions:[condition('parent','eq',true)]},
 {name:'parent false',conditions:[condition('parent','eq',false)]},
 {name:'child names AND',conditions:[condition('category_name','eq','First category'),condition('payee_name','eq','First payee')]},
 {name:'child category group',conditions:[condition('category_group','eq',id(2))]},
 {name:'child tags',conditions:[condition('notes','hasTags','#Second')]},
 {name:'child tags cross condition fails',conditions:[condition('notes','hasTags','#Second'),condition('category','eq',id(3))]},
 {name:'uncategorized scope',scope:'uncategorized',conditions:[]},
 {name:'search child',search:'Second allocation',conditions:[]},
 {name:'search and filter same child',search:'Second allocation',conditions:[condition('category','eq',id(4))]},
 {name:'search and filter different child fails',search:'Second allocation',conditions:[condition('category','eq',id(3))]},
];
try {
 await api.init({dataDir:directory});let account;await api.runImport('Register integration oracle',async()=>{account=await api.createAccount({name:'Spending'},0);});
 const references=new Map();
 for(const g of groups)references.set(g.id,await api.createCategoryGroup({name:g.name}));
 for(const c of categories)references.set(c.id,await api.createCategory({name:c.name,group_id:references.get(c.group_id)}));
 for(const p of payees)references.set(p.id,await api.createPayee({name:p.name}));
 const tx=t=>({id:t.id,date:'2026-01-02',amount:t.amount,notes:t.notes,category:references.get(t.category_id),payee:references.get(t.payee_id),...(t.children?{subtransactions:t.children.map(tx)}:{})});
 await api.addTransactions(account,inputs.map(tx));
 const fieldMap={category_group:'category.group',category_name:'category.name',payee_name:'payee.name',parent:'is_parent'};
 const expected=[];
 for(const c of cases){
  let q=api.q('transactions').options({splits:'grouped'}).filter({account});
  if(c.scope)q=q.filter({'account.offbudget':false,category:null,is_parent:false});
  if(c.search)q=q.filter({notes:{$like:'%'+c.search+'%'}});
  if(c.conditions.length)q=q.filter({[c.op==='or'?'$or':'$and']:c.conditions.map(x=>({[fieldMap[x.field]??x.field]:{[x.op==='contains'?'$like':x.op==='hasTags'?'$regexp':'$'+x.op]:x.op==='contains'?'%'+x.value+'%':x.op==='hasTags'?'(?<!#)'+x.value+'([\\s#]|$)':references.get(x.value)??x.value}}))});
  const {data}=await api.aqlQuery(q.select('*').orderBy({id:'asc'}));
  expected.push({name:c.name,query:{...(c.scope?{account_scope:c.scope}:{}),...(c.search?{search:c.search}:{}),filter:{op:c.op??'and',conditions:c.conditions}},ids:data.map(t=>t.id),total:data.reduce((n,t)=>n+t.amount,0)});
 }
 const fixture={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',description:'Grouped executor matches complete predicate on one parent or child. Totals sum returned parent amounts exactly once, per register API contract; not the separate inline AQL aggregate mode.',groups,categories,payees,inputs,cases:expected};
 const target=new URL('../tests/fixtures/register-integration-26.10.json',import.meta.url),text=JSON.stringify(fixture,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(await readFile(target,'utf8'),text);else await writeFile(target,text);
 console.log('PASS pinned Actual grouped child and null predicates');
} finally {await api.shutdown();await rm(directory,{recursive:true,force:true});}
