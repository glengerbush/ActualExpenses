// Compare source financial normalization with the running import preview.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const fixture=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-normalization-source.json',import.meta.url),'utf8'));
const server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
async function response(path,body,headers={}){return fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),...headers},...(body===undefined?{}:{body:JSON.stringify(body)})});}
async function call(path,body){const result=await response(path,body);assert.ok(result.ok,`${path}: ${result.status} ${await result.clone().text()}`);return result.json();}
const excludedSelection=new Set(['unselected','unselected-reconcile-disabled','matched-display-row','force-add-existing','force-add-ignored']);
const quote=value=>`"${String(value??'').replaceAll('"','""')}"`;
const failures=[];
let tested=0;
try{
  const budget=randomUUID(),account=randomUUID(),payee=randomUUID(),root=`/budgets/${budget}`;
  await call('/budgets',{id:budget,name:'Import normalization parity'});
  await call(root+'/accounts',{id:account,name:'Checking',opening_balance:0,opening_date:'2026-01-01'});
  await call(root+'/payees',{id:payee,name:'Cafe'});
  const categories=fixture.cases[0].input.categories;
  for(const category of categories)await call(root+'/categories',{id:category.id,name:category.name,is_income:false});
  const before=await call(root+'/transactions'),checkpoint=await call(root+'/checkpoint');
  const references=await Promise.all([call(root+'/payees'),call(root+'/categories')]);
  for(const item of fixture.cases){
    // Raw file parsers and reviewed selection actions have separate consumers.
    if(item.id.startsWith('format-')||excludedSelection.has(item.id))continue;
    const selected=process.argv.find(arg=>arg.startsWith('--case='))?.slice(7);
    if(selected&&item.id!==selected)continue;
    tested++;
    try{
      const columns=[...new Set(Object.values(item.input.fieldMappings))];
      const csv=[columns.map(quote).join(','),...item.input.transactions.map(row=>columns.map(key=>quote(row[key])).join(','))].join('\n')+'\n';
      const upload=await fetch(`${api}${root}/accounts/${account}/transaction-imports?filename=bank.csv`,{method:'POST',headers:{'Content-Type':'text/csv','Idempotency-Key':randomUUID()},body:csv});
      assert.equal(upload.status,201,await upload.clone().text());
      const stage=(await upload.json()).data.id;
      const mapping=Object.fromEntries(Object.entries(item.input.fieldMappings).map(([key,value])=>[key==='inOut'?'in_out':key,value]));
      const names={parseDateFormat:'date_format',splitMode:'split_mode',inOutMode:'in_out_mode',outValue:'out_value',flipAmount:'flip_amount',multiplierAmount:'multiplier',clearOnImport:'clear_on_import',importNotes:'import_notes',reconcile:'reconcile'};
      const options={has_header_row:true,...Object.fromEntries(Object.entries(names).map(([key,value])=>[value,item.input[key]]))};
      const body={snapshot:checkpoint.data.snapshot,mapping,options};
      const preview=(await call(`${root}/transaction-imports/${stage}/preview`,body)).data;
      assert.equal(preview.errors.length>0,!!item.result.error,'Same invalid-date/amount decision as Actual');
      if(item.result.error){
        const rejected=await response(`${root}/transaction-imports/${stage}/commit`,{...body,rows:item.input.transactions.map((_,row)=>({row,action:'add'}))},{'X-Actual-Budget-Epoch':checkpoint.data.epoch});
        assert.equal(rejected.status,400,`Invalid batch must reject atomically: ${await rejected.clone().text()}`);
      }else{
        assert.equal(preview.rows.length,item.result.transactions.length);
        for(const [index,row] of preview.rows.entries()){
          const expected=item.result.transactions[index],actual=row.transaction;
          for(const field of ['date','amount'])assert.deepEqual(actual[field],expected[field],field);
          assert.equal(actual.notes,'','notes');assert.equal(actual.cleared,true,'Every planned entry is complete');
          assert.equal(actual.category_id,expected.category??null,'exact category association');
          assert.equal(actual.payee_id,payee,'mapped existing payee');
          assert.equal(row.match_id,null,'Empty ledger offers no invented match');
        }
      }
      assert.deepEqual(await call(root+'/transactions'),before,'No financial mutation from preview or failed commit');
      assert.deepEqual(await call(root+'/checkpoint'),checkpoint,'No checkpoint advance');
      assert.deepEqual(await Promise.all([call(root+'/payees'),call(root+'/categories')]),references,'No reference mutation');
      console.log(`PASS ${item.id}`);
    }catch(error){failures.push(item.id);console.error(`FAIL ${item.id}: ${error.message}`);}
  }
  assert.ok(tested,'Selection must exercise a fixture');
  assert.equal(failures.length,0,`${failures.length}/${tested} normalization cases failed`);
  console.log(`PASS ${tested} source financial normalization cases through public API`);
}finally{await server.stop();}
