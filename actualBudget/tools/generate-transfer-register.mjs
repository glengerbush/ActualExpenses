// Independently pin register behavior through Actual 26.10's public AQL/API.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp, readFile, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json',import.meta.url))).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/transfer-register-');
const id=n=>'15000000-0000-4000-8000-'+String(n).padStart(12,'0');
try {
 await api.init({dataDir:directory});
 let cash,card,off;
 await api.runImport('Transfer register oracle',async()=>{
  cash=await api.createAccount({name:'Account 2'},0);
  card=await api.createAccount({name:'Account 3'},0);
  off=await api.createAccount({name:'Off budget',offbudget:true},0);
 });
 const payees=await api.getPayees(), cardPayee=payees.find(p=>p.transfer_acct===card).id, offPayee=payees.find(p=>p.transfer_acct===off).id;
 await api.addTransactions(cash,[{id:id(10),date:'2026-01-01',amount:-1000,subtransactions:[{id:id(11),amount:-600,payee:cardPayee},{id:id(13),amount:-400}]},{id:id(20),date:'2026-01-01',amount:-200,payee:cardPayee},{id:id(30),date:'2026-01-01',amount:-300,payee:offPayee}],{runTransfers:true});
 await api.updateAccount(card,{name:'Zen savings'});
 const counterpartAliases=new Map();
 for(const t of await api.getTransactions(cash))for(const row of [t,...(t.subtransactions??[])])if(row.transfer_id)counterpartAliases.set(row.transfer_id,id(row.id===id(11)?12:row.id===id(20)?21:31));
 const cases=[
  {name:'transfer',query:{filter:{conditions:[{field:'transfer',op:'eq',value:true}]}},filter:{transfer_id:{$ne:null}}},
  {name:'renamed account search',query:{search:'Zen savings'},account:true,filter:{'payee.transfer_acct.name':{$like:'%Zen savings%'}}},
  {name:'renamed payee name',query:{filter:{conditions:[{field:'payee_name',op:'eq',value:'Zen savings'}]}},account:true,filter:{'payee.name':'Zen savings'}},
  {name:'sorted transfer names',query:{sort:'payee',direction:'asc'},account:true,ordered:true,filter:{}},
  {name:'uncategorized',query:{account_scope:'uncategorized'},filter:{$and:[{'account.offbudget':false,category:null,is_parent:false},{$or:{'payee.transfer_acct.offbudget':true,'payee.transfer_acct':null}}]}},
 ];
 const expected=[];
 for(const c of cases){
  let q=api.q('transactions').options({splits:'grouped'}).filter(c.filter);
  if(c.account)q=q.filter({account:cash});
  const {data}=await api.aqlQuery(q.select('*').orderBy(c.ordered?{'payee.name':'asc'}:{id:'asc'}));
  expected.push({name:c.name,query:c.query,account:c.account?id(2):null,...(c.ordered?{ordered:true}:{}),ids:c.ordered?data.map(t=>counterpartAliases.get(t.id)??t.id):data.map(t=>counterpartAliases.get(t.id)??t.id).sort(),total:data.reduce((n,t)=>n+t.amount,0)});
 }
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',cases:expected};
 const target=new URL('../tests/fixtures/transfer-register.json',import.meta.url),serialized=JSON.stringify(result,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(await readFile(target,'utf8'),serialized);else await writeFile(target,serialized);
 console.log('PASS pinned transfer register queries');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
