// Executes pinned Actual handlers through init().send; no replacement code participates.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const require = createRequire(process.env.ACTUAL_ORACLE_PACKAGE ?? new URL('../../actualExpenses/tools/compatibility/package.json', import.meta.url));
const api = require('@actual-app/api');
assert.equal(JSON.parse(await readFile(join(require.resolve('@actual-app/api'),'../../package.json'),'utf8')).version, '26.10.0');
const directory = await mkdtemp(join(tmpdir(), 'locations-oracle-'));
const labels=new Map(), payees=[], locations=[];
const alias=(id,label)=>(labels.set(id,label),id);
let account; const snapshots=[];
const point={latitude:40,longitude:-74};
try {
 const {send}=await api.init({dataDir:directory});
 await api.runImport('Payee locations oracle',async()=>{
  account=await api.createAccount({name:'Daily'},0);
  for(let i=0;i<13;i++)payees.push(alias(await api.createPayee({name:`Shop ${i.toString().padStart(2,'0')}`}),`shop${i}`));
 });
 const save=async(p,lat,lon,label)=>{const id=alias(await send('payee-location-create',{payeeId:payees[p],latitude:lat,longitude:lon}),label);locations.push({id,payee_id:payees[p],latitude:lat,longitude:lon});return id;};
 for(let i=0;i<11;i++)await save(i,40+i*0.0001,-74,`location${i}`);
 await save(0,41,-74,'far-branch');
 await save(0,40.00005,-74,'second-near-branch');
 await save(11,40.00449,-74,'inside500');
 await save(12,40.00450,-74,'outside500');
 const snap=async(step,position=point)=>{const rows=await send('payees-get-nearby',position);snapshots.push({step,position,nearby:rows.map(r=>({payee:labels.get(r.payee.id),location:labels.get(r.location.id),distance:r.location.distance}))});};
 await snap('nearest-ten');
 await snap('500-meter-boundary',{latitude:40.00898,longitude:-74});
 await send('payee-location-delete',{id:locations[0].id});
 await snap('forgot-closest-retains-second');
 await api.mergePayees(payees[1],[payees[0]]);
 await snap('merged-payee-locations-not-moved');
 await api.deletePayee(payees[2]);
 await snap('removed-payee-excluded');
 const all=await send('payee-locations-get',{});
 const result={oracle:'@actual-app/api@26.10.0 init().send pinned handlers',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',snapshots,retained_locations:all.map(r=>labels.get(r.id)).sort()};
 const fixture=new URL('../tests/fixtures/payee-locations.json',import.meta.url);
 if(process.argv.includes('--check'))assert.deepEqual(result,JSON.parse(await readFile(fixture,'utf8')));
 else {
  await writeFile(fixture,JSON.stringify(result,null,2)+'\n');
  await writeFile(new URL('../tests/fixtures/payee-locations-import.zip',import.meta.url),await api.exportBudget());
  await writeFile(new URL('../tests/fixtures/payee-locations-import.json',import.meta.url),JSON.stringify({account,payees,locations,labels:Object.fromEntries(labels)},null,2)+'\n');
 }
 console.log('PASS pinned location capture/nearest/forget/merge/remove oracle');
} finally {await api.shutdown();await rm(directory,{recursive:true,force:true});}
