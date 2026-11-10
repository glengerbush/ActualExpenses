import {cloneImportedTemplate} from '../web/tests/import-template.mjs';
// The first normal export is captured verbatim from Actual's real CSV writer.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const {parse:parseCSV}=require('csv-parse/sync');
import {randomUUID,createHash} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const f=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-import-manual.json',import.meta.url))),server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
async function call(path,body){const r=await fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)})});assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json();}
try{
 const bytes=await readFile(new URL('../tests/fixtures/'+f.archive.filename,import.meta.url));assert.equal(createHash('sha256').update(bytes).digest('hex'),f.archive.sha256);const r=await fetch(api+'/imports/actual',{method:'POST',headers:{'Content-Type':'application/zip','Idempotency-Key':randomUUID()},body:bytes});assert.equal(r.status,201);const stage=(await r.json()).data;await call(`/imports/${stage.id}/activate`,{});stage.budget_id=await cloneImportedTemplate(api,stage.budget_id);const root=`/budgets/${stage.budget_id}`;
 const upload=await fetch(`${api}${root}/accounts/${f.ids.account}/transaction-imports?filename=bank.csv`,{method:'POST',headers:{'Idempotency-Key':randomUUID()},body:f.input.text});assert.equal(upload.status,201);const path=root+'/transaction-imports/'+(await upload.json()).data.id;const cp=(await call(root+'/checkpoint')).data;
 const mapping=Object.fromEntries(Object.entries(f.input.mapping).map(([key,value])=>[key==='inOut'?'in_out':key,value]));const body={snapshot:cp.snapshot,mapping,options:{has_header_row:true},rows:[{row:0,action:'merge',match_id:f.ids.manual}]};const committed=await fetch(api+path+'/commit',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),'X-Actual-Budget-Epoch':cp.epoch},body:JSON.stringify(body)});assert.equal(committed.status,200,await committed.clone().text());
 const before=await call(root+'/transactions'),checkpoint=(await call(root+'/checkpoint')).data;
 const exported=await fetch(api+root+'/transactions/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({snapshot:checkpoint.snapshot,account_id:f.ids.account,layout:'standard'})});assert.equal(exported.status,200,await exported.clone().text());assert.match(exported.headers.get('Content-Type'),/^text\/csv/);assert.equal(await exported.text(),f.exported,'Exact independent writer bytes, escaping, minor-to-major values and status cells');assert.deepEqual(await call(root+'/transactions'),before);assert.deepEqual((await call(root+'/checkpoint')).data,checkpoint);
 console.log('PASS genuine normal CSV export with no financial or checkpoint mutation');
 const source=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-export-source.json',import.meta.url)));
 const original=await readFile(new URL('../tests/fixtures/'+source.archive.filename,import.meta.url));assert.equal(createHash('sha256').update(original).digest('hex'),source.archive.sha256);
 const uploaded=await fetch(api+'/imports/actual',{method:'POST',headers:{'Content-Type':'application/zip','Idempotency-Key':randomUUID()},body:original});assert.equal(uploaded.status,201);const stage2=(await uploaded.json()).data;assert.ok(['ready','activated'].includes(stage2.status));await call(`/imports/${stage2.id}/activate`,{});stage2.budget_id=await cloneImportedTemplate(api,stage2.budget_id);const root2=`/budgets/${stage2.budget_id}`;
 const financial=await call(root2+'/transactions'),saved=(await call(root2+'/checkpoint')).data;
 const failures=[];
 for(const item of source.cases){
  const response=await fetch(api+root2+'/transactions/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({snapshot:saved.snapshot,...item.request})});
  try{assert.equal(response.status,200,await response.clone().text());const csv=await response.text();const rawCell="'\rTransfer note",correctedCell="\"'\rTransfer note\"";const expected=item.csv.replace(','+rawCell+',',','+correctedCell+',');assert.equal(csv,expected,'Source bytes exact except independently specified raw-CR cell quoting');const originalRows=parseCSV(item.csv,{columns:true,record_delimiter:'\n'}),actualRows=parseCSV(csv,{columns:true});assert.deepEqual(actualRows,originalRows,'Independent parser preserves every row/cell');if(item.csv.includes(','+rawCell+',')){assert.equal(actualRows.filter(row=>row.Notes===rawCell).length,1,'The exact CR note and formula-protection prefix survive as one cell');}assert.deepEqual(await call(root2+'/transactions'),financial);assert.deepEqual((await call(root2+'/checkpoint')).data,saved);console.log('PASS '+item.id);}catch(error){failures.push(item.id);console.error('FAIL '+item.id+': '+error.message);}
 }
 assert.equal(failures.length,0,failures.join(','));
 for(const invalid of [{layout:'unknown'},{ids:[source.ids.first,source.ids.first]},{ids:Array.from({length:10001},()=>randomUUID())},{query:{filter:{conditions:[{field:'unsupported',op:'eq',value:'x'}]}}}]){
  const rejected=await fetch(api+root2+'/transactions/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({snapshot:saved.snapshot,...invalid})});assert.equal(rejected.status,400);assert.deepEqual(await call(root2+'/transactions'),financial);assert.deepEqual((await call(root2+'/checkpoint')).data,saved);
 }
 const changed=await fetch(api+root2+'/transactions/'+source.ids.first,{method:'PATCH',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID()},body:JSON.stringify({notes:'a'.repeat(5000)+'!'})});assert.equal(changed.status,200);
 const currentFinancial=await call(root2+'/transactions'),currentCheckpoint=(await call(root2+'/checkpoint')).data;
 const stale=await fetch(api+root2+'/transactions/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({snapshot:saved.snapshot})});assert.equal(stale.status,409);
 const bounded=await fetch(api+root2+'/transactions/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({snapshot:currentCheckpoint.snapshot,query:{filter:{conditions:[{field:'notes',op:'matches',value:'^(a+)+$'}]}}})});assert.equal(bounded.status,400);assert.equal((await bounded.json()).error.code,'regex_work_limit','Exhausted shared work refuses instead of emitting a partial/empty CSV');assert.deepEqual(await call(root2+'/transactions'),currentFinancial);assert.deepEqual((await call(root2+'/checkpoint')).data,currentCheckpoint);
 console.log('PASS export selection/bounds/stale/shared-regex refusals preserve state');
}finally{await server.stop();}
