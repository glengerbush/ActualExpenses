// Executable pinned engine oracle. No rewritten rule implementation is imported.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
process.env.TZ='UTC';
const NativeDate=Date;
const frozen=NativeDate.parse('2026-11-09T12:00:00Z');
globalThis.Date=class extends NativeDate { constructor(...args){super(...(args.length?args:[frozen]));}static now(){return frozen;} };
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/rules-oracle-');
const result={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',clock:'2026-11-09T12:00:00Z',cases:[]};
const identity=n=>`19190000-0000-4000-8000-${String(n).padStart(12,'0')}`;
try {
 const engine=await api.init({dataDir:directory});
 const ids={};
 await api.runImport('Rule engine oracle',async()=>{
  ids.account=await api.createAccount({name:'Cash'},0);
  const group=await api.createCategoryGroup({name:'Expenses',is_income:false});
  ids.group=group;
  ids.Food=await api.createCategory({name:'Food',group_id:group});ids.Travel=await api.createCategory({name:'Travel',group_id:group});
  ids.Shop=await api.createPayee({name:'Shop'});
 });
 const rules=[
  {id:identity(3),stage:'post',conditionsOp:'and',conditions:[{field:'category',op:'is',value:ids.Food}],actions:[{op:'append-notes',value:' / checked'}]},
  {id:identity(2),stage:null,conditionsOp:'and',conditions:[{field:'payee',op:'is',value:ids.Shop}],actions:[{op:'set',field:'category',value:ids.Food}]},
  {id:identity(1),stage:'pre',conditionsOp:'and',conditions:[{field:'imported_payee',op:'contains',value:'SHOP'}],actions:[{op:'set',field:'payee',value:ids.Shop}]},
 ];
 for(const rule of rules){const saved=await engine.send('rule-add',rule);assert.equal(saved.id,rule.id);assert.ok(!saved.error,JSON.stringify(saved));}
 const transaction={id:identity(20),account:ids.account,payee:null,category:null,amount:-1234,date:'2026-11-08',notes:'Original',imported_payee:'MY SHOP',cleared:false};
 const after=await engine.send('rules-run',{transaction});
 result.ids=ids;
 result.cases.push({name:'pre rename then default category then post notes',rules,transaction,after});
 const archive=await api.exportBudget();
 const checks=[
  ['empty conditions',[],{},'and'],
  ['or second condition',[{field:'amount',op:'is',value:999},{field:'notes',op:'is',value:'original'}],{},'or'],
  ['and false',[{field:'notes',op:'is',value:'original'},{field:'cleared',op:'is',value:true}],{},'and'],
  ['id null',[{field:'category',op:'is',value:null}],{category:null}],
  ['id not null',[{field:'category',op:'isNot',value:null}],{category:ids.Food}],
  ['membership',[{field:'payee',op:'oneOf',value:[ids.Shop]}],{payee:ids.Shop}],
  ['null notOneOf',[{field:'payee',op:'notOneOf',value:[ids.Shop]}],{payee:null}],
  ['notes empty',[{field:'notes',op:'is',value:''}],{notes:null}],
  ['contains normalized',[{field:'notes',op:'contains',value:'ORIG'}],{}],
  ['doesNotContain',[{field:'notes',op:'doesNotContain',value:'other'}],{}],
  ['regex unicode units',[{field:'notes',op:'matches',value:'^.$'}],{notes:'😀'}],
  ['regex lower normalization',[{field:'notes',op:'matches',value:'\\D'}],{notes:'7'}],
  ['invalid regex nonmatch',[{field:'notes',op:'matches',value:'['}],{}],
  ['tags exact boundaries',[{field:'notes',op:'hasTags',value:'Food bus'}],{notes:'#food#BUS'}],
  ['escaped tag',[{field:'notes',op:'hasAnyTag',value:'food'}],{notes:'##food'}],
  ['inclusive range reversed',[{field:'amount',op:'isbetween',value:{num1:-1000,num2:-1500}}],{}],
  ['approx rounding boundary',[{field:'amount',op:'isapprox',value:100}],{amount:108}],
  ['approx outside',[{field:'amount',op:'isapprox',value:100}],{amount:109}],
  ['outflow zero',[{field:'amount',op:'is',value:0,options:{outflow:true}}],{amount:0}],
  ['outflow sign',[{field:'amount',op:'is',value:1234,options:{outflow:true}}],{}],
  ['inflow rejects expense',[{field:'amount',op:'gt',value:0,options:{inflow:true}}],{}],
  ['month date',[{field:'date',op:'is',value:'2026-11'}],{}],
  ['year date',[{field:'date',op:'is',value:'2026'}],{}],
  ['approx date inclusive',[{field:'date',op:'isapprox',value:'2026-11-10'}],{}],
  ['approx date outside',[{field:'date',op:'isapprox',value:'2026-11-11'}],{}],
  ['on budget',[{field:'account',op:'onBudget',value:null}],{}],
  ['category group',[{field:'category_group',op:'is',value:ids.group}],{category:ids.Food}],
  ['recurring beyond100',[{field:'date',op:'is',value:{frequency:'monthly',start:'2010-01-08',interval:1,endMode:'never'}}],{}],
  ['recurring weekend shift',[{field:'date',op:'is',value:{frequency:'monthly',start:'2026-11-07',interval:1,skipWeekend:true,weekendSolveMode:'after',endMode:'never'}}],{date:'2026-11-09'}],
 ];
 for(const [index,[name,conditions,changes,conditionsOp='and']] of checks.entries()) {
  await engine.send('rule-delete-all',(await engine.send('rules-get')).map(r=>r.id));
  const rule={id:identity(100+index),stage:null,conditionsOp,conditions,actions:[{op:'append-notes',value:' / matched'}]};
  const saved=await engine.send('rule-add',rule);assert.ok(!saved.error,JSON.stringify(saved));
  const input={...transaction,...changes};
  result.cases.push({name,rules:[rule],transaction:input,after:await engine.send('rules-run',{transaction:input})});
 }


 const file=new URL('../tests/fixtures/rules-26.10.0.json',import.meta.url);
 if(process.argv.includes('--check')){
  const normalize=x=>{let text=JSON.stringify(x,(key,value)=>key==='subtransactions'?value.map(({id,...child})=>child):value);for(const [name,id] of Object.entries(x.ids))text=text.replaceAll(id,name.toUpperCase());return text;};
  assert.equal(normalize(JSON.parse(await readFile(file,'utf8'))),normalize(result));
 } else {await writeFile(file,JSON.stringify(result,null,2)+'\n');await writeFile(new URL('../tests/fixtures/rules-base-26.10.0.zip',import.meta.url),archive);}
 console.log('PASS independent rule stages and chained payee/category/notes');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
