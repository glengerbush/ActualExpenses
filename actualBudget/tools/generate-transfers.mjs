// Independent financial oracle: published @actual-app/api only, disposable budget.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setImmediate as yieldLoop } from 'node:timers/promises';
async function settled(check) { const deadline=performance.now()+5000;while(!await check()){assert.ok(performance.now()<deadline,'Public transfer mutation did not settle');await yieldLoop();} }
const require = createRequire(new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(new URL('../../actualExpenses/tools/compatibility/node_modules/@actual-app/api/package.json', import.meta.url))).version, '26.10.0');
const root = await mkdtemp(join(tmpdir(), 'actual-transfers-'));
const labels = new Map();
const label = id => id == null ? null : labels.get(id) ?? id;
const alias = (id, name) => { labels.set(id, name); return id; };
const snapshots = [];
let initialized = false;
async function snapshot(step) {
 const accounts = await api.getAccounts();
 const rows = [];
 for (const account of accounts) for (const tx of await api.getTransactions(account.id)) rows.push(tx);
 for (const tx of rows) if (tx.transfer_id && !labels.has(tx.transfer_id)) alias(tx.transfer_id, `${label(tx.id)}-other`);
 snapshots.push({step, accounts: accounts.map(a => ({id:label(a.id), closed:a.closed,off_budget:a.offbudget})).sort((a,b)=>a.id.localeCompare(b.id)), transactions:rows.map(t=>({id:label(t.id),account_id:label(t.account),amount:t.amount,date:t.date,cleared:t.cleared,category_id:label(t.category),notes:t.notes??'',transfer_id:label(t.transfer_id)})).sort((a,b)=>a.id.localeCompare(b.id))});
}
try {
 await api.init({dataDir:root}); initialized=true;
 let cash, card, off, category;
 await api.runImport('Disposable transfer oracle', async()=>{
  cash=alias(await api.createAccount({name:'Cash',offbudget:false},0),'cash');
  card=alias(await api.createAccount({name:'Credit card',offbudget:false},0),'card');
  off=alias(await api.createAccount({name:'Off budget',offbudget:true},0),'off');
  const group=await api.createCategoryGroup({name:'Expenses',is_income:false});
  category=alias(await api.createCategory({name:'Spending',group_id:group}),'category');
 });
 const payees=await api.getPayees();
 const to=a=>payees.find(p=>p.transfer_acct===a).id;
 const tx=alias('15000000-0000-4000-8000-000000000001','payment');
 await api.addTransactions(cash,[{id:tx,date:'2026-01-01',amount:-2500,payee:to(card),category,notes:'Card payment',cleared:true}],{runTransfers:true});
 await snapshot('credit-card-payment');
 await api.updateTransaction(tx,{amount:-3100,notes:'Corrected payment',date:'2026-01-02'});
 await settled(async()=> (await api.getTransactions(card))[0]?.amount===3100);
 await snapshot('amount-notes-propagate-date-independent');
 const other=(await api.getTransactions(card))[0].id;
 await api.updateTransaction(other,{date:'2026-01-03',cleared:true});
 await settled(async()=> (await api.getTransactions(card))[0]?.date==='2026-01-03');
 await snapshot('other-leg-date-and-cleared-independent');
 await api.deleteTransaction(other);
 await settled(async()=> (await api.getTransactions(cash)).length===0 && (await api.getTransactions(card)).length===0);
 await snapshot('delete-removes-both');
 const crossing=alias('15000000-0000-4000-8000-000000000002','crossing');
 await api.addTransactions(cash,[{id:crossing,date:'2026-02-01',amount:-700,payee:to(off),category}],{runTransfers:true});
 await snapshot('on-to-off-category');
 await api.setPreference('sync-transfer-date','true');
 if(!process.argv.includes('--check')) await writeFile(new URL('../tests/fixtures/transfers-26.10.0.zip',import.meta.url),await api.exportBudget());
 await api.closeAccount(cash,card);
 await settled(async()=>await api.getAccountBalance(cash)===0);
 const closing={accounts:await Promise.all((await api.getAccounts()).map(async a=>({id:label(a.id),closed:a.closed,balance:await api.getAccountBalance(a.id)}))),rows:[]};
 for(const a of await api.getAccounts())for(const t of await api.getTransactions(a.id))if(t.notes==='Closing account')closing.rows.push({account_id:label(a.id),amount:t.amount,category_id:label(t.category),notes:t.notes});
 closing.accounts.sort((a,b)=>a.id.localeCompare(b.id));closing.rows.sort((a,b)=>a.account_id.localeCompare(b.account_id));
 const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',normalization:'Generated identifiers use stable labels. Financial fields unchanged. Closing-account oracle omits current-day dates and generated transaction IDs.',snapshots,closing};
 const fixture=new URL('../tests/fixtures/transfers.json',import.meta.url);
 if(process.argv.includes('--check')) assert.deepEqual(result,JSON.parse(await readFile(fixture,'utf8')));
 else await writeFile(fixture,JSON.stringify(result,null,2)+'\n');
 console.log(`PASS ${snapshots.length} pinned public transfer snapshots`);
} finally {if(initialized) await api.shutdown();await rm(root,{recursive:true,force:true});}
