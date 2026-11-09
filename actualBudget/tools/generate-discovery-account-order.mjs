// Genuine source account-rank tie witness, using public Actual mutations only.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
process.env.TZ='UTC';
const NativeDate=Date;
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:['2026-11-09T12:00:00Z']));}static now(){return NativeDate.parse('2026-11-09T12:00:00Z');}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const root=await mkdtemp(`${tmpdir()}/discovery-account-order-`);
try{
 const engine=await api.init({dataDir:root});
 const ids={};
 await api.runImport('Discovery rank tie',async()=>{
  const accounts=[await api.createAccount({name:'First'},0),await api.createAccount({name:'Second'},0)].sort();
  ids.ZULU=accounts[0];ids.ALPHA=accounts[1];
  await api.updateAccount(ids.ZULU,{name:'Zulu',sort_order:0});
  await api.updateAccount(ids.ALPHA,{name:'Alpha',sort_order:0});
  ids.PAYEE=await api.createPayee({name:'Shared bill'});
  for(const [account,amount] of [['ALPHA',-10000],['ZULU',-20000]]){
   const rows=['2026-10-12','2026-10-19','2026-10-26','2026-11-02'].map((date,i)=>{
    const label=`${account}-${i}`;ids[label]=`22220000-0000-4000-8000-${String((account==='ALPHA'?910:920)+i).padStart(12,'0')}`;
    return{id:ids[label],date,amount,payee:ids.PAYEE,notes:label};
   });
   await api.addTransactions(ids[account],rows);
  }
 });
 const normalize=value=>JSON.parse(JSON.stringify(value,(_,v)=>{
  if(typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f-]{27}$/.test(v)){const label=Object.keys(ids).find(k=>ids[k]===v);assert.ok(label,`Unknown source ID ${v}`);return label;}return v;
 }));
 const accounts=(await api.aqlQuery(api.q('accounts').select('*'))).data;
 assert.deepEqual(accounts.map(a=>a.sort_order),[0,0]);
 assert.deepEqual(accounts.map(a=>a.name),['Alpha','Zulu']);
 assert.ok(ids.ZULU<ids.ALPHA,'UUID ordering intentionally opposes source name ordering');
 const suggestions=(await engine.send('schedule/discover')).map(({id,...rest})=>normalize(rest));
 assert.equal(suggestions.length,1);assert.equal(suggestions[0].account,'ALPHA');assert.equal(suggestions[0].amount,-10000);
 const observed={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',accounts:normalize(accounts.map(a=>({id:a.id,name:a.name,sort_order:a.sort_order}))),suggestions};
 const jsonPath=new URL('../tests/fixtures/discovery-account-order.json',import.meta.url);
 const zipPath=new URL('../tests/fixtures/discovery-account-order-26.10.zip',import.meta.url);
 if(process.argv.includes('--check')){
  const saved=JSON.parse(await readFile(jsonPath,'utf8'));
  assert.deepEqual(observed,saved.observed);
  assert.equal(createHash('sha256').update(await readFile(zipPath)).digest('hex'),saved.archive_sha256);
 }else{
  const zip=await api.exportBudget();await writeFile(zipPath,zip,{flag:'wx'});
  await writeFile(jsonPath,`${JSON.stringify({observed,ids,archive_sha256:createHash('sha256').update(zip).digest('hex')},null,2)}\n`,{flag:'wx'});
 }
 console.log('PASS genuine tied-rank account discovery winner');
}finally{await api.shutdown();await rm(root,{recursive:true,force:true});globalThis.Date=NativeDate;}
