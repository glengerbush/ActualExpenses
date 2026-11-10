"""Capture real accepted schema22 via public HTTP; never rewrite source SQLite."""
import hashlib, json, os, select, shutil, sqlite3, subprocess, sys, tempfile, urllib.request, zipfile, io
from pathlib import Path
from datetime import datetime, timezone
fixtures=Path(__file__).resolve().parents[1]/"tests/fixtures"
binary=Path(sys.argv[1]).resolve()
sha=lambda b:hashlib.sha256(b).hexdigest()
binary_sha=sha(binary.read_bytes())
assert binary_sha=="fa0146112b54a00ab78f5b7b1c9d4363256efb25c45aff49356a3b45b22172e0"
outputs={e:fixtures/("bank-schema22"+e)for e in [".json",".zip",".sqlite3"]}
assert not any(p.exists()for p in outputs.values()),"Original fixture already exists"
core=json.loads((fixtures/"bank-sync-source-26.10.json").read_text())
seed=next(c for c in core['cases']if c['id']=='manual-pending-booked-same-id')
linked=json.loads((fixtures/"bank-linked-source-26.10.json").read_text())['cases'][0]
clock=datetime.now(timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')
owner="28280000-0000-4000-8000-000000000022"
receipts=[]
with tempfile.TemporaryDirectory(prefix="bank-schema22-")as directory:
 env={**os.environ,"ACTUALBUDGET_DATA_DIR":directory,"ACTUALBUDGET_HOST":"127.0.0.1","ACTUALBUDGET_PORT":"0","ACTUALBUDGET_WEB_DIR":directory}
 env.pop('ACTUALBUDGET_PUBLIC_ORIGIN',None)
 process=subprocess.Popen([str(binary)],env=env,stdout=subprocess.PIPE,text=True)
 try:
  assert select.select([process.stdout],[],[],15)[0],"Old fixture server did not start"
  line=process.stdout.readline().strip();assert line.startswith('LISTENING '),line
  base='http://'+line.removeprefix('LISTENING ')+'/api/v1'
  def request(path,body=None,*,method=None,key='fixture',headers=None,status=200,raw=False):
   data=body if isinstance(body,bytes)else json.dumps(body,separators=(',',':')).encode()if body is not None else None
   h={'Idempotency-Key':key,**(headers or{})}
   if data is not None:h.setdefault('Content-Type','application/json')
   req=urllib.request.Request(base+path,data=data,headers=h,method=method or('POST'if data is not None else'GET'))
   with urllib.request.urlopen(req,timeout=15)as response:
    result=response.read();assert response.status==status
   return result if raw else json.loads(result)
  source=fixtures/'bank-sync-source'/seed['archive']['filename'];assert sha(source.read_bytes())==seed['archive']['sha256']
  staged=request('/imports/actual',source.read_bytes(),key='actual-seed',headers={'Content-Type':'application/zip'},status=201)['data']
  assert staged['status']=='ready',json.dumps(staged['report'].get('requirements'))
  activated=request('/imports/'+staged['id']+'/activate',{},key='activate',status=201)['data']
  budget=activated['id'];account=seed['ids']['account'];root='/budgets/'+budget
  epoch=request(root+'/checkpoint')['data']['epoch']
  def owned(counter,key):return {'X-Actual-Budget-Epoch':epoch,'X-Actual-History-Owner':owner,'X-Actual-Edit-Clock':clock+f'-{counter:04X}-2222222222222222'}
  for index,delimiter in enumerate([',',';']):
   contents=(delimiter.join(['date','payee','amount','notes'])+'\n'+delimiter.join(['2026-11-05','Original v22 merchant','-3.21'if index==0 else'-1.00','Original v22 file'])+'\n').encode()
   file=request(root+'/accounts/'+account+'/transaction-imports?filename=before-bank.csv',contents,key='file-'+str(index),headers={'Content-Type':'application/octet-stream'},status=201)['data']['id']
   body={'snapshot':request(root+'/checkpoint')['data']['snapshot'],'mapping':{'date':'date','payee':'payee','amount':'amount','notes':'notes'},'options':{'date_format':'yyyy mm dd','has_header_row':True,'delimiter':delimiter},'remember':True,'rows':[{'row':0,'action':'auto'}]}
   headers=owned(index,'remember-'+str(index));response=request(root+'/transaction-imports/'+file+'/commit',body,key='remember-'+str(index),headers=headers)
   receipts.append({'path':root+'/transaction-imports/'+file+'/commit','body':body,'key':'remember-'+str(index),'headers':headers,'response':response})
  expense='28280000-0000-4000-8000-000000000444'
  body={'id':expense,'account_id':account,'amount':-444,'date':'2026-11-07','notes':'Before upgrade'}
  response=request(root+'/transactions',body,key='owned-create',headers=owned(2,'owned-create'),status=201);receipts.append({'path':root+'/transactions','body':body,'key':'owned-create','headers':owned(2,'owned-create'),'response':response,'status':201})
  patch={'notes':'Original v22 owned edit'};response=request(root+'/transactions/'+expense,patch,method='PATCH',key='owned-edit',headers=owned(3,'owned-edit'));receipts.append({'path':root+'/transactions/'+expense,'method':'PATCH','body':patch,'key':'owned-edit','headers':owned(3,'owned-edit'),'response':response})
  linked_path=fixtures/'bank-sync-source'/linked['archive']['filename'];assert sha(linked_path.read_bytes())==linked['archive']['sha256']
  blocked=request('/imports/actual',linked_path.read_bytes(),key='linked-bank-stage',headers={'Content-Type':'application/zip'},status=201)['data'];assert blocked['status']=='blocked'
  pref_case=next(c for c in core['cases']if c['id']=='custom-payment-deposit-mappings')
  pref_path=fixtures/'bank-sync-source'/pref_case['archive']['filename'];assert sha(pref_path.read_bytes())==pref_case['archive']['sha256']
  pref_stage=request('/imports/actual',pref_path.read_bytes(),key='bank-prefs-stage',headers={'Content-Type':'application/zip'},status=201)['data'];assert pref_stage['status']=='blocked'
  nynab_path=fixtures/'ynab/nynab-demo.json'
  foreign=request('/imports/nynab?filename=protected-nynab.json',nynab_path.read_bytes(),key='nynab-stage',headers={'Content-Type':'application/json'},status=201)['data'];assert foreign['report']['source_format']=='nynab'
  snapshot={name:request(root+suffix,headers={'X-Actual-History-Owner':owner})for name,suffix in [('budget',''),('accounts','/accounts'),('transactions','/transactions'),('payees','/payees'),('categories','/categories'),('checkpoint','/checkpoint'),('settings','/accounts/'+account+'/transaction-import-settings?format=csv'),('history','/history')]}
  versions=snapshot['settings']['data']['field_versions'];assert versions['csv-delimiter-'+account]['value']==owned(1,'')['X-Actual-Edit-Clock'];assert versions['csv-mappings-'+account]['value']==owned(0,'')['X-Actual-Edit-Clock']
  point=request(root+'/backups',{'label':'Genuine accepted22 before bank23'},key='native-backup',headers={'X-Actual-Budget-Epoch':epoch},status=201)['data']
  native=request(root+'/backups/'+point['id']+'/download',raw=True)
  with zipfile.ZipFile(io.BytesIO(native))as z:assert json.loads(z.read('manifest.json'))['schema_version']==22
  outputs['.zip'].write_bytes(native)
 finally:
  process.terminate();process.wait(timeout=15)
 source_db=Path(directory)/'actualbudget.sqlite3'
 with sqlite3.connect(f'file:{source_db}?mode=ro&immutable=1',uri=True)as db:
  assert db.execute('pragma user_version').fetchone()[0]==22
  assert db.execute('pragma integrity_check').fetchone()[0]=='ok'
  assert not db.execute('pragma foreign_key_check').fetchall()
 shutil.copyfile(source_db,outputs['.sqlite3'])
 manifest={'schema_version':22,'binary_sha256':binary_sha,'accepted_commit':'0a284f2','budget_id':budget,'account_id':account,'owner':owner,'expense_id':expense,'snapshot':snapshot,'receipts':receipts,'blocked_actual_stage':blocked,'blocked_preference_stage':pref_stage,'foreign_stage':foreign,'source_files':[{'path':'bank-sync-source/'+seed['archive']['filename'],'sha256':seed['archive']['sha256']},{'path':'bank-sync-source/'+linked['archive']['filename'],'sha256':linked['archive']['sha256']},{'path':'bank-sync-source/'+pref_case['archive']['filename'],'sha256':pref_case['archive']['sha256']},{'path':'ynab/nynab-demo.json','sha256':sha(nynab_path.read_bytes())}],'scope':'Real22 DB includes global blocked SimpleFIN linkage and bank-preference stages plus an nYNAB stage. NativeZIP is active-budget-only; it excludes those other-budget global stages. Official sourceZIP has values but no exported CRDT clocks. Old receipt/history/epoch and two independent file preference HLCs are captured through HTTP.','artifacts':{key:{'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size}for key,p in outputs.items()if key!='.json'}}
 outputs['.json'].write_text(json.dumps(manifest,indent=2)+'\n')
 print('PASS genuine22 public capture, distinct preference clocks, receipts/history, blocked/foreign stages and readonly integrity')
