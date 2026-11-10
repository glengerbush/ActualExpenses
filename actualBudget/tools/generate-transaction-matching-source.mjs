// Observe matching and repeated imports using the genuine pinned engine.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm,mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';

process.env.TZ='UTC';
const NativeDate=Date,clock='2026-11-09T12:00:00Z';
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const seed=(label,date='2026-11-09',payee='Cafe',extra={})=>({label,date,payee,amount:-1234,notes:label,cleared:false,...extra});
const incoming=(date='2026-11-09',payee_name='Cafe',extra={})=>({date,payee_name,imported_payee:payee_name,amount:-1234,notes:'Bank note',cleared:true,...extra});
const cases=[
  {id:'global-payee-before-fallback',existing:[seed('cafe'),seed('shop','2026-11-10','Shop')],incoming:[incoming('2026-11-09','Unknown'),incoming('2026-11-10','Cafe')]},
  {id:'global-id-before-fuzzy',existing:[seed('bank','2026-11-09','Cafe',{imported_id:'bank-1'}),seed('manual','2026-11-10')],incoming:[incoming(),incoming('2026-11-09','Cafe',{imported_id:'bank-1'})]},
  {id:'duplicate-real-purchases',existing:[],incoming:[incoming(),incoming()]},
  {id:'one-manual-two-purchases',existing:[seed('manual')],incoming:[incoming(),incoming()]},
  {id:'equal-distance-tie',existing:[seed('earlier','2026-11-08'),seed('later','2026-11-10')],incoming:[incoming()]},
  {id:'inclusive-minus-seven',existing:[seed('manual','2026-11-02')],incoming:[incoming()]},
  {id:'inclusive-plus-seven',existing:[seed('manual','2026-11-16')],incoming:[incoming()]},
  {id:'outside-eight-days',existing:[seed('manual','2026-11-01')],incoming:[incoming()]},
  {id:'same-id-date-amount-differ',existing:[seed('bank','2026-10-01','Cafe',{imported_id:'bank-1'})],incoming:[incoming('2026-11-09','Cafe',{amount:-555,imported_id:'bank-1'})]},
  {id:'different-nonempty-ids',existing:[seed('bank','2026-11-09','Cafe',{imported_id:'bank-1'})],incoming:[incoming('2026-11-09','Cafe',{imported_id:'bank-2'})]},
  {id:'deleted-id-suppressed',existing:[seed('deleted','2026-11-09','Cafe',{imported_id:'bank-1',deleted:true})],incoming:[incoming('2026-11-09','Cafe',{imported_id:'bank-1'})],opts:{reimportDeleted:false}},
  {id:'deleted-id-reimported',existing:[seed('deleted','2026-11-09','Cafe',{imported_id:'bank-1',deleted:true})],incoming:[incoming('2026-11-09','Cafe',{imported_id:'bank-1'})],opts:{reimportDeleted:true}},
  {id:'reconciled-match-unchanged',existing:[seed('locked','2026-11-09','Cafe',{reconciled:true,cleared:true})],incoming:[incoming()]},
  {id:'force-add-selected',existing:[seed('manual')],incoming:[incoming('2026-11-09','Cafe',{forceAddTransaction:true})]},
];
const directory=await mkdtemp(tmpdir()+'/transaction-match-oracle-');
const fixture=new URL('../tests/fixtures/transaction-matching-source.json',import.meta.url);
const plain=value=>JSON.parse(JSON.stringify(value));
try{
  const engine=await api.init({dataDir:directory});
  const observations=[],archives=[];
  const exportDirectory=new URL('../tests/fixtures/transaction-matching/',import.meta.url);
  if(process.argv.includes('--export-seeds'))await mkdir(exportDirectory,{recursive:true});
  for(const [caseIndex,item] of cases.entries()){
    const ids={};
    await api.runImport(item.id,async()=>{
      ids.account=await api.createAccount({name:'Checking'},0);
      for(const name of ['Cafe','Shop','Unknown'])ids[name]=await api.createPayee({name});
      for(const [index,row] of item.existing.entries()){
        const id=`25260000-0000-4000-8000-${String(caseIndex*100+index+1).padStart(12,'0')}`;
        ids[row.label]=id;
        const {label,deleted,payee,...fields}=row;
        await api.addTransactions(ids.account,[{...fields,id,payee:ids[payee]}]);
      }
    });
    // The registered desktop deletion handler awaits its ledger mutation. The
    // convenience API wrapper in 26.10 indexes an unresolved promise and returns
    // early; using it during runImport can race the budget close/reopen.
    for(const row of item.existing)if(row.deleted)await engine.send('transaction-delete',{id:ids[row.label]});
    const snapshot=async()=>plain({transactions:await api.getTransactions(ids.account,'2026-01-01','2026-12-31'),payees:await api.getPayees()});
    const before=await snapshot();
    if(process.argv.includes('--export-seeds')){const bytes=await api.exportBudget();const filename=item.id+'-26.10.zip';await writeFile(new URL(filename,exportDirectory),bytes,{flag:'wx'});archives.push({id:item.id,ids,filename,sha256:createHash('sha256').update(bytes).digest('hex')});}
    const request={accountId:ids.account,transactions:item.incoming,opts:{reimportDeleted:false,...item.opts}};
    const preview=plain(await engine.send('transactions-import',{...request,isPreview:true}));
    assert.deepEqual(await snapshot(),before,`${item.id}: preview is read-only`);
    const first=plain(await engine.send('transactions-import',{...request,isPreview:false}));
    const after=await snapshot();
    const second=plain(await engine.send('transactions-import',{...request,isPreview:false}));
    const repeated=await snapshot();
    assert.deepEqual(first.errors,[],item.id);
    assert.deepEqual(second.errors,[],item.id);
    observations.push({id:item.id,input:item,ids,before,preview,first,after,second,repeated});
  }
  const path='packages/loot-core/src/server/accounts/sync.ts';
  const bytes=await readFile(new URL('../reference/actual-26.10.0/'+path,import.meta.url));
  const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,source_files:{[path]:createHash('sha256').update(bytes).digest('hex')},cases:observations};
  function stable(value){
    const ids=new Map();
    function walk(v){
      if(typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v)){
        if(v.startsWith('25260000-'))return v;
        if(!ids.has(v))ids.set(v,`generated-id-${ids.size}`);
        return ids.get(v);
      }
      if(Array.isArray(v)){
        const rows=v.map(walk);
        // Frozen-clock creates may share date/rank; source UUID ties are random.
        // Normalize only those ties, preserving meaningful date/rank ordering.
        if(rows.length&&rows.every(row=>row&&typeof row==='object'&&!Array.isArray(row)&&'sort_order'in row&&'date'in row)){
          const semantic=row=>JSON.stringify(Object.fromEntries(Object.entries(row).filter(([key])=>key!=='id')));
          return rows.sort((a,b)=>b.date.localeCompare(a.date)||b.sort_order-a.sort_order||semantic(a).localeCompare(semantic(b)));
        }
        return rows;
      }
      if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,item])=>[k,walk(item)]));
      return v;
    }
    return walk(value);
  }
  if(process.argv.includes('--check'))assert.deepEqual(stable(output),stable(JSON.parse(await readFile(fixture,'utf8'))));
  else await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
  if(process.argv.includes('--export-seeds'))await writeFile(new URL('manifest.json',exportDirectory),JSON.stringify({source_commit:output.source_commit,archives},null,2)+'\n',{flag:'wx'});
  console.log(`PASS ${observations.length} genuine matching/preview/repeated-import observations`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
