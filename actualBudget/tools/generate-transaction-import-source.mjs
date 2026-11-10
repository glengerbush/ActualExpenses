// Execute pinned Actual parser, unchanged frontend normalization and real import.
// No replacement importer/parser/matcher is loaded to create expectations.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {loadSourceNormalization} from './transaction-import-source-runtime.mjs';

process.env.TZ='UTC';
const NativeDate=Date, clock='2026-11-09T12:00:00Z';
globalThis.Date=class extends NativeDate {
  constructor(...args){super(...(args.length?args:[clock]));}
  static now(){return NativeDate.parse(clock);}
};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const digest=value=>createHash('sha256').update(value).digest('hex');
const {finalize:finalizer,sourceFiles}=await loadSourceNormalization();
const plain=value=>JSON.parse(JSON.stringify(value));
const directory=await mkdtemp(tmpdir()+'/transaction-import-oracle-');
const fixture=new URL('../tests/fixtures/transaction-import-manual.json',import.meta.url);
const archivePath=new URL('../tests/fixtures/transaction-import-manual-base-26.10.zip',import.meta.url);
const check=process.argv.includes('--check');
try{
  const engine=await api.init({dataDir:directory});
  const send=(name,body)=>engine.send(name,body);
  const ids={manual:'25260000-0000-4000-8000-000000000001'};
  await api.runImport('CSV manual matching source',async()=>{
    ids.account=await api.createAccount({name:'Checking'},0);
    ids.payee=await api.createPayee({name:'Cafe'});
    ids.group=await api.createCategoryGroup({name:'Expenses',is_income:false});
    ids.category=await api.createCategory({name:'Food',group_id:ids.group,is_income:false});
    await api.addTransactions(ids.account,[{id:ids.manual,date:'2026-11-09',amount:-1234,payee:ids.payee,category:ids.category,notes:'Keep manual note',cleared:false}]);
  });
  const snapshot=async()=>plain({transactions:await api.getTransactions(ids.account,'2026-01-01','2026-12-31'),payees:await api.getPayees(),categories:await api.getCategories()});
  const before=await snapshot();
  assert.equal(before.transactions.length,1);
  assert.equal(before.transactions[0].cleared,false);
  const archive=check?await readFile(archivePath):await api.exportBudget();
  const input='Date,Payee,Amount,Memo\n2026-11-09,Cafe,-12.34,Imported memo\n';
  const filepath=directory+'/bank.csv';
  await writeFile(filepath,input);
  const parseOptions={hasHeaderRow:true,delimiter:',',encoding:'auto',skipStartLines:0,skipEndLines:0};
  const parsed=plain(await send('transactions-parse-file',{filepath,options:parseOptions}));
  assert.deepEqual(parsed.errors,[]);
  const mapping={date:'Date',payee:'Payee',amount:'Amount',notes:'Memo',category:null,inOut:null,outflow:null,inflow:null};
  const normalized=plain(finalizer({
    transactions:parsed.transactions.map((row,index)=>({...row,trx_id:String(index),selected:true,selected_merge:true,existing:false,ignored:false})),
    reconcile:true,fieldMappings:mapping,filetype:'csv',parseDateFormat:'yyyy mm dd',splitMode:false,inOutMode:false,outValue:'',flipAmount:false,multiplierAmount:'1',categories:before.categories,clearOnImport:true,importNotes:true,
  }));
  assert.equal(normalized.error,null);
  assert.equal(normalized.transactions[0].amount,-1234);
  const request={accountId:ids.account,transactions:normalized.transactions,opts:{reimportDeleted:false}};
  const preview=plain(await send('transactions-import',{...request,isPreview:true}));
  assert.deepEqual(preview.errors,[]);
  assert.deepEqual(await snapshot(),before,'Real import preview must leave ledger and references unchanged');
  const first=plain(await send('transactions-import',{...request,isPreview:false}));
  assert.deepEqual(first.errors,[]);
  const after=await snapshot();
  assert.equal(after.transactions.length,1);
  assert.equal(after.transactions[0].id,ids.manual);
  assert.equal(after.transactions[0].notes,'Keep manual note');
  assert.equal(after.transactions[0].amount,-1234);
  assert.equal(after.transactions[0].cleared,true);
  const second=plain(await send('transactions-import',{...request,isPreview:false}));
  assert.deepEqual(second.errors,[]);
  assert.deepEqual(await snapshot(),after,'Repeated genuine CSV import must not add a second purchase');
  const groups=(await api.getCategoryGroups()).map(group=>({...group,categories:after.categories.filter(c=>c.group_id===group.id)}));
  const exported=await send('transactions-export',{transactions:after.transactions,accounts:await api.getAccounts(),categoryGroups:groups,payees:after.payees});
  assert.equal(typeof exported,'string');
  const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,
    source_files:sourceFiles,
    ids,input:{filename:'bank.csv',text:input,sha256:digest(input),parse_options:parseOptions,mapping,date_format:'yyyy mm dd',clear_on_import:true,import_notes:true},
    archive:{filename:'transaction-import-manual-base-26.10.zip',sha256:digest(archive)},parsed,normalized,before,preview,first,after,second,exported};
  function stable(value,record){
    const references=new Map(Object.entries(record.ids).map(([label,id])=>[id,label]));
    for(const row of record.before.payees)if(!references.has(row.id))references.set(row.id,'payee:'+row.name);
    for(const row of record.before.categories)if(!references.has(row.id))references.set(row.id,'category:'+row.name);
    function walk(v){if(typeof v==='string')return references.get(v)??v;if(Array.isArray(v))return v.map(walk);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,val])=>[k,walk(val)]));return v;}
    return walk(value);
  }
  if(check){const expected=JSON.parse(await readFile(fixture,'utf8'));assert.deepEqual(stable(output,output),stable(expected,expected));}
  else{await writeFile(archivePath,archive,{flag:'wx'});await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});}
  console.log('PASS genuine CSV parser, frontend normalization, read-only matching, manual identity/notes, cleared state, repeat import and source CSV export');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
