"""Capture a genuine schema12 database/archive through the fixed old public server."""
import hashlib, json, os, pathlib, shutil, sqlite3, subprocess, tempfile, urllib.request
base=pathlib.Path(__file__).resolve().parents[1]
binary=pathlib.Path(os.environ['ACTUALBUDGET_SCHEMA12_BINARY'])
fixture=base/'tests/fixtures'
assert hashlib.sha256(binary.read_bytes()).hexdigest() == '66e3295af5dc5bd30fd7e477e89ee16899dbcf335dd19271cef28d371a6df702', 'Use the immutable pre-transfer schema12 server'
id=lambda n:f'15120000-0000-4000-8000-{n:012}'
with tempfile.TemporaryDirectory(prefix='reconciliation-schema12-') as directory:
 env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0'}
 env.pop('ACTUALBUDGET_PUBLIC_ORIGIN',None)
 process=subprocess.Popen([str(binary)],env=env,stdout=subprocess.PIPE,text=True)
 url='http://'+process.stdout.readline().strip().removeprefix('LISTENING ')
 def call(path,body=None,key=None,method=None,headers=None):
  raw=body if isinstance(body,bytes) else json.dumps(body).encode() if body is not None else None
  request=urllib.request.Request(url+'/api/v1'+path,data=raw,method=method or ('POST' if raw else 'GET'),headers={**({'Content-Type':'application/zip' if isinstance(body,bytes) else 'application/json'} if raw else {}),**({'Idempotency-Key':key} if key else {}),**(headers or {})})
  with urllib.request.urlopen(request) as response:
   data=response.read()
   return data if response.headers.get_content_type()=='application/zip' else json.loads(data)
 try:
  root='/budgets/'+id(1)
  call('/budgets',{'id':id(1),'name':'Schema12 reconciliation'},'budget')
  account_body={'id':id(2),'name':'Checking','opening_balance':0,'opening_date':'2026-01-01'}
  account_receipt=call(root+'/accounts',account_body,'account')
  call(root+'/payees',{'id':id(3),'name':'Merchant'},'payee')
  call(root+'/transactions',{'id':id(4),'account_id':id(2),'date':'2026-02-28','amount':-300,'cleared':True,'payee_id':id(3)},'parent')
  split_path=root+'/transactions/'+id(4)+'/splits'
  split_body={'snapshot':call(split_path)['snapshot'],'children':[{'id':id(5),'amount':-100,'payee_id':id(3)},{'id':id(6),'amount':-200,'payee_id':id(3)}]}
  split_receipt=call(split_path,split_body,'split')
  reconciliation_path=root+'/accounts/'+id(2)+'/reconciliation'
  lock_body={'action':'lock','statement_balance':-300,'snapshot':call(reconciliation_path)['data']['snapshot'],'date':'2026-02-28'}
  lock_receipt=call(reconciliation_path,lock_body,'lock')
  locked_split=call(split_path)['data']
  state=call(root+'/register-settings')['data']
  call(root+'/register-settings',{'revision':0,'settings':{'views':{'all':{'query':{'hide_reconciled':True}}},'saved_filters':[]}},'settings',method='PATCH',headers={'X-Actual-Budget-Epoch':state['epoch']})
  blocked=call('/imports/actual',(fixture/'import-transfers-26.10.0.zip').read_bytes(),'blocked-source')
  assert blocked['data']['status']=='blocked'
  imported=call('/imports/actual',(fixture/'reconciliation-import.zip').read_bytes(),'split-source')
  assert imported['data']['status']=='ready',imported
  call('/imports/'+imported['data']['id']+'/activate',{},'activate-split-source')
  active='/budgets/'+imported['data']['budget_id']
  backup=call(root+'/backups',{'label':'Schema12 split and register settings'},'backup')
  archive=call(root+'/backups/'+backup['data']['id']+'/download')
  assert isinstance(archive,bytes)
  (fixture/'reconciliation-schema12.zip').write_bytes(archive)
  metadata={'source_commit':'963e893','server_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'budget':id(1),'account':id(2),'parent':id(4),'account_body':account_body,'account_receipt':account_receipt,'split_body':split_body,'split_receipt':split_receipt,'lock_body':lock_body,'lock_receipt':lock_receipt,'locked_split':locked_split,'account_state':call(root+'/accounts/'+id(2))['data'],'settings':call(root+'/register-settings')['data']['settings'],'blocked_receipt':blocked,'active_budget':imported['data']['budget_id'],'active_register':call(active+'/transactions'),'active_accounts':call(active+'/accounts'),'active_import':imported['data']['id']}
 finally:
  process.terminate();process.wait()
 with sqlite3.connect(pathlib.Path(directory)/'actualbudget.sqlite3') as db:
  assert db.execute('PRAGMA user_version').fetchone()[0]==12
  db.execute('PRAGMA wal_checkpoint(TRUNCATE)')
 shutil.copyfile(pathlib.Path(directory)/'actualbudget.sqlite3',fixture/'reconciliation-schema12.sqlite3')
 (fixture/'reconciliation-schema12.json').write_text(json.dumps(metadata,indent=2)+'\n')
print('PASS genuine schema12 reconciliation upgrade fixtures')
