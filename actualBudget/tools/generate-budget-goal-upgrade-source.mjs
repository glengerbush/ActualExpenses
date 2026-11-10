// Real Actual26.10 goal cells retained after disabling their stored templates.
// Schema19 supports those cells and notes, but not active goal definitions.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/budget-goal-upgrade-'),check=process.argv.includes('--check');
const archivePath=new URL('../tests/fixtures/budget-goal-upgrade-source.zip',import.meta.url),manifestPath=new URL('../tests/fixtures/budget-goal-upgrade-source.json',import.meta.url);
try{
 const engine=await api.init({dataDir:directory}),send=(name,body)=>engine.send(name,body),ids={};
 const notes='Keep original description\n#template 72.99\n#goal 1000\n#cleanup source';
 await api.runImport('Goal cells before template support',async()=>{
  await send('preferences/save',{id:'budgetType',value:'envelope'});
  ids.Checking=await api.createAccount({name:'Checking'},0);
  ids.Plans=await api.createCategoryGroup({name:'Plans',is_income:false});
  ids.Fixed=await api.createCategory({name:'Fixed',group_id:ids.Plans,is_income:false});
  ids.Salary=await api.createCategory({name:'Salary',group_id:(await api.getCategoryGroups()).find(g=>g.is_income).id,is_income:true});
  await api.addTransactions(ids.Checking,[{id:'19200000-0000-4000-8000-000000000001',date:'2026-11-01',amount:100000,category:ids.Salary,cleared:true}]);
  await send('notes-save',{id:ids.Fixed,note:notes});
 });
 await send('budget/apply-goal-template',{month:'2026-11'});
 const cells={};
 for(const field of ['budget','goal','long-goal','leftover'])cells[field]=(await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids.Fixed})).value;
 assert.deepEqual(cells,{budget:7299,goal:100000,'long-goal':1,leftover:7299});
 // This is an actual engine command, not a SQLite rewrite or synthetic schema.
 await send('budget/set-category-automations',{source:'notes',categoriesWithTemplates:[{id:ids.Fixed,templates:[],cleanup:[]}]});
 for(const [field,value] of Object.entries(cells))assert.equal((await send('get-cell',{sheetName:'budget202611',name:field+'-'+ids.Fixed})).value,value);
 assert.equal((await send('notes-get',{id:ids.Fixed})).note,notes);
 const category=(await api.runQuery(api.q('categories').filter({id:ids.Fixed}).select('*'))).data[0];
 assert.equal(category.goal_def,null);assert.equal(category.cleanup_def,null);assert.deepEqual(category.template_settings,{source:'notes'});
 const generated=await api.exportBudget(),bytes=check?await readFile(archivePath):generated;
 const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',ids,notes,cells,archive_sha256:createHash('sha256').update(bytes).digest('hex')};
 if(check){const original=JSON.parse(await readFile(manifestPath,'utf8'));const {ids:ignore,...a}=output,{ids:ignore2,...b}=original;assert.deepEqual(a,b);}
 else{await writeFile(archivePath,bytes,{flag:'wx'});await writeFile(manifestPath,JSON.stringify(output,null,2)+'\n',{flag:'wx'});}
 console.log('PASS genuine source goal cells, original notes and disabled definitions');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
