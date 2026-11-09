// Independent pinned source: historical schedule deletion does not own payment unlinking.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';const NativeDate=Date;
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:['2026-11-09T12:00:00Z']));}static now(){return NativeDate.parse('2026-11-09T12:00:00Z');}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const root=await mkdtemp(tmpdir()+'/schedule-deleted-link-');
try{
 const engine=await api.init({dataDir:root}),send=(name,body)=>engine.send(name,body);const ids={PAYMENT:'22220000-0000-4000-8000-000000000981',SCHEDULE:'22220000-0000-4000-8000-000000000982'};
 await api.runImport('Historical schedule link',async()=>{ids.ACCOUNT=await api.createAccount({name:'Spending'},0);ids.PAYEE=await api.createPayee({name:'Bill'});});
 await api.addTransactions(ids.ACCOUNT,[{id:ids.PAYMENT,amount:-100,date:'2026-11-09',payee:ids.PAYEE,notes:'Original payment',cleared:true}],{runTransfers:true});
 await send('schedule/create',{schedule:{id:ids.SCHEDULE,name:'Deleted plan'},conditions:[{field:'account',op:'is',value:ids.ACCOUNT},{field:'payee',op:'is',value:ids.PAYEE},{field:'amount',op:'is',value:-100},{field:'date',op:'is',value:'2026-11-09'}]});
 const snapshots=[];const label=value=>Object.entries(ids).find(([,id])=>id===value)?.[0]??value;
 async function snapshot(step,expected){let rows;for(let n=0;n<100;n++){rows=await api.getTransactions(ids.ACCOUNT);if(rows.find(r=>r.id===ids.PAYMENT)?.schedule===expected||expected===null&&rows.find(r=>r.id===ids.PAYMENT)?.schedule==null)break;await new Promise(r=>setTimeout(r,10));}assert.equal(rows.length,1);assert.equal(rows[0].schedule??null,expected);snapshots.push({step,rows:rows.map(r=>({id:label(r.id),account:label(r.account),payee:label(r.payee),category:r.category??null,amount:r.amount,date:r.date,notes:r.notes??'',cleared:!!r.cleared,reconciled:!!r.reconciled,schedule:r.schedule?label(r.schedule):null})),active_schedule_count:(await api.aqlQuery(api.q('schedules').select('*'))).data.length,unlinked_query_ids:(await api.aqlQuery(api.q('transactions').filter({schedule:null}).select('id'))).data.map(r=>label(r.id)).sort(),balance:await api.getAccountBalance(ids.ACCOUNT)});}
 await send('transactions-batch-update',{updated:[{id:ids.PAYMENT,schedule:ids.SCHEDULE}]});await snapshot('linked',ids.SCHEDULE);
 await send('schedule/delete',{id:ids.SCHEDULE});await snapshot('schedule deleted, payment retained',null);
 const original=await api.exportBudget();const tempZip=root+'/deleted.zip';await writeFile(tempZip,original);
 const raw=JSON.parse(execFileSync('python3',['-c',`import json,zipfile,sqlite3,tempfile,pathlib,sys
with zipfile.ZipFile(sys.argv[1]) as z,tempfile.TemporaryDirectory() as d:
 p=pathlib.Path(d)/'source.sqlite';p.write_bytes(z.read('db.sqlite'))
 db=sqlite3.connect('file:'+str(p)+'?mode=ro',uri=True)
 print(json.dumps({'schedule':db.execute('SELECT schedule FROM transactions WHERE id=?',(sys.argv[2],)).fetchone()[0],'tombstone':db.execute('SELECT tombstone FROM schedules WHERE id=?',(sys.argv[3],)).fetchone()[0]}))`,tempZip,ids.PAYMENT,ids.SCHEDULE],{encoding:'utf8'}));assert.equal(raw.schedule,ids.SCHEDULE);assert.equal(raw.tombstone,1);
 const zip=new URL('../tests/fixtures/schedule-deleted-link-26.10.zip',import.meta.url);if(!process.argv.includes('--check'))await writeFile(zip,original,{flag:'wx'});const archive_sha256=createHash('sha256').update(await readFile(zip)).digest('hex');

 await send('transactions-batch-update',{updated:[{id:ids.PAYMENT,schedule:null}]});await snapshot('historical link removed',null);
 const out={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',archive_sha256,raw_deleted_schedule_link:{schedule:'SCHEDULE',tombstone:raw.tombstone},snapshots};const file=new URL('../tests/fixtures/schedule-deleted-link.json',import.meta.url);if(process.argv.includes('--check'))assert.deepEqual(out,JSON.parse(await readFile(file,'utf8')));else await writeFile(file,JSON.stringify(out,null,2)+'\n',{flag:'wx'});console.log('PASS independent deleted-schedule unlink and unchanged payment');
}finally{await api.shutdown();await rm(root,{recursive:true,force:true});globalThis.Date=NativeDate;}
