// Independent pinned Actual engine; no replacement parser or calculator is imported.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';

process.env.TZ='UTC';
const NativeDate=Date;
globalThis.Date=class extends NativeDate {
  constructor(...args){super(...(args.length?args:['2026-11-09T12:00:00Z']));}
  static now(){return NativeDate.parse('2026-11-09T12:00:00Z');}
};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/budget-template-oracle-');
const check=process.argv.includes('--check');
const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',grammar:[],financial:[]};
const files=new Map();
async function archive(name){
  const path=new URL('../tests/fixtures/'+name,import.meta.url);
  const generated=await api.exportBudget();
  const bytes=check?await readFile(path):generated;
  if(!check)files.set(path,generated);
  return {filename:name,sha256:createHash('sha256').update(bytes).digest('hex')};
}
try {
  const engine=await api.init({dataDir:directory});
  const send=(name,body)=>engine.send(name,body);
  const ids={};
  await api.runImport('Budget template oracle',async()=>{
    await send('preferences/save',{id:'budgetType',value:'envelope'});
    ids.account=await api.createAccount({name:'Checking'},0);
    ids.group=await api.createCategoryGroup({name:'Plans',is_income:false});
    ids.fixed=await api.createCategory({name:'Fixed',group_id:ids.group,is_income:false});
    const income=(await api.getCategoryGroups()).find(g=>g.is_income).id;
    ids.salary=await api.createCategory({name:'Salary',group_id:income,is_income:true});
    await api.addTransactions(ids.account,[{id:'23240000-0000-4000-8000-000000000001',date:'2026-11-01',amount:100000,category:ids.salary}]);
    await send('notes-save',{id:ids.fixed,note:'#template 72.99'});
  });
  await send('budget/store-note-templates',[ids.fixed]);
  const parsed=(await send('budget/get-category-automations',ids.fixed))[ids.fixed];
  assert.deepEqual(parsed,[{type:'simple',monthly:72.99,limit:null,priority:0,directive:'template'}]);
  output.grammar.push({name:'simple decimal',notes:'#template 72.99',templates:parsed});
  const before=await archive('budget-templates-simple-base-26.10.zip');
  const read=async()=>{
    const values={};
    for(const field of ['budget','goal','long-goal','leftover']){
      const result=await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids.fixed});
      assert.ok(Object.hasOwn(result,'value'),field+' missing source cell');
      values[field]=result.value;
    }
    return values;
  };
  const seed=await read();
  const notification=await send('budget/apply-goal-template',{month:'2026-11'});
  const result=await read();
  assert.equal(result.budget,7299);
  assert.equal(result.goal,7299);
  assert.equal((await send('notes-get',{id:ids.fixed})).note,'#template 72.99');
  output.financial.push({name:'simple decimal',method:'envelope',month:'2026-11',notes:'#template 72.99',seed,notification,result,archive:before});
  const path=new URL('../tests/fixtures/budget-templates.json',import.meta.url);
  if(check)assert.deepEqual(output,JSON.parse(await readFile(path,'utf8')));
  else{
    for(const [path,bytes] of files)await writeFile(path,bytes,{flag:'wx'});
    await writeFile(path,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
  }
  console.log(`PASS pinned budget templates: ${output.grammar.length} grammar, ${output.financial.length} financial cases`);
}finally{
  await api.shutdown();
  await rm(directory,{recursive:true,force:true});
  globalThis.Date=NativeDate;
}
