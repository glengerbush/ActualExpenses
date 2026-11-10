import {cloneImportedTemplate} from '../web/tests/import-template.mjs';
// Manual merge expectations are complete persisted Actual graphs, not new math.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID,createHash} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const fixture=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-merge-source.json',import.meta.url))),server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
async function response(path,body,headers={}){return fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),...headers},...(body===undefined?{}:{body:JSON.stringify(body)})});}
async function call(path,body,headers={}){const r=await response(path,body,headers);assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json();}
function financial(rows,source){const flat=rows.flatMap(row=>source?[row]:[row,...(row.children??[])]);const unique=new Map(flat.map(row=>[row.id,row]));return [...unique.values()].map(row=>({id:row.id,account:row[source?'account':'account_id'],amount:row.amount,date:row.date,payee:row[source?'payee':'payee_id']??null,category:row[source?'category':'category_id']??null,notes:row.notes??'',cleared:!!row.cleared,reconciled:!!row.reconciled,is_parent:!!row.is_parent,parent_id:row.parent_id??null,transfer_id:row.transfer_id??null,schedule:row[source?'schedule':'schedule_id']??null,imported_id:row.imported_id??null,imported_payee:row.imported_payee??null,raw_synced_data:row.raw_synced_data??null,starting_balance:!!row[source?'starting_balance_flag':'starting_balance']})).sort((a,b)=>a.id.localeCompare(b.id));}
const failures=[];let tested=0;
try{for(const item of fixture.cases){const filter=process.argv.find(arg=>arg.startsWith('--case='))?.slice(7);if(filter&&item.id!==filter)continue;tested++;try{
 const bytes=await readFile(new URL('../tests/fixtures/'+item.archive.filename,import.meta.url));assert.equal(createHash('sha256').update(bytes).digest('hex'),item.archive.sha256);
 const r=await fetch(api+'/imports/actual',{method:'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/zip'},body:bytes});assert.equal(r.status,201,await r.clone().text());const stage=(await r.json()).data;assert.ok(['ready','activated'].includes(stage.status));await call(`/imports/${stage.id}/activate`,{});stage.budget_id=await cloneImportedTemplate(api,stage.budget_id);const root=`/budgets/${stage.budget_id}`;
 const before=await call(root+'/transactions'),cp=(await call(root+'/checkpoint')).data;assert.deepEqual(financial(before.data,false),financial(item.before.transactions,true),'Exact genuine starting graph');
 const body={snapshot:cp.snapshot,ids:[item.ids.a,item.input.duplicate?item.ids.a:item.ids.b]},owner=randomUUID(),headers={'X-Actual-Budget-Epoch':cp.epoch,'X-Actual-History-Owner':owner};
 const previewResponse=await response(root+'/transactions/merge/preview',body);
 if(item.error){assert.equal(previewResponse.status,400,await previewResponse.clone().text());const rejected=await response(root+'/transactions/merge',body,headers);assert.equal(rejected.status,400);assert.deepEqual(await call(root+'/transactions'),before);assert.deepEqual((await call(root+'/checkpoint')).data,cp);console.log('PASS '+item.id);continue;}
 assert.equal(previewResponse.status,200,await previewResponse.clone().text());const preview=(await previewResponse.json()).data;assert.equal(preview.keeper_id,item.kept);assert.deepEqual(financial(preview.after,false),financial(item.after.transactions,true),'Preview exposes full persisted resulting graph');assert.deepEqual(await call(root+'/transactions'),before);assert.deepEqual((await call(root+'/checkpoint')).data,cp);
 if(preview.protected_count){const refused=await response(root+'/transactions/merge',body,headers);assert.equal(refused.status,409,'Locked graph needs captured explicit confirmation');assert.deepEqual(await call(root+'/transactions'),before);body.protection_token=preview.protection_token;}
 const wire={...headers,'Idempotency-Key':randomUUID()};const result=await call(root+'/transactions/merge',body,wire);assert.equal(result.data.keeper_id,item.kept);const after=await call(root+'/transactions');assert.deepEqual(financial(after.data,false),financial(item.after.transactions,true));assert.deepEqual(await call(root+'/transactions/merge',body,wire),result);assert.deepEqual(await call(root+'/transactions'),after);
 const history=(await call(root+'/history',undefined,headers)).data;assert.ok(history.undo?.id,'Whole connected merge has one undo action');const action=history.undo.id;
 await call(root+'/history/undo',{action_id:action},headers);assert.equal((await call(root+'/history',undefined,headers)).data.undo,null,'No second partial merge action remains');assert.deepEqual(financial((await call(root+'/transactions')).data,false),financial(item.before.transactions,true),'Undo restores every original parent/child/peer and bank identity');await call(root+'/history/redo',{action_id:action},headers);assert.deepEqual(financial((await call(root+'/transactions')).data,false),financial(item.after.transactions,true));assert.deepEqual(await call(root+'/transactions/merge',body,wire),result,'Original merge receipt survives undo/redo');
 if(item.id==='earlier-kept'){
  const edit=await fetch(api+root+'/transactions/'+item.kept,{method:'PATCH',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),'X-Actual-Budget-Epoch':cp.epoch},body:JSON.stringify({notes:'Independent newer note'})});assert.equal(edit.status,200,await edit.clone().text());
  await call(root+'/history/undo',{action_id:action},headers);
  const expected=financial(item.before.transactions,true).map(row=>row.id===item.kept?{...row,notes:'Independent newer note'}:row);
  const current=await call(root+'/transactions');assert.deepEqual(financial(current.data,false),expected,'Unrelated ordinary notes survive the permitted inverse');assert.deepEqual(await call(root+'/transactions/merge',body,wire),result);assert.deepEqual(await call(root+'/transactions'),current);
 }
 console.log('PASS '+item.id);
}catch(error){failures.push(item.id);console.error('FAIL '+item.id+': '+error.message);}}
assert.ok(tested);assert.equal(failures.length,0,failures.join(','));console.log(`PASS ${tested} genuine manual merge graph workflows`);
}finally{await server.stop();}
