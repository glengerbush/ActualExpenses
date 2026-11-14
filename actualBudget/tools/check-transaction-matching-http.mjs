import {cloneImportedTemplate} from '../web/tests/import-template.mjs';
// Full import/preview/retry outcomes against independently executed Actual.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {randomUUID,createHash} from 'node:crypto';
import {startServer} from '../web/tests/server.mjs';
const fixture=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-matching-source.json',import.meta.url),'utf8'));
const server=await startServer(),api=server.url.replace('actualbudget.test','127.0.0.1')+'/api/v1';
const failures=[];
const manifest=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-matching/manifest.json',import.meta.url)));
let tested=0;
async function call(path,body,headers={}){
  const response=await fetch(api+path,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json','Idempotency-Key':randomUUID(),...headers},...(body===undefined?{}:{body:JSON.stringify(body)})});
  assert.ok(response.ok,`${path}: ${response.status} ${await response.clone().text()}`);
  return response.json();
}
const quote=value=>`"${String(value??'').replaceAll('"','""')}"`;
function decimal(amount){const minor=BigInt(amount),positive=minor<0n?-minor:minor;return `${minor<0n?'-':''}${positive/100n}.${String(positive%100n).padStart(2,'0')}`;}
function financial(rows,manualIds,source){
  const output=rows.map(row=>({
    id:manualIds.has(row.id)?row.id:'<new unique transaction>',
    account_id:row[source?'account':'account_id'],date:row.date,amount:row.amount,
    payee_id:row[source?'payee':'payee_id']??null,category_id:row[source?'category':'category_id']??null,
    notes:source?'':row.notes??'',cleared:source?true:!!row.cleared,reconciled:source?false:!!row.reconciled,
    imported_id:row.imported_id??null,imported_payee:row.imported_payee??null,
  }));
  assert.equal(new Set(rows.map(row=>row.id)).size,rows.length,'Every distinct purchase retains a unique ID');
  return output.sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)));
}
try{
  for(const sourceItem of fixture.cases){
    const seed=manifest.archives.find(seed=>seed.id===sourceItem.id);assert.ok(seed);
    const replacements=new Map(Object.entries(sourceItem.ids).map(([key,value])=>[value,seed.ids[key]]));
    const item=JSON.parse(JSON.stringify(sourceItem),(_,value)=>typeof value==='string'&&replacements.has(value)?replacements.get(value):value);
    // Reconciliation no longer blocks importing bank identity onto this same row.
    if(item.id==='reconciled-match-unchanged'){
      item.preview.updated=[item.ids.locked];
      for(const state of [item.after,item.repeated])state.transactions.find(row=>row.id===item.ids.locked).imported_payee=item.input.incoming[0].imported_payee;
    }
    const selected=process.argv.find(arg=>arg.startsWith('--case='))?.slice(7);
    if(selected&&item.id!==selected)continue;
    tested++;
    try{
      const archive=await readFile(new URL('../tests/fixtures/transaction-matching/'+seed.filename,import.meta.url));assert.equal(createHash('sha256').update(archive).digest('hex'),seed.sha256);
      const uploadedSeed=await fetch(api+'/imports/actual',{method:'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/zip'},body:archive});assert.equal(uploadedSeed.status,201,await uploadedSeed.clone().text());
      const stageSeed=(await uploadedSeed.json()).data;assert.ok(['ready','activated'].includes(stageSeed.status));await call(`/imports/${stageSeed.id}/activate`,{});stageSeed.budget_id=await cloneImportedTemplate(api,stageSeed.budget_id);
      const account=item.ids.account,root=`/budgets/${stageSeed.budget_id}`,manualIds=new Set(item.input.existing.map(row=>item.ids[row.label]));
      assert.deepEqual(financial((await call(root+'/transactions')).data,manualIds,false),financial(item.before.transactions,manualIds,true),'Genuine source starting ledger reproduced');
      const csv=['Date,Payee,Amount,Memo',...item.input.incoming.map(row=>[row.date,row.payee_name,decimal(row.amount),row.notes].map(quote).join(','))].join('\n')+'\n';
      const bankFormat=item.input.incoming.some(row=>row.imported_id);
      const escape=text=>String(text??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;');
      const ofx='<OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>'+item.input.incoming.map(row=>'<STMTTRN><DTPOSTED>'+row.date.replaceAll('-','')+'</DTPOSTED><TRNAMT>'+decimal(row.amount)+'</TRNAMT><NAME>'+escape(row.payee_name)+'</NAME><MEMO>'+escape(row.notes)+'</MEMO>'+(row.imported_id?'<FITID>'+escape(row.imported_id)+'</FITID>':'')+'</STMTTRN>').join('')+'</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>';
      const upload=await fetch(`${api}${root}/accounts/${account}/transaction-imports?filename=bank.${bankFormat?'ofx':'csv'}`,{method:'POST',headers:{'Idempotency-Key':randomUUID(),'Content-Type':'application/octet-stream'},body:bankFormat?ofx:csv});
      assert.equal(upload.status,201,await upload.clone().text());
      const stage=(await upload.json()).data.id,path=`${root}/transaction-imports/${stage}`;
      let firstImportIds;
      for(const [round,expected] of [item.after,item.repeated].entries()){
        const checkpoint=(await call(root+'/checkpoint')).data,before=await call(root+'/transactions');
        const body={snapshot:checkpoint.snapshot,mapping:{date:'Date',payee:'Payee',amount:'Amount',notes:'Memo'},options:{has_header_row:true,date_format:'yyyy mm dd',reimport_deleted:item.input.opts?.reimportDeleted??false}};
        const preview=(await call(path+'/preview',body)).data;
        assert.deepEqual(preview.errors,[]);
        assert.deepEqual(await call(root+'/transactions'),before,'Preview leaves ledger unchanged');
        assert.deepEqual((await call(root+'/checkpoint')).data,checkpoint,'Preview leaves checkpoint unchanged');
        if(round===0){
          // Source force-add is an explicit review choice in this public API,
          // applied below; an offered candidate is not an accepted merge.
          const matches=preview.rows.filter(row=>row.match_id&&!item.input.incoming[row.row].forceAddTransaction).map(row=>row.match_id);
          assert.deepEqual(matches,item.preview.updated,'Same source matching allocation/order');
        }
        const rows=preview.rows.map(row=>({row:row.row,action:item.input.incoming[row.row].forceAddTransaction?'add':row.match_id?'merge':'add',...(row.match_id?{match_id:row.match_id}:{})}));
        const command={...body,rows,remember:false};
        const headers={'Idempotency-Key':randomUUID(),'X-Actual-Budget-Epoch':checkpoint.epoch};
        const receipt=await call(path+'/commit',command,headers);
        const after=await call(root+'/transactions');
        assert.deepEqual(financial(after.data,manualIds,false),financial(expected.transactions,manualIds,true),`Source ledger after import ${round+1}`);
        if(round===0)firstImportIds=after.data.map(row=>row.id);
        else{
          assert.ok(item.after.transactions.every(row=>item.repeated.transactions.some(later=>later.id===row.id)),'Source case retains every first-import identity');
          assert.ok(firstImportIds.every(id=>after.data.some(row=>row.id===id)),'Repeated import preserves existing purchase identities');
        }
        assert.deepEqual(await call(path+'/commit',command,headers),receipt,'Lost-ack retry returns exact receipt despite stale snapshot');
        assert.deepEqual(await call(root+'/transactions'),after,'Receipt retry never duplicates a purchase');
      }
      console.log(`PASS ${item.id}`);
    }catch(error){failures.push(item.id);console.error(`FAIL ${item.id}: ${error.message}`);}
  }
  assert.ok(tested,'Selection must exercise a fixture');
  assert.equal(failures.length,0,`${failures.length}/${tested} matching cases failed`);
  console.log(`PASS ${tested} genuine financial matching outcomes through public API`);
}finally{await server.stop();}
