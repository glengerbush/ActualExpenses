import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
const require=createRequire(new URL('../../actualExpenses/tools/compatibility/package.json',import.meta.url));
const api=require('@actual-app/api');
assert.equal(JSON.parse(await readFile(require.resolve('@actual-app/api').replace(/dist\/index\.js$/,'package.json'),'utf8')).version,'26.10.0');
const directory=await mkdtemp(tmpdir()+'/register-query-oracle-');
const input=[
 {id:'12000000-0000-4000-8000-000000000001',amount:-1250,date:'2026-01-02',notes:'Coffee #Cafe #Work',cleared:true},
 {id:'12000000-0000-4000-8000-000000000002',amount:-375,date:'2026-01-03',notes:'Coffee beans ##Cafe',cleared:false},
 {id:'12000000-0000-4000-8000-000000000003',amount:2400,date:'2026-02-02',notes:'Refund #Work',cleared:true},
 {id:'12000000-0000-4000-8000-000000000004',amount:-1300,date:'2027-01-02',notes:'COFFEE #Cafe',cleared:false},
];
input.push(...['café','cafe','١','1'].map((notes,i)=>({id:'12000000-0000-4000-8000-00000000000'+(i+5),amount:-100,date:'2030-06-15',notes,cleared:false})));
input.push(...['\ufeff','\u0085','😀','a','\u2028','\u2029','\r','\n','\u00a0','😀😀','a\n','u','uu','ſ','K','x😀y','\u0002'].map((notes,i)=>({id:'12000000-0000-4000-8000-'+String(100+i).padStart(12,'0'),amount:-100,date:'2030-06-15',notes,cleared:false})));
const cases=[
 ...['^\\s$','^[^\\s]$','^.$','^..$','^\\ud83d\\ude00$','^\\ud83d.$','^[😀]$','^😀+$','^(.)\\1$','^(?<part>.)\\k<part>$','^\\u{2}$','^\\2$','a$','(?<=x).(?=y)'].map(pattern=>({name:'UTF16 and Annex B '+pattern,query:{filter:{conditions:[{field:'notes',op:'matches',value:pattern}]}},aql:{notes:{$transform:'$lower',$regexp:pattern.toLowerCase()}}})),
 ...['?','??','s','k'].map(value=>({name:'JavaScript LIKE '+value,query:{filter:{conditions:[{field:'notes',op:'contains',value}]}},aql:{notes:{$transform:'$lower',$like:'%'+value+'%'}}})),
 ...['^\\w+$','^\\d+$','\\bcaf\\b','^\\W+$','^\\D+$','caf\\B'].map(pattern=>({name:'JavaScript regex '+pattern,query:{filter:{conditions:[{field:'notes',op:'matches',value:pattern}]}},aql:{notes:{$transform:'$lower',$regexp:pattern.toLowerCase()}}})),
 {name:'single-character wildcard',query:{filter:{conditions:[{field:'notes',op:'contains',value:'coff?e'}]}},aql:{notes:{$like:'%coff?e%'}}},
 {name:'literal underscore',query:{filter:{conditions:[{field:'notes',op:'contains',value:'coff_e'}]}},aql:{notes:{$like:'%coff_e%'}}},
 {name:'escaped percent',query:{filter:{conditions:[{field:'notes',op:'contains',value:'coffee\\%'}]}},aql:{notes:{$like:'%coffee\\%%'}}},
 {name:'negative coffee',query:{filter:{op:'and',conditions:[{field:'notes',op:'contains',value:'coffee'},{field:'amount',op:'lt',value:0}]}},aql:{$and:[{notes:{$like:'%coffee%'}},{amount:{$lt:0}}]}},
 {name:'regex',query:{filter:{conditions:[{field:'notes',op:'matches',value:'^coffee.*beans'}]}},aql:{notes:{$transform:'$lower',$regexp:'^coffee.*beans'}}},
 {name:'approximate outflow',query:{filter:{conditions:[{field:'amount',op:'isapprox',value:1250,options:{outflow:true}}]}},aql:{$and:[{amount:{$lt:0}},{amount:{$transform:'$neg',$gte:1156}},{amount:{$transform:'$neg',$lte:1344}}]}},
 {name:'between',query:{filter:{conditions:[{field:'amount',op:'isbetween',value:{num1:-400,num2:-1300}}]}},aql:{amount:[{$gte:-1300},{$lte:-400}]}},
 {name:'month',query:{filter:{conditions:[{field:'date',op:'eq',value:'2026-01'}]}},aql:{$and:[{date:{$gte:'2026-01-00'}},{date:{$lte:'2026-01-99'}}]}},
 {name:'tag identity',query:{filter:{conditions:[{field:'notes',op:'hasTags',value:'#Cafe'}]}},aql:{notes:{$regexp:'(?<!#)#Cafe([\\s#]|$)'}}},
 {name:'or',query:{filter:{op:'or',conditions:[{field:'amount',op:'gt',value:0},{field:'cleared',op:'eq',value:false}]}},aql:{$or:[{amount:{$gt:0}},{cleared:false}]}},
];
try{
 await api.init({dataDir:directory});let account;await api.runImport('Register query oracle',async()=>{account=await api.createAccount({name:'Spending'},0);});await api.addTransactions(account,input);
 const expected=[];
 for(const item of cases){const response=await api.aqlQuery(api.q('transactions').filter({account}).filter(item.aql).select(['id','amount','cleared']).orderBy({id:'asc'}));const rows=response.data;expected.push({...item,ids:rows.map(r=>r.id),balances:{total:rows.reduce((n,r)=>n+r.amount,0),cleared:rows.filter(r=>r.cleared).reduce((n,r)=>n+r.amount,0),uncleared:rows.filter(r=>!r.cleared).reduce((n,r)=>n+r.amount,0)}});}
 const text=JSON.stringify({oracle:'@actual-app/api@26.10.0',source_commit:'2bebdbaae0b701120a8cad9d56aafd64fa04c577',description:'Published AQL output; filter translations follow pinned conditionsToAQL. No production dataset.',input,cases:expected},null,2)+'\n';const path=new URL('../tests/fixtures/register-query.json',import.meta.url);
 if(process.argv.includes('--check'))assert.equal(await readFile(path,'utf8'),text);else await writeFile(path,text);
 console.log('PASS pinned Actual 26.10 register filter fixture');
}finally{await api.shutdown();await rm(directory,{recursive:true,force:true});}
