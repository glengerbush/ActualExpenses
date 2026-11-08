// Execute the pinned UI arithmetic source and public amount converter, never replacement code.
import assert from 'node:assert/strict';
import {createRequire,stripTypeScriptTypes} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
const source=new URL('../reference/actual-26.10.0/packages/loot-core/src/shared/',import.meta.url);
const directory=await mkdtemp(join(tmpdir(),'actual-calculator-oracle-'));
const cases=[
 ['1.005','1.005'],['-1.005','-1.005'],['0.005','0.005'],['-0.005','-0.005'],['2.675','2.675'],['-2.675','-2.675'],
 ['1/3','1/3'],['-1/3','-1/3'],['0.1+0.2','0.1+0.2'],['(5+3)*10','(5+3)*10'],['5+3*10','5+3*10'],['-2^2','-2^2'],['2^3^2','2^3^2'],['24/3*2','24/3*2'],
 ['1+2)foo','1+2)foo'],['(1+2)x','(1+2)x'],['10+20 trailing','10+20 trailing'],['1+2(3','1+2(3'],
 ['$1,222.45 + $2.55','1222.45+2.55'],['1\u202f222,45 + 2,55','1222.45+2.55','space-comma'],["1’222.45 + 2.55",'1222.45+2.55','apostrophe-dot'],
 ['1.5','1.5','comma-dot',0],['-1.5','-1.5','comma-dot',0],['90071992547409.91','90071992547409.91'],
];
try {
 for (const name of ['currencies','util','arithmetic']) {
  let js=stripTypeScriptTypes(await readFile(new URL(`${name}.ts`,source),'utf8'));
  js=js.replaceAll("from './util'","from './util.mjs'").replaceAll("from './currencies'","from './currencies.mjs'").replace("from 'date-fns'",`from '${pathToFileURL(require.resolve('date-fns'))}'`);
  await writeFile(join(directory,`${name}.mjs`),js);
 }
 const util=await import(pathToFileURL(join(directory,'util.mjs')));
 const {evalArithmetic}=await import(pathToFileURL(join(directory,'arithmetic.mjs')));
 const results=cases.map(([expression,canonical,number_format='comma-dot',decimal_places=2])=> {
  util.setNumberFormat({format:number_format,hideFraction:false});
  const value=evalArithmetic(expression,null);
  assert.notEqual(value,null,expression);
  return {expression,canonical,number_format,decimal_places,expected:api.utils.amountToInteger(value,decimal_places)};
 });
 const rendered=JSON.stringify({oracle:'Actual 26.10.0 UI arithmetic + public API amountToInteger',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',cases:results},null,2)+'\n';
 const target=new URL('../tests/fixtures/calculator.json',import.meta.url);
 if(process.argv.includes('--check')) assert.equal(await readFile(target,'utf8'),rendered);else await writeFile(target,rendered);
 console.log('Calculator oracle verified');
} finally {await rm(directory,{recursive:true,force:true});}
