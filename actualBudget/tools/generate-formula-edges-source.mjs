// Concrete formula joins: named condition metadata, invalid category regex,
// imported statement text, duplicate account names, volatile clock and preferences.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {formulaSourceRuntime} from './formula-source-runtime.mjs';
process.env.TZ='UTC';
const NativeDate=Date,clock='2026-11-09T12:00:00.000Z';
globalThis.Date=class extends NativeDate{constructor(...args){super(...(args.length?args:[clock]));}static now(){return NativeDate.parse(clock);}};
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url)),api=require('@actual-app/api'),
 directory=await mkdtemp(tmpdir()+'/formula-edges-source-'),root=new URL('../tests/fixtures/',import.meta.url),
 file=new URL('formula-edges-source.json',root),sha=v=>createHash('sha256').update(v).digest('hex'),
 reports=JSON.parse(await readFile(new URL('reports-source-26.10.json',root))),
 merges=JSON.parse(await readFile(new URL('transaction-merge-source.json',root))),
 base={filename:'reports-source-26.10.zip',sha256:reports.archive_sha256},check=process.argv.includes('--check');
async function load(seed){const bytes=await readFile(new URL(seed.filename,root));assert.equal(sha(bytes),seed.sha256);await api.importBudget(bytes,{filename:seed.filename});}
try{
 const engine=await api.init({dataDir:directory}),cases=[];
 const ledger=async()=>JSON.parse(JSON.stringify((await api.aqlQuery(api.q('transactions').options({splits:'all'}).select(['id','account','category','amount','date','payee','notes','parent_id','transfer_id','is_parent','is_child','cleared','reconciled','imported_payee']).orderBy('id'))).data));
 async function observe(id,seed,inputs,{prepare,locale='en-US',preferences}={}){
  await load(seed);const before=await ledger();
  if(prepare)await prepare();
  assert.deepEqual(await ledger(),before,'Preparation changes metadata/preferences only');
  const runtime=formulaSourceRuntime(api,engine),accounts=await api.getAccounts(),observations=[];
  for(const input of inputs)observations.push({...input,locale,output:await runtime.evaluate(input.formula,input.queries,input.namedExpressions,accounts,locale)});
  let archive=seed;
  if(prepare){const filename='formula-'+id+'-26.10.zip',bytes=check?await readFile(new URL(filename,root)):await api.exportBudget();
   if(!check)await writeFile(new URL(filename,root),bytes,{flag:'wx'});
   archive={filename,sha256:sha(bytes)};}
  cases.push({id,seed,archive,locale,...(preferences?{preferences}:{}),accounts:accounts.map(a=>({id:a.id,name:a.name,closed:a.closed,offbudget:a.offbudget})),ledger:before,source_hashes:runtime.hashes,observations});
 }
 const condition={field:'category',op:'is',value:reports.ids.food},
  named={Named:{conditions:[{...condition,customName:'Preserved name'}]}},
  invalid={Bad:{conditions:[{field:'category',op:'matches',value:'['}]}},
  markers=[
   {name:'query-named-marker',formula:'=QUERY("Named")',queries:named},
   {name:'query-falsy-marker',formula:'=QUERY("Named")',queries:{Named:{conditions:[{...condition,customName:''}]}}},
   {name:'categories-named-marker',formula:'=COUNTA(QUERY_EXTRACT_CATEGORIES("Named"))',queries:named},
   {name:'invalid-category-regex',formula:'=QUERY_EXTRACT_CATEGORIES("Bad")',queries:invalid},
   {name:'invalid-category-regex-budget',formula:'=BUDGET_QUERY("budgeted",QUERY_EXTRACT_CATEGORIES("Bad"),"2026-11","2026-11")',queries:invalid},
   {name:'volatile-today',formula:'=TODAY()'},
   {name:'volatile-now',formula:'=NOW()'},
  ];
 await observe('query-edges',base,markers);
 const imported=merges.cases.find(c=>c.id==='imported-payee-kept').archive;
 await observe('imported-text',imported,[
  ...['is','isNot','contains','matches'].map(op=>({name:'imported-'+op,formula:'=QUERY("Imported")',queries:{Imported:{conditions:[{field:'imported_payee',op,value:op==='matches'?'^bank cafe$':'Bank Cafe'}]}}})),
  {name:'imported-empty',formula:'=QUERY_COUNT("Empty")',queries:{Empty:{conditions:[{field:'imported_payee',op:'is',value:''}]}}},
 ]);
 await observe('duplicate-account',base,[
  {name:'duplicate-name',formula:'=BALANCE_OF("Duplicate")'},
  {name:'duplicate-first-id',formula:`=BALANCE_OF("${reports.ids.checking}")`},
  {name:'duplicate-second-id',formula:`=BALANCE_OF("${reports.ids.savings}")`},
 ],{prepare:async()=>{await api.updateAccount(reports.ids.checking,{name:'Duplicate',closed:true});await api.updateAccount(reports.ids.savings,{name:'Duplicate'});}});
 for(const configuration of [{id:'eur-preferences',locale:'de-DE',values:{'flags.currency':'true',defaultCurrencyCode:'EUR',numberFormat:'dot-comma',hideFraction:'true',currencySymbolPosition:'after',currencySpaceBetweenAmountAndSymbol:'true'}},
   {id:'jpy-preferences',locale:'ja-JP',values:{'flags.currency':'true',defaultCurrencyCode:'JPY',numberFormat:'comma-dot',hideFraction:'false',currencySymbolPosition:'before',currencySpaceBetweenAmountAndSymbol:'false'}}]){
  await observe(configuration.id,base,[
   {name:'preference-number',formula:'=FORMATNUMBER(1234.567)'},
   {name:'preference-currency',formula:'=FORMATCURRENCY(-1234.567)'},
   {name:'preference-explicit-decimals',formula:'=FORMATNUMBER(1234.567,3)'},
  ],{locale:configuration.locale,preferences:configuration.values,prepare:async()=>{for(const[id,value]of Object.entries(configuration.values))await engine.send('preferences/save',{id,value});}});
 }
 const output={source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',api:'26.10.0',clock,cases};
 if(check)assert.deepEqual(output,JSON.parse(await readFile(file)));
 else await writeFile(file,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
 console.log('PASS genuine formula edge source '+cases.reduce((sum,c)=>sum+c.observations.length,0));
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
