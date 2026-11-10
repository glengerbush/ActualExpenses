// Public running-API consumer of independently recorded Actual parser results.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID,createHash} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';

const fixture=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-parser-source.json',import.meta.url),'utf8'));
const server=await startServer();
const api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
const failures=[];
let tested=0;
async function call(path,body){
  const response=await fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID()},...(body===undefined?{}:{body:JSON.stringify(body)})});
  assert.ok(response.ok,`${path}: ${response.status} ${await response.clone().text()}`);
  return await response.json();
}
try{
  const budget=randomUUID(),account=randomUUID(),root=`/budgets/${budget}`;
  await call('/budgets',{id:budget,name:'Source parser parity'});
  await call(root+'/accounts',{id:account,name:'Checking',opening_balance:0,opening_date:'2026-01-01'});
  const before=await call(root+'/transactions'),checkpoint=await call(root+'/checkpoint');
  for(const item of fixture.cases){
    if(process.argv.includes('--csv')&&!/\.(csv|tsv)$/.test(item.filename))continue;
    const selected=process.argv.find(arg=>arg.startsWith('--case='))?.slice(7);
    if(selected&&item.id!==selected)continue;
    tested++;
    try{
      const bytes=Buffer.from(item.bytes_base64,'base64');
      assert.equal(createHash('sha256').update(bytes).digest('hex'),item.sha256);
      const uploaded=await fetch(`${api}${root}/accounts/${account}/transaction-imports?filename=${encodeURIComponent(item.filename)}`,{method:'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/octet-stream'},body:bytes});
      if(item.filename.endsWith('.txt')){
        assert.equal(uploaded.status,400,'Unsupported file extension rejected before parsing');
      }else{
        assert.equal(uploaded.status,201,`${uploaded.status} ${await uploaded.clone().text()}`);
        const stage=(await uploaded.json()).data.id;
        const options=Object.fromEntries(Object.entries(item.options).map(([key,value])=>[key.replace(/[A-Z]/g,c=>'_'+c.toLowerCase()),value]));
        const parsed=(await call(`${root}/transaction-imports/${stage}/parse`,{options})).data;
        assert.ok(Array.isArray(parsed.errors),'Public parse returns visible errors');
        assert.equal(parsed.errors.length>0,item.result.errors.length>0,'Source error versus success');
        if(item.result.errors.length){
          assert.ok(parsed.errors.every(error=>typeof error.message==='string'&&error.message.trim()),'Errors need readable messages');
        }
        assert.deepEqual(parsed.transactions??[],item.result.transactions??[],'All source fields/order/values retained, including rejected input');
      }
      assert.deepEqual(await call(root+'/transactions'),before,'Parsing never mutates ledger or balances');
      assert.deepEqual(await call(root+'/checkpoint'),checkpoint,'Parsing never advances financial checkpoint');
      console.log(`PASS ${item.id}`);
    }catch(error){failures.push({id:item.id,error:error.message});console.error(`FAIL ${item.id}: ${error.message}`);}
  }
  assert.ok(tested,'Selection must exercise at least one fixture');
  assert.equal(failures.length,0,`${failures.length}/${tested} parser cases failed`);
  console.log(`PASS ${tested} independent source cases through public API`);
}finally{await server.stop();}
