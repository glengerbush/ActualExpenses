// Genuine pinned-engine template archive; the historical simple-rule ZIP stays immutable.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/rules-requirement-');
const source=await readFile(new URL('../tests/fixtures/import-unsupported-rule-26.10.0.zip',import.meta.url));
try{
 const engine=await api.init({dataDir:directory});
 await api.importBudget(source,{type:'actual'});
 const original=await api.getRules();assert.equal(original.length,1);
 const rule={id:'19190000-0000-4000-8000-000000009942',stage:'post',conditionsOp:'and',conditions:[{field:'notes',op:'contains',value:'template'}],actions:[{op:'set',field:'notes',value:'',options:{template:'{{payee_name}}'}}]};
 const created=await engine.send('rule-add',rule);assert.ok(!created.error,JSON.stringify(created));
 const saved=(await api.getRules()).find(r=>r.id===rule.id);assert.deepEqual(saved.actions.map(({type,...action})=>action),rule.actions);
 const archive=await api.exportBudget();
 const metadata={oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',source_archive_sha256:createHash('sha256').update(source).digest('hex'),archive_sha256:createHash('sha256').update(archive).digest('hex'),requirement_ticket:42,rule:saved};
 await writeFile(new URL('../tests/fixtures/rules-template-requirement-26.10.0.zip',import.meta.url),archive);
 await writeFile(new URL('../tests/fixtures/rules-template-requirement-26.10.0.json',import.meta.url),JSON.stringify(metadata,null,2)+'\n');
 console.log('PASS genuine pinned template archive; historical simple-rule source unchanged');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
