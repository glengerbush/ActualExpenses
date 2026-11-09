// Pinned public mutation pipeline, followed by settled persisted-ledger reads.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
process.env.TZ='UTC';const NativeDate=Date;const clock='2026-11-09T12:00:00Z';
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const ts=createRequire(new URL('../web/package.json',import.meta.url))('typescript');
const reference=new URL('../reference/actual-26.10.0/packages/loot-core/src/',import.meta.url);
const loaded=new Map();
async function loadSource(name){
 if(loaded.has(name))return loaded.get(name);
 const text=await readFile(new URL(name+'.ts',reference),'utf8');
 const imports=[...text.matchAll(/from ['"](#shared\/[^'"]+)['"]/g)].map(m=>m[1]);
 const dependencies=new Map();for(const imp of imports)dependencies.set(imp,await loadSource(imp.slice(1)));
 const exports={};new Function('require','exports',ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText)(name=>dependencies.has(name)?dependencies.get(name):require(name),exports);loaded.set(name,exports);return exports;
}
const months=await loadSource('shared/months');
const scheduleSource=await readFile(new URL('shared/schedules.ts',reference),'utf8');const ast=ts.createSourceFile('schedules.ts',scheduleSource,ts.ScriptTarget.Latest,true);
const requested=['getUpcomingDays','getScheduledAmount','getStatus'];const declarations=requested.map(name=>{const node=ast.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text===name);assert.ok(node,name);return node.getText(ast);}).join('\n');
const constants=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='DEFAULT_UPCOMING_SCHEDULE_DAYS'));assert.ok(constants);
const helpers={};new Function('monthUtils','exports',ts.transpileModule(constants.getText(ast)+'\n'+declarations,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText)(months,helpers);
const root=await mkdtemp(tmpdir()+'/schedule-financial-');const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock,horizons:[],amounts:[],cases:[]};
for(const today of ['2026-11-09','2026-02-15','2028-02-15','2026-12-31'])for(const value of ['1','7','14','oneMonth','currentMonth','2-day','5-week','3-month','1-year']){const days=helpers.getUpcomingDays(value,today);assert.ok(Number.isSafeInteger(days)&&days>=0);result.horizons.push({today,value,days});}
for(const value of [-101,101,{num1:-100,num2:-101},{num1:100,num2:101},{num1:9007199254740990,num2:9007199254740991},{num1:-9007199254740990,num2:-9007199254740991}]){const amount=helpers.getScheduledAmount(value);assert.ok(Number.isSafeInteger(amount));result.amounts.push({value,amount});}
try{
 const engine=await api.init({dataDir:root});
 const scenarios=[
  {name:'linked category and staged notes',amount:-1234,actions:[{op:'set',field:'category',value:'CATEGORY'},{op:'set',field:'notes',value:'Bill'}],ordinary:true},
  {name:'income approximate amount',amount:2345,amountOp:'isapprox',actions:[]},
  {name:'range midpoint negative tie',amount:{num1:-100,num2:-101},amountOp:'isbetween',actions:[]},
  {name:'transfer retains reciprocal schedule',amount:-876,transfer:true,actions:[{op:'set',field:'notes',value:'Move'}]},
  {name:'split linked rule',amount:-101,actions:[{op:'set-split-amount',value:-40,options:{splitIndex:1,method:'fixed-amount'}},{op:'set',field:'category',value:'CATEGORY',options:{splitIndex:1}},{op:'set-split-amount',value:null,options:{splitIndex:2,method:'remainder'}}]},
  {name:'split transfer linked rule',amount:-101,actions:[{op:'set-split-amount',value:-40,options:{splitIndex:1,method:'fixed-amount'}},{op:'set',field:'payee',value:'TRANSFER',options:{splitIndex:1}},{op:'set-split-amount',value:null,options:{splitIndex:2,method:'remainder'}}]},
  {name:'post today bypasses own future condition',amount:-90,date:'2027-01-05',today:true,actions:[{op:'set',field:'notes',value:'Approved early'}]},
  {name:'rule suppresses scheduled transaction',amount:-55,actions:[{op:'delete-transaction',value:null}]},
 ];
 for(const[index,c]of scenarios.entries()){
  const ids={};await api.runImport('Schedule financial '+index,async()=>{ids.ACCOUNT=await api.createAccount({name:'Cash'},0);ids.PEER=await api.createAccount({name:'Savings'},0);ids.PAYEE=await api.createPayee({name:'Bill'});const group=await api.createCategoryGroup({name:'Expenses',is_income:false});ids.CATEGORY=await api.createCategory({name:'Utilities',group_id:group});});ids.TRANSFER=(await api.getPayees()).find(p=>p.transfer_acct===ids.PEER).id;ids.BACK_TRANSFER=(await api.getPayees()).find(p=>p.transfer_acct===ids.ACCOUNT).id;
  const resolve=value=>JSON.parse(JSON.stringify(value,(key,v)=>typeof v==='string'&&ids[v]?ids[v]:v));
  const schedule=`21220000-0000-4000-8001-${String(index+1).padStart(12,'0')}`;
  const conditions=[{field:'account',op:'is',value:ids.ACCOUNT},{field:'payee',op:'is',value:c.transfer?ids.TRANSFER:ids.PAYEE},{field:'amount',op:c.amountOp??'is',value:c.amount},{field:'date',op:'is',value:c.date??'2026-11-09'}];
  assert.equal(await engine.send('schedule/create',{schedule:{id:schedule,name:c.name,posts_transaction:false},conditions}),schedule);
  const row=(await api.aqlQuery(api.q('schedules').filter({id:schedule}).select('*'))).data[0];assert.ok(row.rule);
  const rule=await engine.send('rule-get',{id:row.rule});assert.ok(rule);
  const saved=await engine.send('rule-update',{...rule,actions:[...rule.actions,...resolve(c.actions)]});assert.ok(!saved?.error,JSON.stringify(saved));
  if(c.ordinary){const saved=await engine.send('rule-add',{stage:'post',conditionsOp:'and',conditions:[{field:'category',op:'is',value:ids.CATEGORY}],actions:[{op:'append-notes',value:' / checked'}]});assert.ok(!saved.error);}
  await engine.send('schedule/post-transaction',{id:schedule,today:c.today??false});
  let lists;for(let attempt=0;attempt<40;attempt++){lists=await Promise.all([api.getTransactions(ids.ACCOUNT),api.getTransactions(ids.PEER)]);if(c.name.startsWith('rule suppresses')||lists[0].length&&(c.transfer||c.name.includes('split transfer')?lists[1].length:true))break;await new Promise(r=>setTimeout(r,25));}
  for(const list of lists)list.sort((a,b)=>a.date.localeCompare(b.date)||a.amount-b.amount||String(a.notes??'').localeCompare(String(b.notes??'')));
  const flat=[];for(let account=0;account<lists.length;account++)for(const[parentIndex,parent]of lists[account].entries()){const key=`${account?'PEER':'ACCOUNT'}/${parentIndex}`;flat.push({key,row:parent,parent:null});for(const[childIndex,child]of(parent.subtransactions??[]).entries())flat.push({key:`${key}/${childIndex}`,row:child,parent:key});}
  for(const {row}of flat){assert.ok(typeof row.id==='string'&&typeof row.date==='string'&&Number.isSafeInteger(row.amount));}
  const labels=new Map(flat.map(({key,row})=>[row.id,key]));
  const name=id=>Object.entries(ids).find(([,v])=>v===id)?.[0]??null;
  const assertReference=id=>{const mapped=name(id);assert.ok(mapped,'Unknown persisted reference '+id);return mapped;};
  const ledger=flat.map(({key,row,parent})=>({key,parent,amount:row.amount,date:row.date,notes:row.notes??'',cleared:!!row.cleared,payee:row.payee==null?null:assertReference(row.payee),category:row.category==null?null:assertReference(row.category),schedule:row.schedule??null,peer:row.transfer_id?labels.get(row.transfer_id)??'MISSING':null}));
  result.cases.push({...c,schedule,conditions:conditions.map(cond=>({...cond,value:name(cond.value)??cond.value})),ledger,balances:await Promise.all([api.getAccountBalance(ids.ACCOUNT),api.getAccountBalance(ids.PEER)])});
 }
 const file=new URL('../tests/fixtures/schedule-financial.json',import.meta.url);if(process.argv.includes('--check'))assert.deepEqual(result,JSON.parse(await readFile(file,'utf8')));else await writeFile(file,JSON.stringify(result,null,2)+'\n');
 console.log(`PASS ${result.horizons.length} horizon +${result.amounts.length} amount source helpers and ${result.cases.length} persisted financial cases`);
}finally{await api.shutdown();await rm(root,{recursive:true,force:true});globalThis.Date=NativeDate;}
