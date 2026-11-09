// Independent pinned Actual26.10 runtime. Replacement code is never imported.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
process.env.TZ='UTC';
const NativeDate=Date;let asOf='2026-11-06T12:00:00Z';
globalThis.Date=class extends NativeDate {constructor(...args){super(...(args.length?args:[NativeDate.parse(asOf)]));}static now(){return NativeDate.parse(asOf);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const root=await mkdtemp(tmpdir()+'/schedules-oracle-');
const id=n=>`21220000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const out={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',calendar:[],snapshots:[]};
try{
 const engine=await api.init({dataDir:root});const send=(name,body)=>engine.send(name,body);
 const ids={};await api.runImport('Schedule oracle',async()=>{ids.account=await api.createAccount({name:'Spending'},0);ids.peer=await api.createAccount({name:'Savings'},0);ids.payee=await api.createPayee({name:'Power'});const group=await api.createCategoryGroup({name:'Bills',is_income:false});ids.category=await api.createCategory({name:'Utilities',group_id:group});});out.ids=ids;
 const configs=[
 ['daily count',{start:'2026-11-06',frequency:'daily',endMode:'after_n_occurrences',endOccurrences:4}],
 ['weekly interval',{start:'2026-11-06',frequency:'weekly',interval:2,endMode:'never'}],
 ['day31 skips months',{start:'2026-10-31',frequency:'monthly',endMode:'never'}],
 ['last day leap year',{start:'2026-11-01',frequency:'monthly',patterns:[{type:'day',value:-1}],endMode:'never'}],
 ['mixed monthly patterns',{start:'2026-11-01',frequency:'monthly',patterns:[{type:'day',value:15},{type:'MO',value:1}],endMode:'after_n_occurrences',endOccurrences:3}],
 ['specific multiple days',{start:'2026-11-01',frequency:'monthly',patterns:[{type:'day',value:1},{type:'day',value:15}],endMode:'never'}],
 ['weekend before',{start:'2026-11-07',frequency:'daily',skipWeekend:true,weekendSolveMode:'before',endMode:'after_n_occurrences',endOccurrences:5}],
 ['weekend after',{start:'2026-11-07',frequency:'daily',skipWeekend:true,weekendSolveMode:'after',endMode:'after_n_occurrences',endOccurrences:5}],
 ['end date inclusive',{start:'2026-11-06',frequency:'daily',endMode:'on_date',endDate:'2026-11-08'}],
 ['last weekday',{start:'2026-11-01',frequency:'monthly',patterns:[{type:'FR',value:-1}],endMode:'never'}],
 ['yearly leap day',{start:'2024-02-29',frequency:'yearly',endMode:'never'}],
 ];
 for(const [name,config]of configs){const dates=await send('schedule/get-upcoming-dates',{config,count:16});assert.ok(Array.isArray(dates),JSON.stringify(dates));out.calendar.push({name,as_of:asOf.slice(0,10),config,dates});}
 const conds=(date,amount=-100,account=ids.account)=>[{field:'account',op:'is',value:account},{field:'payee',op:'is',value:ids.payee},{field:'amount',op:'is',value:amount},{field:'date',op:'is',value:date}];
 async function create(n,date,automatic=false,extra={}){const conditions=conds(date);const schedule={id:id(n),name:'Schedule '+n,posts_transaction:automatic,...extra};const saved=await send('schedule/create',{schedule,conditions});assert.equal(saved,id(n));return{id:id(n),conditions,...schedule};}
 async function snapshot(step){const schedules=(await api.aqlQuery(api.q('schedules').select('*'))).data;const rows=(await api.aqlQuery(api.q('transactions').options({splits:'all'}).select('*').orderBy({date:'asc'},{id:'asc'}))).data;out.snapshots.push({step,as_of:asOf.slice(0,10),schedules:schedules.map(s=>{for(const k of ['id','next_date','completed','posts_transaction','rule'])assert.ok(k in s,k);return {id:s.id,next_date:s.next_date,completed:s.completed,posts_transaction:s.posts_transaction,rule:s.rule};}).sort((a,b)=>a.id.localeCompare(b.id)),transactions:rows.sort((a,b)=>a.date.localeCompare(b.date)||String(a.schedule).localeCompare(String(b.schedule))||Object.keys(ids).find(k=>ids[k]===a.account).localeCompare(Object.keys(ids).find(k=>ids[k]===b.account))||a.amount-b.amount||String(a.notes).localeCompare(String(b.notes))).map(t=>{for(const k of ['id','amount','date','account','cleared'])assert.ok(k in t,k);return {id:t.id,account:t.account,payee:t.payee,category:t.category,amount:t.amount,date:t.date,notes:t.notes,cleared:t.cleared,schedule:t.schedule,transfer_id:t.transfer_id};})});}
 out.created=[];
 out.created.push(await create(1,{start:'2026-11-06',frequency:'daily',endMode:'after_n_occurrences',endOccurrences:5},true));
 out.created.push(await create(2,'2026-11-06',false));
 out.created.push(await create(3,'2026-11-07',true));
 out.created.push(await create(4,{start:'2026-11-07',frequency:'monthly',skipWeekend:true,weekendSolveMode:'before',endMode:'never'},false));
 await snapshot('created');
 await send('schedule/post-transaction',{id:id(2)});await snapshot('manual post');
 await send('schedule/skip-next-date',{id:id(4)});await snapshot('weekend skipped');
 asOf='2026-11-09T12:00:00Z';await send('schedule/force-run-service');await snapshot('catch up');
 await send('schedule/force-run-service');await snapshot('repeat same day');
 asOf='2026-11-10T12:00:00Z';await send('schedule/force-run-service');await snapshot('next day');
 await send('schedule/update',{schedule:{id:id(1),completed:true}});await snapshot('completed');
 // Export authentic source schedules/next-date state for converter tests.
 if(!process.argv.includes('--check'))await writeFile(new URL('../tests/fixtures/schedules-26.10.zip',import.meta.url),await api.exportBudget());
 const path=new URL('../tests/fixtures/schedules.json',import.meta.url);
 // Generated rule/transaction IDs are normalized by first appearance only.
 const replacements=new Map(Object.entries(ids).map(([k,v])=>[v,`<${k}>`]));let serial=0;
 const normalized=JSON.stringify(out,(k,v)=>{if(typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f-]{27}$/.test(v)&&!v.startsWith('21220000-')){if(!replacements.has(v))replacements.set(v,`<generated-${++serial}>`);return replacements.get(v);}return v;});
 if(process.argv.includes('--check')){const expected=JSON.parse(await readFile(path,'utf8'));assert.deepEqual(JSON.parse(normalized),expected);}else await writeFile(path,JSON.stringify(JSON.parse(normalized),null,2)+'\n');
 console.log(`PASS ${out.calendar.length} pinned calendar cases + ${out.snapshots.length} persisted schedule lifecycle snapshots`);
}finally{await api.shutdown();await rm(root,{recursive:true,force:true});}
