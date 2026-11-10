// Pinned import pipeline: lazy payees, rules before matching, and saved options.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';
const NativeDate=Date,clock='2026-11-09T12:00:00Z';
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/transaction-import-rules-'),check=process.argv.includes('--check');
const fixture=new URL('../tests/fixtures/transaction-import-rules-source.json',import.meta.url);
const plain=value=>JSON.parse(JSON.stringify(value)),hash=bytes=>createHash('sha256').update(bytes).digest('hex');
try{
  const engine=await api.init({dataDir:directory});
  const observations=[];
  for(const [index,name] of ['new-payee','matched-new-payee','rules-before-match','rules-split'].entries()){
    const ids={manual:`25280000-0000-4000-8000-${String(index+1).padStart(12,'0')}`};
    await api.runImport(name,async()=>{
      ids.account=await api.createAccount({name:'Checking'},0);
      ids.cafe=await api.createPayee({name:'Cafe'});
      ids.group=await api.createCategoryGroup({name:'Expenses',is_income:false});
      ids.food=await api.createCategory({name:'Food',group_id:ids.group,is_income:false});
      if(name==='matched-new-payee'||name==='rules-before-match')await api.addTransactions(ids.account,[{id:ids.manual,date:'2026-11-09',amount:name==='rules-before-match'?-1500:-1234,payee:ids.cafe,notes:'',cleared:false}]);
    });
    const settings={
      [`csv-mappings-${ids.account}`]:JSON.stringify({date:'Date',payee:'Payee',amount:'Amount',notes:'Memo',category:null,inOut:'Direction',outflow:null,inflow:null}),
      [`csv-delimiter-${ids.account}`]:';', [`csv-encoding-${ids.account}`]:'windows-1252',
      [`csv-has-header-${ids.account}`]:'false', [`csv-skip-start-lines-${ids.account}`]:'2', [`csv-skip-end-lines-${ids.account}`]:'1',
      [`csv-in-out-mode-${ids.account}`]:'true', [`csv-out-value-${ids.account}`]:'Debit',
      [`parse-date-${ids.account}-csv`]:'dd mm yyyy', [`parse-date-${ids.account}-qif`]:'mm dd yy',
      [`flip-amount-${ids.account}-csv`]:'true', [`flip-amount-${ids.account}-qif`]:'false',
      [`import-notes-${ids.account}-csv`]:'true', [`import-notes-${ids.account}-qif`]:'false',
      [`ofx-fallback-missing-payee-${ids.account}`]:'false', [`ofx-swap-payee-memo-${ids.account}`]:'true',
      [`qif-swap-payee-memo-${ids.account}`]:'true', [`camt-swap-payee-memo-${ids.account}`]:'true',
      [`import-reimport-deleted-${ids.account}`]:'false',
    };
    for(const [key,value] of Object.entries(settings))await api.setPreference(key,value);
    // Exercise a later option value. Official Actual ZIP exports clear CRDT
    // history, so these archives prove values, not per-key clock preservation.
    await api.setPreference(`csv-delimiter-${ids.account}`,',');
    settings[`csv-delimiter-${ids.account}`]=',';
    const rules=[];
    if(name.startsWith('rules')){
      const actions=name==='rules-before-match'?[{op:'set',field:'amount',value:-1500},{op:'set',field:'category',value:ids.food},{op:'append-notes',value:' / checked'}]:[{op:'append-notes',value:' / checked'},{op:'set-split-amount',value:50,options:{method:'fixed-percent',splitIndex:1}},{op:'set',field:'category',value:ids.food,options:{splitIndex:1}},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:2}}];
      rules.push({id:`25280000-0000-4000-8001-${String(index+1).padStart(12,'0')}`,stage:'post',conditionsOp:'and',conditions:[{field:'amount',op:'is',value:-1234}],actions});
      for(const rule of rules){const result=await engine.send('rule-add',rule);assert.ok(!result.error,JSON.stringify(result));}
    }
    const snapshot=async()=>plain({transactions:await api.getTransactions(ids.account,'2026-01-01','2026-12-31'),payees:await api.getPayees(),categories:await api.getCategories(),preferences:await api.getPreferences()});
    const before=await snapshot();
    for(const [key,value] of Object.entries(settings))assert.equal(before.preferences[key],value);
    const archiveName=`transaction-import-${name}-26.10.zip`,archivePath=new URL('../tests/fixtures/'+archiveName,import.meta.url);
    const archive=check?await readFile(archivePath):await api.exportBudget();
    if(!check)await writeFile(archivePath,archive,{flag:'wx'});
    const payee_name=name.includes('new-payee')?'BRAND NEW SHOP':'Cafe';
    const input={accountId:ids.account,transactions:[{date:'2026-11-09',amount:-1234,payee_name,imported_payee:payee_name,notes:'Bank note',cleared:true}],opts:{reimportDeleted:false}};
    const preview=plain(await engine.send('transactions-import',{...input,isPreview:true}));
    assert.deepEqual(await snapshot(),before,'Preview may propose new payees or split rules but writes nothing');
    const first=plain(await engine.send('transactions-import',{...input,isPreview:false})),after=await snapshot();
    const second=plain(await engine.send('transactions-import',{...input,isPreview:false})),repeated=await snapshot();
    assert.deepEqual(first.errors,[]);assert.deepEqual(second.errors,[]);
    if(name==='rules-before-match'){
      assert.equal(after.transactions.length,1,'Changed rule amount must match the original manual transaction');
      assert.equal(after.transactions[0].id,ids.manual);
      assert.equal(after.transactions[0].amount,-1500);
      assert.equal(after.transactions[0].notes,'Bank note / checked');
    }
    if(name==='rules-split'){
      assert.equal(after.transactions.length,1);
      assert.equal(after.transactions[0].notes,'Bank note / checked');
      assert.equal(after.transactions[0].subtransactions.length,2);
      assert.deepEqual(after.transactions[0].subtransactions.map(row=>row.amount),[-617,-617]);
    }
    assert.deepEqual(repeated,after,'Repeating import preserves rule results and reference identities');
    observations.push({id:name,ids,settings,rules,input,archive:{filename:archiveName,sha256:hash(archive)},before,preview,first,after,second,repeated});
  }
  const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,cases:observations};
  function stable(value){
    const ids=new Map();
    const identity=id=>{if(id.startsWith('25280000-'))return id;if(!ids.has(id))ids.set(id,`generated-id-${ids.size}`);return ids.get(id);};
    const string=text=>text.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g,identity);
    const walk=v=>typeof v==='string'?string(v):Array.isArray(v)?v.map(walk):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([key,item])=>[string(key),walk(item)])):v;
    return walk(value);
  }
  if(check)assert.deepEqual(stable(output),stable(JSON.parse(await readFile(fixture,'utf8'))));
  else await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
  console.log('PASS four genuine import/payee/rules/settings observations');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
