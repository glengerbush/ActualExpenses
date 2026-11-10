// Execute note parsing through pinned Actual's real registered engine messages.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const cases=[
 ['fixed','#template 100'],['negative','#template -103.23'],['zero','#template 0'],
 ['currency','#template € 72.99'],['priority','#template-12 50'],['priority zero','#template-0 25'],
 ['monthly cap','#template 10 up to 100'],['cap only','#template up to 100'],
 ['daily cap','#template 10 up to 3 per day'],['weekly cap','#template 10 up to 25 per week starting 2026-11-02'],
 ['held cap','#template 10 up to 100 hold'],['held cap only','#template up to 100 hold'],
 ['target','#template 1000 by 2027-04'],['target monthly repeat','#template 1000 by 2027-04 repeat every month'],
 ['target month interval','#template 1000 by 2027-04 repeat every 3 months'],
 ['target annual repeat','#template 1000 by 2027-04 repeat every year'],
 ['target year interval','#template 1000 by 2027-04 repeat every 2 years'],
 ['spend interval','#template 1000 by 2027-04 spend from 2026-12'],
 ['spend annual','#template 1000 by 2027-04 spend from 2026-12 repeat every year'],
 ['period day','#template 5 repeat every day starting 2026-11-01'],
 ['period days','#template 5 repeat every 3 days starting 2026-11-01'],
 ['period week','#template 5 repeat every week starting 2026-11-01'],
 ['period weeks','#template 5 repeat every 2 weeks starting 2026-11-01'],
 ['period months','#template 5 repeat every 2 months starting 2026-11-01'],
 ['period year','#template 5 repeat every year starting 2026-11-01'],
 ['period years','#template 5 repeat every 2 years starting 2026-11-01'],
 ['period capped','#template 5 repeat every week starting 2026-11-01 up to 100'],
 ['percentage category','#template 12.5% of Salary'],['percentage previous','#template 10% of previous Salary'],
 ['percentage available','#template 10% of available funds'],['percentage all income','#template 10% of all income'],
 ['schedule','#template schedule Rent'],['schedule full','#template schedule full Rent'],
 ['schedule percent','#template schedule Rent [increase 12.5%]'],
 ['schedule decrease','#template schedule Rent [decrease 25]'],
 ['schedule bracket name','#template schedule Rent [primary] [increase 1]'],
 ['average','#template average 6 months'],['average short','#template average 6'],
 ['average percent','#template average 6 months [decrease 10%]'],
 ['average fixed','#template average 6 months [increase 5.25]'],
 ['copy','#template copy from 2 months ago'],['copy capped','#template copy from 2 months ago up to 250'],
 ['remainder','#template remainder'],['weighted remainder','#template remainder 3'],
 ['capped remainder','#template remainder 2 up to 100'],['goal','#goal 1000'],
 ['goal with template','#template 100\n#goal 1000'],
 ['multiple descriptions','Netflix\n#template 24.99\nDisney Plus\n#template 9.99\nAmazon Prime\n#template 7.99'],
 ['description block','First line\nSecond line  \n#template 10'],
 ['blank ends description','Not attached\n\n#template 10'],
 ['cleanup ends description','Not attached\n#cleanup all\n#template 10'],
 ['inline prefix','test: #template 12'],['leading whitespace','  \t#template 12  '],
 ['case preserved lower prefix','#template AVERAGE 3 MONTHS'],
 ['uppercase prefix','#TEMPLATE 10\n#GOAL 100'],
 ['irrelevant notes','Normal note only'],['empty',''],
 ['malformed amount','#template 1.999'],['missing amount','#template'],
 ['negative priority','#template--1 10'],['unknown directive','#template nonsense'],
 ['zero average','#template average 0 months'],['zero copy','#template copy from 0 months ago'],
 ['invalid percent adjustment','#template average 3 months [decrease 100%]'],
 ['invalid large adjustment','#template schedule Rent [increase 1001%]'],
 ['maximum adjustment','#template schedule Rent [increase 1000%]'],
 ['duplicate goals','#goal 100\n#goal 200'],
 ['invalid calendar syntax accepted by grammar','#template 100 by 2026-13'],
 ['weekly invalid calendar accepted by grammar','#template 5 repeat every week starting 2026-13-40'],
 ['trailing comment','#template 10 trailing text'],
];
const directory=await mkdtemp(tmpdir()+'/budget-template-grammar-');
const check=process.argv.includes('--check');
try{
 const engine=await api.init({dataDir:directory}),send=(name,body)=>engine.send(name,body);
 let group;const ids={};
 await api.runImport('Budget template grammar',async()=>{
  await api.createAccount({name:'Checking'},0);
  group=await api.createCategoryGroup({name:'Grammar',is_income:false});
 });
 const observations=[];
 for(const [index,[name,notes]] of cases.entries()){
  const id=await api.createCategory({name:`Case ${String(index+1).padStart(2,'0')}: ${name}`,group_id:group,is_income:false});ids[name]=id;
  await send('notes-save',{id,note:notes});
  await send('budget/store-note-templates',[id]);
  const templates=(await send('budget/get-category-automations',id))[id]??[];
  assert.equal((await send('notes-get',{id})).note,notes,'Parsing must preserve original notes');
  observations.push({name,notes,templates});
 }
 const zip=new URL('../tests/fixtures/budget-template-grammar-26.10.zip',import.meta.url);
 const generated=await api.exportBudget();
 const bytes=check?await readFile(zip):generated;
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',archive:{filename:'budget-template-grammar-26.10.zip',sha256:createHash('sha256').update(bytes).digest('hex'),ids},cases:observations};
 const file=new URL('../tests/fixtures/budget-template-grammar.json',import.meta.url);
 if(check){const original=JSON.parse(await readFile(file,'utf8'));result.archive.ids=original.archive.ids;assert.deepEqual(result,original);}
 else{await writeFile(zip,bytes,{flag:'wx'});await writeFile(file,JSON.stringify(result,null,2)+'\n',{flag:'wx'});}
 console.log(`PASS ${observations.length} pinned note grammar observations; original notes retained`);
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
