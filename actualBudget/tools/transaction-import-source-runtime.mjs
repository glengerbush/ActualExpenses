// Shared harness for executing unchanged pinned frontend import logic.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import vm from 'node:vm';

export async function loadSourceNormalization(translate = () => { throw new Error('Unexpected invalid row'); }) {
  const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
  const requireWeb=createRequire(new URL('../web/package.json',import.meta.url));
  const ts=requireWeb('typescript'), dateFns=require('date-fns');
  assert.equal(require('date-fns/package.json').version,'4.4.0');
  const base=new URL('../reference/actual-26.10.0/',import.meta.url);
  const names={
    utils:'packages/desktop-client/src/components/modals/ImportTransactionsModal/utils.ts',
    modal:'packages/desktop-client/src/components/modals/ImportTransactionsModal/ImportTransactionsModal.tsx',
    util:'packages/loot-core/src/shared/util.ts',
  };
  const sources=Object.fromEntries(await Promise.all(Object.entries(names).map(async([k,path])=>[k,await readFile(new URL(path,base),'utf8')])));
  function section(source,start,end){const a=source.indexOf(start),b=source.indexOf(end,a+start.length);assert.ok(a>=0&&b>a,`Missing source boundary ${start}`);return source.slice(a,b);}
  function execute(code,dependencies={}){
    const context={exports:{},Date:globalThis.Date,...dependencies};
    vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
    return context.exports;
  }
  const sourceUtil=execute(
    section(sources.util,'export const MAX_SAFE_NUMBER =','export function safeNumber(')+
    section(sources.util,'export function amountToInteger(','export function sortByKey<'),
  );
  const frontend=execute(sources.utils,{require(name){
    if(name==='date-fns')return dateFns;
    if(name==='@actual-app/core/shared/util')return sourceUtil;
    if(name==='@actual-app/core/shared/months')return {format(){throw new Error('Display formatting is outside this normalization tracer');}};
    throw new Error(`Unexpected source dependency ${name}`);
  }});
  const finalLoop=section(sources.modal,'    const finalTransactions = [];','\n    if (errorMessage) {');
  const finalize=execute(
    sources.modal.slice(sources.modal.indexOf('function isOfxFile('))+
    `\nexports.finalize=function(options){
      const {transactions,reconcile,fieldMappings,filetype,parseDateFormat,splitMode,inOutMode,outValue,flipAmount,multiplierAmount,categories,clearOnImport,importNotes}=options;
      ${finalLoop}
      return {transactions:finalTransactions,error:errorMessage??null};
    };`,
    {...frontend,...sourceUtil,t:translate},
  ).finalize;
  const preferenceWrites=section(sources.modal,'    if (!isOfxFile(filetype) && !isCamtFile(filetype)) {','\n    importTransactions.mutate(');
  const saveOptions=execute(sources.modal.slice(sources.modal.indexOf('function isOfxFile('))+`
    exports.saveOptions=function(options){
      const {accountId,filetype,parseDateFormat,fallbackMissingPayeeToMemo,ofxSwapPayeeAndMemo,fieldMappings,delimiter,csvEncoding,hasHeaderRow,skipStartLines,skipEndLines,inOutMode,outValue,flipAmount,importNotes,qifSwapPayeeAndMemo,camtSwapPayeeAndMemo,reimportDeleted}=options;
      const preferences={};const savePrefs=values=>Object.assign(preferences,values);
      ${preferenceWrites}
      return preferences;
    };`).saveOptions;
  return {frontend,sourceUtil,finalize,saveOptions,sourceFiles:Object.fromEntries(Object.entries(names).map(([k,path])=>[path,createHash('sha256').update(sources[k]).digest('hex')]))};
}
