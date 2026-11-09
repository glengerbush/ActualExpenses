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
  ['null id regex',[{field:'payee',op:'matches',value:null}],{payee:ids.Shop}],
  ['null id contains',[{field:'payee',op:'contains',value:null}],{payee:ids.Shop}],
  ['null id not contains',[{field:'payee',op:'doesNotContain',value:null}],{payee:ids.Shop}],
  ['JS FEFF tag whitespace',[{field:'notes',op:'hasTags',value:'food\uFEFFbus'}],{notes:'#food #bus'}],
  ['JS nonwhitespace0085',[{field:'notes',op:'hasTags',value:'food\u0085bus'}],{notes:'#food #bus'}],
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


 const actionCases=[
  ['literal fields',[{op:'set',field:'amount',value:-999},{op:'set',field:'date',value:'2026-10-01'},{op:'set',field:'cleared',value:true},{op:'set',field:'notes',value:'set'},{op:'prepend-notes',value:'before:'},{op:'append-notes',value:':after'}],{}],
  ['clear references',[{op:'set',field:'category',value:null},{op:'set',field:'payee',value:null}],{category:ids.Food,payee:ids.Shop}],
  ['payee name existing',[{op:'set',field:'payee_name',value:'Shop'}],{}],
  ['payee name creation',[{op:'set',field:'payee_name',value:'Generated shop'}],{}],
  ['delete',[{op:'delete-transaction',value:null}],{}],
  ['split fixed percent remainders',[{op:'set-split-amount',value:-20,options:{method:'fixed-amount',splitIndex:1}},{op:'set',field:'category',value:ids.Food,options:{splitIndex:1}},{op:'set-split-amount',value:50,options:{method:'fixed-percent',splitIndex:2}},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:3}},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:4}},{op:'set',field:'category',value:ids.Travel,options:{splitIndex:4}}],{amount:-101}],
  ['split apply all notes',[{op:'append-notes',value:' parent'},{op:'set-split-amount',value:50,options:{method:'fixed-percent',splitIndex:1}},{op:'prepend-notes',value:'child:',options:{splitIndex:1}},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:2}}],{amount:-101}],
  ['child is not resplit',[{op:'append-notes',value:' child'},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:1}},{op:'set-split-amount',value:null,options:{method:'remainder',splitIndex:2}}],{is_child:true,parent_id:identity(999),amount:-101}],
 ];
 for(const [index,[name,actions,changes]] of actionCases.entries()) {
  await engine.send('rule-delete-all',(await engine.send('rules-get')).map(r=>r.id));
  const rule={id:identity(300+index),stage:null,conditionsOp:'and',conditions:[{field:'amount',op:'lt',value:0}],actions};
  const saved=await engine.send('rule-add',rule);assert.ok(!saved.error,JSON.stringify(saved));
  const input={...transaction,...changes},after=await engine.send('rules-run',{transaction:input});
  let new_payee_name;
  if(actions.some(a=>a.field==='payee_name')){const payee=(await api.getPayees()).find(p=>p.id===after.payee);new_payee_name=payee.name;if(new_payee_name==='Generated shop')ids.Generated=payee.id;}
  result.cases.push({name,rules:[rule],transaction:input,after,...(new_payee_name?{new_payee_name}:{})});
 }

 result.learning=[];
 const learningCases=[
  {name:'two votes insufficient',categories:['A','A'],changed:1},
  {name:'three of five',categories:['A','A','B','A','B'],changed:4},
  {name:'most recent uncategorized',categories:['A','A','A',null],changed:2},
  {name:'changed row outside latest five',categories:['A','B','B','B','B','B'],changed:0},
  {name:'closed account',categories:['A','A','A'],changed:2,closed:true},
  {name:'payee learning disabled',categories:['A','A','A'],changed:2,enabled:false},
  {name:'global learning disabled',categories:['A','A','A'],changed:2,global:false},
  {name:'manual no learning',categories:['A','A','A'],changed:2,learn:false},
  {name:'uncategorized edit never teaches',categories:['A','A','A'],changed:2,category:null},
  {name:'older than180 days',categories:['A','A','A','B'],days:[-183,-182,-181,0],changed:3},
  {name:'future beyond180 days',categories:['A','A','A','B'],days:[181,182,183,0],changed:3},
  {name:'manual category edit learns',categories:['B','B','A'],changed:2,category:'B'},
  {name:'duplicate setters both updated',categories:['B','B','B'],changed:2,existing:['A','A']},
 ];
 for(const [index,c] of learningCases.entries()) {
  const data={};
  await api.runImport('Learning oracle '+index,async()=>{
   data.account=await api.createAccount({name:'Cash'},c.closed?c.categories.length*100:0);
   const group=await api.createCategoryGroup({name:'Expenses',is_income:false});
   data.A=await api.createCategory({name:'A',group_id:group});data.B=await api.createCategory({name:'B',group_id:group});
   data.payee=await api.createPayee({name:'Learning shop',learn_categories:c.enabled!==false});
   data.rows=c.categories.map((category,i)=>({id:identity(10000+index*100+i),date:new NativeDate(frozen+(c.days?.[i]??i-5)*86400000).toISOString().slice(0,10),amount:-100,payee:data.payee,category:category?data[category]:null,notes:'Vote'+i}));
   await api.addTransactions(data.account,data.rows);
  });
  if(c.enabled===false)await api.updatePayee(data.payee,{learn_categories:false});
  if(c.closed)await api.updateAccount(data.account,{closed:true});
  await engine.send('preferences/save',{id:'learn-categories',value:String(c.global!==false)});
  for(const [i,category] of (c.existing??[]).entries())await engine.send('rule-add',{id:identity(20000+index*100+i),stage:null,conditionsOp:'and',conditions:[{field:'payee',op:'is',value:data.payee}],actions:[{op:'set',field:'category',value:data[category]}]});
  const category=Object.hasOwn(c,'category')?c.category:c.categories[c.changed];
  const update={id:data.rows[c.changed].id,category:category?data[category]:null};
  await engine.send('transactions-batch-update',{updated:[update],learnCategories:c.learn!==false&&c.global!==false});
  const learned=await api.getPayeeRules(data.payee);
  result.learning.push({...c,rows:data.rows.map(({payee,...r},i)=>({...r,category:c.categories[i]})),updated:{id:update.id,category},expected_categories:learned.filter(r=>r.stage===null&&r.conditions.length===1&&r.actions.length===1&&r.actions[0].op==='set'&&r.actions[0].field==='category').map(r=>r.actions[0].value===data.A?'A':r.actions[0].value===data.B?'B':'unexpected').sort()});
 }

 result.financial=[];
 const financialArchives=[];
 const financialCases=[
  {name:'one paired amount',actions:[{op:'set',field:'amount',value:-77}],both:false},
  {name:'both paired amounts',actions:[{op:'set',field:'amount',value:-77}],both:true},
  {name:'both paired notes',actions:[{op:'append-notes',value:' / rule'}],both:true},
  {name:'both paired delete',actions:[{op:'delete-transaction',value:null}],both:true},
  {name:'split transfer child notes',actions:[{op:'append-notes',value:' / rule'}],split:true,both:false},
  {name:'split transfer both notes',actions:[{op:'append-notes',value:' / rule'}],split:true,both:true},
 ];
 for(const [index,c] of financialCases.entries()) {
  let cash,other;
  await api.runImport('Rule financial oracle '+index,async()=>{cash=await api.createAccount({name:'Cash'},0);other=await api.createAccount({name:'Other'},0);});
  const payee=(await api.getPayees()).find(p=>p.transfer_acct===other).id;
  const root=identity(30000+index*100),childId=identity(30001+index*100),otherChild=identity(30002+index*100);
  await api.addTransactions(cash,[c.split?{id:root,date:'2026-11-08',amount:-200,notes:'Parent',subtransactions:[{id:childId,amount:-100,payee,notes:'Pair'},{id:otherChild,amount:-100,notes:'Other child'}]}:{id:root,date:'2026-11-08',amount:-100,payee,notes:'Pair'}],{runTransfers:true});
  async function state(){const left=await api.getTransactions(cash),right=await api.getTransactions(other);const flat=[...left.flatMap(r=>[r,...(r.subtransactions??[])]),...right.flatMap(r=>[r,...(r.subtransactions??[])])];return {cash:left.map(r=>({amount:r.amount,notes:r.notes??'',children:(r.subtransactions??[]).map(c=>({amount:c.amount,notes:c.notes??'',paired:!!c.transfer_id}))})),other:right.map(r=>({amount:r.amount,notes:r.notes??'',paired:!!r.transfer_id})),balances:[await api.getAccountBalance(cash),await api.getAccountBalance(other)],reciprocal_balanced:flat.filter(r=>r.transfer_id).every(r=>flat.some(p=>p.id===r.transfer_id&&p.transfer_id===r.id&&p.amount===-r.amount))};}
  const initial=await state();const left=await api.getTransactions(cash),right=await api.getTransactions(other);const selected=[c.split?left[0].subtransactions.find(r=>r.id===childId):left[0],...(c.both?[right[0]]:[])];assert.ok(selected.every(Boolean),c.name);
  financialArchives.push(await api.exportBudget());
  const applied=await engine.send('rule-apply-actions',{transactions:selected,actions:c.actions});assert.ok(applied&&Array.isArray(applied.errors),c.name);const after=await state();
  result.financial.push({...c,initial,after,errors:applied.errors});
 }
 const file=new URL('../tests/fixtures/rules-26.10.0.json',import.meta.url);
 if(process.argv.includes('--check')){
  const normalize=x=>{let text=JSON.stringify(x,(key,value)=>key==='subtransactions'?value.map(({id,...child})=>child):value);for(const [name,id] of Object.entries(x.ids))text=text.replaceAll(id,name.toUpperCase());return text;};
  assert.equal(normalize(JSON.parse(await readFile(file,'utf8'))),normalize(result));
 } else {await writeFile(file,JSON.stringify(result,null,2)+'\n');await writeFile(new URL('../tests/fixtures/rules-base-26.10.0.zip',import.meta.url),archive);for(const [i,data] of financialArchives.entries())await writeFile(new URL(`../tests/fixtures/rules-financial-${i}-26.10.0.zip`,import.meta.url),data);}
 console.log('PASS independent rule stages and chained payee/category/notes');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});globalThis.Date=NativeDate;}
