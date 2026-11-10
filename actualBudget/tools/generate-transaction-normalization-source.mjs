import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {loadSourceNormalization} from './transaction-import-source-runtime.mjs';

// Translation affects only presentation. Record its source key/arguments rather
// than choosing localized error text or duplicating date/amount logic.
const translate=(key,args={})=>JSON.stringify({key,args});
const {finalize,sourceFiles}=await loadSourceNormalization(translate);
const categories=[{id:'25260000-0000-4000-8000-000000000003',name:'Food'}];
const defaults={reconcile:true,fieldMappings:{date:'Date',payee:'Payee',amount:'Amount',notes:'Memo',category:'Category',inOut:'Direction',outflow:'Out',inflow:'In'},filetype:'csv',parseDateFormat:'yyyy mm dd',splitMode:false,inOutMode:false,outValue:'Debit',flipAmount:false,multiplierAmount:'1',categories,clearOnImport:true,importNotes:true};
const base={Date:'2026-11-09',Payee:'Cafe',Amount:'-12.34',Memo:'Import note',Category:'Food'};
const cases=[];
function add(id,rows=[base],options={}){
  const input={...defaults,...options,transactions:rows.map((row,i)=>({trx_id:String(i),selected:true,selected_merge:true,existing:false,ignored:false,...row}))};
  const result=JSON.parse(JSON.stringify(finalize(input)));
  cases.push({id,input,result});
}
add('mapped-csv');
for(const [format,date] of [['yyyy mm dd','20261109'],['yy mm dd','26/11/09'],['mm dd yyyy','11/09/2026'],['mm dd yy','11/09/26'],['dd mm yyyy','09/11/2026'],['dd mm yy','09/11/26']])add(`date-${format}`,[{...base,Date:date}],{parseDateFormat:format});
for(const [id,date] of [['english-month','9 November 2026'],['invalid','31/02/2026'],['leap-valid','29/02/2024'],['leap-invalid','29/02/2025'],['empty',''],['extra-groups','09/11/2026 12:34']])add(`date-${id}`,[{...base,Date:date}],{parseDateFormat:'dd mm yyyy'});
for(const [id,amount] of [['positive','12.34'],['negative-half','-1.005'],['positive-half','1.005'],['negative-exact-half','-1.1250'],['positive-exact-half','1.1250'],['extra-precision','12.3456'],['comma','12,34'],['thousands','1,234.56'],['currency','$ 12.34'],['invalid','word'],['empty',''],['zero','0'],['missing',null]])add(`amount-${id}`,[{...base,Amount:amount}]);
for(const multiplierAmount of ['2','0','-1','0.125','bad','2tail'])add(`multiplier-${multiplierAmount}`,[base],{multiplierAmount});
add('flip',[base],{flipAmount:true});
add('flip-multiplier',[base],{flipAmount:true,multiplierAmount:'2'});
add('split-both',[{...base,Out:'12.34',In:'50.00'}],{splitMode:true});
add('split-inflow',[{...base,Out:'',In:'50.00'}],{splitMode:true});
add('split-negative-out',[{...base,Out:'-12.34',In:'50.00'}],{splitMode:true});
add('direction-out',[{...base,Amount:'12.34',Direction:'Debit'}],{inOutMode:true});
add('direction-case-sensitive',[{...base,Amount:'12.34',Direction:'debit'}],{inOutMode:true});
add('direction-negative',[{...base,Amount:'-12.34',Direction:'Credit'}],{inOutMode:true});
add('category-case-sensitive',[{...base,Category:'food'}]);
add('category-no-creation',[{...base,Category:'Unknown'}]);
add('category-id-not-name',[{...base,Category:categories[0].id}]);
add('no-notes',[base],{importNotes:false});
add('not-cleared',[base],{clearOnImport:false});
add('unselected',[{...base,selected:false}]);
add('unselected-reconcile-disabled',[{...base,selected:false}],{reconcile:false});
add('matched-display-row',[{...base,isMatchedTransaction:true}]);
add('force-add-existing',[{...base,existing:true,selected_merge:false}]);
add('force-add-ignored',[{...base,ignored:true}]);
add('invalid-second-row',[base,{...base,Date:'bad'}]);
const parsed=JSON.parse(await readFile(new URL('../tests/fixtures/transaction-parser-source.json',import.meta.url),'utf8'));
for(const item of parsed.cases.filter(item=>item.source_file&&item.result.errors.length===0)){
  add(`format-${item.id}`,item.result.transactions,{fieldMappings:null,filetype:item.filename.split('.').at(-1),parseDateFormat:item.filename.endsWith('.qif')?'mm dd yyyy':'yyyy mm dd',importNotes:item.options.importNotes!==false});
}
const output={oracle:'Actual 26.10.0 unchanged frontend import finalization',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',source_files:sourceFiles,error_presentation:'Translation calls recorded as JSON key/args; no localization logic asserted.',cases};
assert.equal(new Set(cases.map(item=>item.id)).size,cases.length);
const fixture=new URL('../tests/fixtures/transaction-normalization-source.json',import.meta.url);
if(process.argv.includes('--check'))assert.deepEqual(output,JSON.parse(await readFile(fixture,'utf8')));
else if(process.argv.includes('--extend')){
  const previous=JSON.parse(await readFile(fixture,'utf8'));
  assert.deepEqual({...output,cases:previous.cases},previous);
  for(const item of previous.cases)assert.deepEqual(cases.find(next=>next.id===item.id),item);
  await writeFile(fixture,JSON.stringify(output,null,2)+'\n');
}else await writeFile(fixture,JSON.stringify(output,null,2)+'\n',{flag:'wx'});
console.log(`PASS ${cases.length} unchanged frontend normalization observations`);
