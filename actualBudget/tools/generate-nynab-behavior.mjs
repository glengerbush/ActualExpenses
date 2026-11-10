// Independent pinned import + registered schedule/rule evaluation, no replacement code.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';const OriginalDate=Date,clock='2026-11-09T12:00:00.000Z';globalThis.Date=class extends OriginalDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return OriginalDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'))).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/nynab-behavior-'),input=await readFile(new URL('../tests/fixtures/ynab/nynab-demo.json',import.meta.url)),path=new URL('../tests/fixtures/nynab-behavior.json',import.meta.url),check=process.argv.includes('--check');
try{const engine=await api.init({dataDir:directory});await api.importBudget(input,{type:'ynab5',filename:'ynab5-demo-budget.json'});const accounts=await api.getAccounts(),payees=await api.getPayees(),categories=await api.getCategories(),groups=await api.getCategoryGroups(),rules=await api.getRules(),schedules=await api.getSchedules();
 const ids=new Map([...accounts.map(a=>[a.id,'account:'+a.name]),...payees.map(p=>[p.id,p.transfer_acct?'transfer:account:'+accounts.find(a=>a.id===p.transfer_acct).name:'payee:'+p.name]),...categories.map(c=>[c.id,'category:'+groups.find(g=>g.id===c.group_id).name+'/'+c.name]),...schedules.map(s=>[s.id,'schedule:'+s.name])]);
 const normalize=v=>Array.isArray(v)?v.map(normalize):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,normalize(x)])):ids.get(v)??v;
 const before=await Promise.all(accounts.map(a=>api.getTransactions(a.id,'1900-01-01','2099-12-31'))),effects=[];
 for(const p of payees.filter(p=>!p.transfer_acct).sort((a,b)=>a.name.localeCompare(b.name))){const transaction={id:'27000000-0000-4000-8000-000000000001',account:accounts[0].id,date:'2026-11-09',amount:-100,payee:p.id,category:null,notes:'',cleared:false};const result=await engine.send('rules-run',{transaction});effects.push(normalize({payee:p.id,category:result.category,amount:result.amount,notes:result.notes}));}
 const definitions=[];for(const s of schedules){const rule=rules.find(r=>r.id===s.rule);assert.ok(rule,s.name);const dates=typeof s.date==='string'?[s.date]:await engine.send('schedule/get-upcoming-dates',{config:s.date,count:s.date.frequency==='yearly'?512:4096});definitions.push(normalize({name:s.name,account:s.account,payee:s.payee,amount:s.amount,date:s.date,completed:s.completed,posts_transaction:s.posts_transaction,conditions:rule.conditions,actions:rule.actions,dates}));}definitions.sort((a,b)=>a.name.localeCompare(b.name));
 assert.deepEqual(await Promise.all(accounts.map(a=>api.getTransactions(a.id,'1900-01-01','2099-12-31'))),before,'Observation does not post or mutate imported money');
 const output={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,input_sha256:createHash('sha256').update(input).digest('hex'),normalization:'Generated reference IDs are labeled by account/payee/category/schedule names. Public upcoming dates are retained so current-day initial next date is compared within an explicit source-observed calendar range; one-off dates remain literal.',effects,definitions};
 if(check)assert.deepEqual(output,JSON.parse(await readFile(path)));else await writeFile(path,JSON.stringify(output,null,2)+'\n',{flag:'wx'});console.log('PASS actual nYNAB imported rules, '+definitions.length+' manual schedule definitions and independently executed upcoming dates');
}finally{await api.shutdown().catch(()=>{});await rm(directory,{recursive:true,force:true});}
