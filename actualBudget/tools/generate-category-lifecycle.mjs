import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const require = createRequire(process.env.ACTUAL_ORACLE_PACKAGE ?? new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../package.json', 'file://' + require.resolve('@actual-app/api')), 'utf8')).version, '26.10.0');
const root = await mkdtemp(join(tmpdir(), 'category-oracle-'));
const names = new Map(); const alias = (id, name) => (names.set(id,name), id); const label = id => names.get(id) ?? id ?? null;
const snapshots = [];
let account, group, other, income, food, target;
async function snapshot(step) {
 const categories = (await api.getCategories()).filter(c => names.has(c.id)).map(c => ({ id:label(c.id),name:c.name,group:label(c.group_id),income:c.is_income,hidden:c.hidden })).sort((a,b)=>a.id.localeCompare(b.id));
 const transactions = (await api.getTransactions(account)).map(t=>({id:label(t.id),amount:t.amount,category:label(t.category),notes:t.notes ?? null})).sort((a,b)=>a.id.localeCompare(b.id));
 const note = await api.getNote(food);
 snapshots.push({step,categories,transactions,note:note?.note ?? null});
}
try {
 await api.init({dataDir:root});
 await api.runImport('Disposable category oracle', async()=> {
  account = await api.createAccount({name:'Checking'},0);
  group=alias(await api.createCategoryGroup({name:'Daily'}),'daily'); other=alias(await api.createCategoryGroup({name:'Other'}),'other');
  income=alias(await api.createCategoryGroup({name:'Earnings',is_income:true}),'income');
  food=alias(await api.createCategory({name:'Food',group_id:group}),'food'); target=alias(await api.createCategory({name:'Dining',group_id:other}),'dining');
  const salary=alias(await api.createCategory({name:'Salary',group_id:income,is_income:true}),'salary');
  await api.addTransactions(account,[-1250,250,400,10000,-99].map((amount,i)=>({id:alias(`10000000-0000-4000-8000-${String(i+1).padStart(12,'0')}`,`transaction-${i+1}`),date:'2026-01-01',amount,category:i<3?food:i===3?salary:null,notes:['purchase','refund','reimbursement','income','uncategorized'][i]})));
 });
 await snapshot('initial');
 await api.updateCategory(food,{name:'Meals',group_id:other,hidden:true}); await api.updateNote(food,'Keep receipts\nReimbursements stay here');
 await snapshot('renamed-moved-hidden-noted');
 if(!process.argv.includes('--check')) {await writeFile(new URL('../tests/fixtures/category-import-26.10.0.zip',import.meta.url),await api.exportBudget()); await writeFile(new URL('../tests/fixtures/category-import-26.10.0.json',import.meta.url),JSON.stringify({aliases:Object.fromEntries(names),snapshot:snapshots.at(-1)},null,2)+'\n');}
 await api.updateCategory(food,{name:'Meals',hidden:false}); await api.deleteCategory(food,target); await snapshot('merged');
 await api.deleteCategory(target); await snapshot('deleted-to-uncategorized');
 if(!process.argv.includes('--check')) await writeFile(new URL('../tests/fixtures/category-retired-26.10.0.zip',import.meta.url),await api.exportBudget());
 const result={oracle:'@actual-app/api@26.10.0',normalization:'Only generated IDs replaced with stable labels; financial values and references untouched.',snapshots};
 const file=new URL('../tests/fixtures/category-lifecycle.json',import.meta.url);
 if(process.argv.includes('--check'))assert.deepEqual(result,JSON.parse(await readFile(file,'utf8')));else await writeFile(file,JSON.stringify(result,null,2)+'\n');
 console.log('PASS category oracle');
} finally {await api.shutdown();await rm(root,{recursive:true,force:true});}
