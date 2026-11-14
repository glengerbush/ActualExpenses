// Structured normalization reads independently captured parser/finalizer outputs.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const parsers=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-parser-source.json',import.meta.url))),normalized=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-normalization-source.json',import.meta.url)));
const server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
async function call(path,body){const r=await fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID()},...(body===undefined?{}:{body:JSON.stringify(body)})});assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json();}
const failures=[];let tested=0;
try{const b=randomUUID(),a=randomUUID(),root=`/budgets/${b}`;await call('/budgets',{id:b,name:'Structured import source parity'});await call(root+'/accounts',{id:a,name:'Checking',opening_balance:0,opening_date:'1900-01-01'});const cp=await call(root+'/checkpoint'),ledger=await call(root+'/transactions'),refs=await call(root+'/payees');
for(const item of normalized.cases.filter(c=>c.id.startsWith('format-'))){tested++;try{
 const raw=parsers.cases.find(c=>c.id===item.id.slice(7));assert.ok(raw);
 const upload=await fetch(`${api}${root}/accounts/${a}/transaction-imports?filename=${encodeURIComponent(raw.filename)}`,{method:'POST',headers:{'Idempotency-Key':randomUUID()},body:Buffer.from(raw.bytes_base64,'base64')});assert.equal(upload.status,201,await upload.clone().text());const stage=(await upload.json()).data.id;
 const names={hasHeaderRow:'has_header_row',delimiter:'delimiter',fallbackMissingPayeeToMemo:'fallback_missing_payee_to_memo',swapPayeeAndMemo:'swap_payee_and_memo',skipStartLines:'skip_start_lines',skipEndLines:'skip_end_lines',importNotes:'import_notes',encoding:'encoding'};
 const options={...Object.fromEntries(Object.entries(raw.options).map(([key,value])=>[names[key],value])),date_format:item.input.parseDateFormat,clear_on_import:item.input.clearOnImport};
 const result=(await call(root+`/transaction-imports/${stage}/preview`,{snapshot:cp.data.snapshot,options})).data;
 assert.equal(result.errors.length>0,!!item.result.error,'Same normalization refusal');
 if(!item.result.error){assert.equal(result.rows.length,item.result.transactions.length);for(const [i,row]of result.rows.entries()){const expected=item.result.transactions[i];for(const field of ['date','amount','cleared'])assert.deepEqual(row.transaction[field],expected[field],`${i}:${field}`);assert.equal(row.transaction.notes,'');assert.equal(row.transaction.imported_id??null,expected.imported_id??null);assert.equal(row.transaction.imported_payee??null,expected.imported_payee??null);assert.equal(row.transaction.category_id,expected.category??null);}}
 assert.deepEqual(await call(root+'/checkpoint'),cp);assert.deepEqual(await call(root+'/transactions'),ledger);assert.deepEqual(await call(root+'/payees'),refs);console.log('PASS '+item.id);
}catch(e){failures.push(item.id);console.error('FAIL '+item.id+': '+e.message);}}
assert.ok(tested);assert.equal(failures.length,0,failures.join(','));console.log(`PASS ${tested} genuine structured finalization observations`);
}finally{await server.stop();}
