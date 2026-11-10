// Baseline generated defaults are observed by executing real rule evaluation.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
process.env.TZ='UTC';const OriginalDate=Date,clock='2026-11-09T12:00:00.000Z';globalThis.Date=class extends OriginalDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return OriginalDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'))).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/ynab-learning-'),input=await readFile(new URL('../tests/fixtures/ynab/ynab4-demo.zip',import.meta.url)),path=new URL('../tests/fixtures/ynab-learning.json',import.meta.url),check=process.argv.includes('--check');
try{const engine=await api.init({dataDir:directory});await api.importBudget(input,{type:'ynab4',filename:'ynab4-demo-budget.zip'});const accounts=await api.getAccounts(),payees=await api.getPayees(),categories=await api.getCategories(),groups=await api.getCategoryGroups(),rules=await api.getRules();
 const category=id=>{const c=categories.find(c=>c.id===id);return c?groups.find(g=>g.id===c.group_id).name+'/'+c.name:null;};const payee=id=>payees.find(p=>p.id===id)?.name??null;
 const samples=rules.map(rule=>{assert.equal(rule.stage??null,null);assert.equal(rule.conditions.length,1);assert.equal(rule.actions.length,1);const c=rule.conditions[0],a=rule.actions[0];assert.equal(c.op,'is');assert.equal(c.field,'payee');assert.equal(a.op,'set');assert.equal(a.field,'category');return {payee:payee(c.value),category:category(a.value)};});
 const before=await Promise.all(accounts.map(a=>api.getTransactions(a.id,'1900-01-01','2099-12-31'))),effects=[];
 for(const p of payees.filter(p=>!p.transfer_acct).sort((a,b)=>a.name.localeCompare(b.name))){const transaction={id:'27000000-0000-4000-8000-000000000001',account:accounts[0].id,date:'2026-11-09',amount:-100,payee:p.id,category:null,notes:'',cleared:false};const result=await engine.send('rules-run',{transaction});effects.push({payee:p.name,category:category(result.category),amount:result.amount,notes:result.notes});}
 assert.deepEqual(await Promise.all(accounts.map(a=>api.getTransactions(a.id,'1900-01-01','2099-12-31'))),before,'Rule observation is read-only');
 const unique=[...new Map(samples.map(s=>[JSON.stringify(s),s])).values()].sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)));
 const stable={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,input_sha256:createHash('sha256').update(input).digest('hex'),unique_setter_effects:unique,effects};
 if(check){const expected=JSON.parse(await readFile(path));const {observed_rule_samples,normalization,...record}=expected;assert.deepEqual(stable,record);for(const rule of samples)assert.ok(expected.unique_setter_effects.some(s=>JSON.stringify(s)===JSON.stringify(rule)));}
 else await writeFile(path,JSON.stringify({...stable,normalization:'Concurrent account import may generate repeated identical one-payee category setters. Full original exported rules remain in the genuine Actual ZIP. Only these proved-idempotent duplicate setters are collapsed for effective-rule comparison; read-only rules-run results are asserted exactly.',observed_rule_samples:samples},null,2)+'\n',{flag:'wx'});
 console.log('PASS source learned defaults via actual rules-run, '+rules.length+' observed rules / '+unique.length+' distinct idempotent setters');
}finally{await api.shutdown().catch(()=>{});await rm(directory,{recursive:true,force:true});}
