import {cloneImportedTemplate} from '../web/tests/import-template.mjs';
// Independent Actual import snapshots; previews must never create references.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID,createHash} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const fixture=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-import-rules-source.json',import.meta.url)));
const server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
async function call(path,body,headers={}){const r=await fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),...headers},...(body===undefined?{}:{body:JSON.stringify(body)})});assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json();}
function shape(rows,payees,categories,source){
 const names=new Map(payees.map(row=>[row.id,row.name])),cats=new Map(categories.map(row=>[row.id,row.name]));
 const flat=rows.flatMap(row=>[row,...(source?row.subtransactions??[]:row.children??[])]);
 return flat.map(row=>({date:row.date,amount:row.amount,notes:row.notes??'',cleared:!!row.cleared,reconciled:!!row.reconciled,is_parent:!!row.is_parent,parent:!!row.parent_id,payee:names.get(row[source?'payee':'payee_id'])??null,category:cats.get(row[source?'category':'category_id'])??null,imported_payee:row.imported_payee??null})).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
try{for(const item of fixture.cases){
 const bytes=await readFile(new URL('../tests/fixtures/'+item.archive.filename,import.meta.url));assert.equal(createHash('sha256').update(bytes).digest('hex'),item.archive.sha256);
 const upload=await fetch(api+'/imports/actual',{method:'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/zip'},body:bytes});assert.equal(upload.status,201,await upload.clone().text());
 const staged=(await upload.json()).data;assert.ok(['ready','activated'].includes(staged.status));await call(`/imports/${staged.id}/activate`,{});staged.budget_id=await cloneImportedTemplate(api,staged.budget_id);
 const root=`/budgets/${staged.budget_id}`,account=item.ids.account;
 const before=await call(root+'/transactions');
 assert.deepEqual(shape(before.data,(await call(root+'/payees')).data,(await call(root+'/categories')).data,false),shape(item.before.transactions,item.before.payees,item.before.categories,true));
 const csv='Date,Payee,Amount,Memo\n'+item.input.transactions.map(row=>[row.date,row.payee_name,(row.amount/100).toFixed(2),row.notes].join(',')).join('\n')+'\n';
 const file=await fetch(`${api}${root}/accounts/${account}/transaction-imports?filename=bank.csv`,{method:'POST',headers:{'Idempotency-Key':randomUUID()},body:csv});assert.equal(file.status,201);const path=root+'/transaction-imports/'+(await file.json()).data.id;
 let firstIds;
 for(const [round,expected] of [item.after,item.repeated].entries()){
  const cp=(await call(root+'/checkpoint')).data,refs=await call(root+'/payees'),financial=await call(root+'/transactions');
  const body={snapshot:cp.snapshot,mapping:{date:'Date',payee:'Payee',amount:'Amount',notes:'Memo'},options:{has_header_row:true}};
  const preview=(await call(path+'/preview',body)).data;assert.deepEqual(preview.errors,[]);assert.deepEqual(await call(root+'/payees'),refs);assert.deepEqual(await call(root+'/transactions'),financial);assert.deepEqual((await call(root+'/checkpoint')).data,cp);
  const command={...body,rows:preview.rows.map(row=>({row:row.row,action:row.match_id?'merge':'add',...(row.match_id?{match_id:row.match_id}:{})}))};
  const headers={'Idempotency-Key':randomUUID(),'X-Actual-Budget-Epoch':cp.epoch};const result=await call(path+'/commit',command,headers);
  const rows=(await call(root+'/transactions')).data,payees=(await call(root+'/payees')).data,categories=(await call(root+'/categories')).data;
  assert.deepEqual(shape(rows,payees,categories,false),shape(expected.transactions,expected.payees,expected.categories,true),item.id);
  assert.deepEqual(payees.map(p=>p.name).sort(),expected.payees.filter(p=>!p.transfer_acct).map(p=>p.name).sort(),'Only used payees persist');
  const ids=rows.flatMap(row=>[row,...(row.children??[])]).map(row=>row.id).sort();
  if(round===0)firstIds=ids;else assert.deepEqual(ids,firstIds,'Repeat preserves every purchase and child ID');
  assert.deepEqual(await call(path+'/commit',command,headers),result,'Original receipt survives stale snapshot');
 }
 console.log('PASS '+item.id);
}}finally{await server.stop();}
