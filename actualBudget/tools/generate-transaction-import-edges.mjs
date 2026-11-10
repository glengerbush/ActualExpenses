// Execute pinned frontend finalization and the real import writer for selection,
// equal-bank-ID and observable new-row ordering. No replacement algorithms.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm,mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {loadSourceNormalization} from './transaction-import-source-runtime.mjs';
process.env.TZ='UTC';const NativeDate=Date,clock='2026-11-09T12:00:00Z';globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/import-edges-'),check=process.argv.includes('--check');const destination=new URL('../tests/fixtures/transaction-import-edges/',import.meta.url);const fixture=new URL('../tests/fixtures/transaction-import-edges.json',import.meta.url);const hash=bytes=>createHash('sha256').update(bytes).digest('hex'),plain=value=>JSON.parse(JSON.stringify(value));
const {finalize,sourceFiles}=await loadSourceNormalization((key,args)=>JSON.stringify({key,args}));
const row=(amount=-12.34,payee_name='Cafe',notes='Bank note',extra={})=>({date:'2026-11-09',amount,payee_name,imported_payee:payee_name,notes,selected:true,selected_merge:true,existing:false,ignored:false,...extra});
const cases=[
 {id:'file-order',existing:null,rows:[row(-1,'Cafe','First'),row(-2,'Cafe','Second'),row(-3,'Cafe','Third')]},
 {id:'same-bank-id-twice',existing:{notes:'',imported_id:'bank-1'},rows:[row(-12.34,'Cafe','First',{imported_id:'bank-1'}),row(-12.34,'Shop','Second',{imported_id:'bank-1'})]},
 {id:'matched-split-status',existing:{notes:'',split:true},rows:[row()]},
 {id:'selected-scope-rematch',existing:{notes:''},rows:[row(-12.34,'Cafe','First',{selected:false,existing:true}),row(-12.34,'Cafe','Second')]},
 {id:'unselected-ignored-still-consumes-match',existing:{notes:'Bank note',cleared:true,imported_payee:'Cafe'},rows:[row(-12.34,'Cafe','Bank note',{selected:false,ignored:true}),row(-12.34,'Shop','Second')]},
];
try{const engine=await api.init({dataDir:directory});await mkdir(destination,{recursive:true});const observations=[];
 for(const [index,item]of cases.entries()){
  const ids={manual:`25290000-0000-4000-8000-${String(index).padStart(12,'0')}`};
  await api.runImport(item.id,async()=>{ids.account=await api.createAccount({name:'Checking'},0);for(const name of ['Cafe','Shop'])ids[name]=await api.createPayee({name});if(item.existing){const {split,...fields}=item.existing;const transaction={id:ids.manual,date:'2026-11-09',amount:-1234,payee:ids.Cafe,cleared:false,...fields};if(split){ids.child1=`25290000-0000-4001-8000-${String(index).padStart(12,'0')}`;ids.child2=`25290000-0000-4002-8000-${String(index).padStart(12,'0')}`;transaction.subtransactions=[{id:ids.child1,amount:-500,notes:'First allocation'},{id:ids.child2,amount:-734,notes:'Second allocation'}];}await api.addTransactions(ids.account,[transaction]);}});
  const snapshot=async()=>plain({transactions:await api.getTransactions(ids.account,'2026-01-01','2026-12-31'),payees:await api.getPayees()});const before=await snapshot();
  const filename=item.id+'-26.10.zip',archive=check?await readFile(new URL(filename,destination)):await api.exportBudget();if(!check)await writeFile(new URL(filename,destination),archive,{flag:'wx'});
  const options={transactions:item.rows,reconcile:true,fieldMappings:null,filetype:'ofx',parseDateFormat:'yyyy mm dd',splitMode:false,inOutMode:false,outValue:'',flipAmount:false,multiplierAmount:'1',categories:[],clearOnImport:true,importNotes:true};
  const initial=finalize({...options,transactions:item.rows.map(row=>({...row,selected:true,existing:false,ignored:false}))});assert.equal(initial.error,null);
  const initialPreview=plain(await engine.send('transactions-import',{accountId:ids.account,transactions:initial.transactions,isPreview:true}));assert.deepEqual(await snapshot(),before);
  const finalized=plain(finalize(options));assert.equal(finalized.error,null);const request={accountId:ids.account,transactions:finalized.transactions,opts:{reimportDeleted:false}};
  const preview=plain(await engine.send('transactions-import',{...request,isPreview:true}));assert.deepEqual(await snapshot(),before);
  const result=plain(await engine.send('transactions-import',{...request,isPreview:false}));assert.deepEqual(result.errors,[]);const after=await snapshot();
  const repeat=plain(await engine.send('transactions-import',{...request,isPreview:false}));assert.deepEqual(repeat.errors,[]);const repeated=await snapshot();if(item.id!=='same-bank-id-twice')assert.deepEqual(repeated,after,'Repeat preserves every identity, field and financial row');
  observations.push({id:item.id,ids,input:item,before,archive:{filename,sha256:hash(archive)},initialPreview,frontend:options,finalized,preview,result,after,repeat,repeated});
 }
 const output={oracle:'Actual26.10 registered import + unchanged frontend finalizer',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,source_files:sourceFiles,cases:observations};
 function stable(value){const ids=new Map();const id=s=>{if(s.startsWith('25290000-'))return s;if(!ids.has(s))ids.set(s,`generated-${ids.size}`);return ids.get(s);};const walk=v=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v)?id(v):Array.isArray(v)?(()=>{const rows=v.map(walk);if(rows.length&&rows.every(r=>r&&typeof r==='object'&&'date'in r&&'sort_order'in r)){const key=r=>JSON.stringify(Object.fromEntries(Object.entries(r).filter(([k])=>k!=='id')));rows.sort((a,b)=>b.date.localeCompare(a.date)||b.sort_order-a.sort_order||(a.id.startsWith('25290000-')&&b.id.startsWith('25290000-')?0:key(a).localeCompare(key(b))));}return rows;})():v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,item])=>[k,walk(item)])):v;return walk(value);}
 if(check)assert.deepEqual(stable(output),stable(JSON.parse(await readFile(fixture,'utf8'))));else await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});console.log('PASS five genuine selection, primary-ID and ordering edge workflows');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
