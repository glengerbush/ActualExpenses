"""Genuine schema21 fixture, created only through the preserved executable's API.
No live data, current executable, version relabeling, or SQL mutations.
"""
import hashlib,io,json,os,shutil,sqlite3,subprocess,sys,tempfile,urllib.request,zipfile
from pathlib import Path
binary=Path(sys.argv[1]).resolve();digest=hashlib.sha256(binary.read_bytes()).hexdigest()
assert digest=='61a29c48caf58cfeb8908c743e3da96718f8109904ff32c868ce1878781f8c26'
fixtures=Path(__file__).resolve().parents[1]/'tests/fixtures';stem=fixtures/'reporting-schema21'
assert not any(stem.with_suffix(ext).exists() for ext in ['.json','.zip','.sqlite3'])
def identity(n):return f'33340021-0000-4000-8000-{n:012}'
with tempfile.TemporaryDirectory(prefix='reporting-schema21-') as directory:
 env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':directory};env.pop('ACTUALBUDGET_PUBLIC_ORIGIN',None)
 process=subprocess.Popen([str(binary)],env=env,stdout=subprocess.PIPE,text=True)
 try:
  line=process.stdout.readline().strip();assert line.startswith('LISTENING '),line;url='http://'+line.removeprefix('LISTENING ')
  def call(path,body=None,key='read',headers=None):
   request=urllib.request.Request(url+'/api/v1'+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json','Idempotency-Key':key,**(headers or {})})
   with urllib.request.urlopen(request,timeout=15) as response:return json.load(response) if response.headers.get('Content-Type','').startswith('application/json') else response.read()
  budget=identity(1);root='/budgets/'+budget
  created=call('/budgets',{'id':budget,'name':'Before reporting','budget_type':'tracking'},'budget21');metadata=call(root)['data'];headers={'X-Actual-Budget-Epoch':metadata['epoch']}
  call(root+'/accounts',{'id':identity(2),'name':'Spending','opening_balance':12345,'opening_date':'2026-11-01'},'account21')
  body={'id':identity(3),'account_id':identity(2),'amount':-345,'date':'2026-11-02','notes':'Before reporting','cleared':True}
  receipt=call(root+'/transactions',body,'expense21',headers)
  register=call(root+'/transactions');assert register['balances']['total']==12000
  companion=call(root+'/companion?account_id='+identity(2))
  backup=call(root+'/backups',{'label':'Before reporting'},'backup21',headers);archive=call(root+'/backups/'+backup['data']['id']+'/download')
  with zipfile.ZipFile(io.BytesIO(archive)) as contents:assert json.loads(contents.read('manifest.json'))['schema_version']==21
  evidence={'schema_version':21,'source_commit':'78610a9','executable_sha256':digest,'created':created,'metadata':metadata,'account':identity(2),'body':body,'key':'expense21','headers':headers,'receipt':receipt,'register':register,'companion':companion,'archive_sha256':hashlib.sha256(archive).hexdigest()}
 finally:process.terminate();process.wait(timeout=15)
 database=Path(directory)/'actualbudget.sqlite3'
 with sqlite3.connect(f'file:{database}?mode=ro&immutable=1',uri=True) as db:
  assert db.execute('PRAGMA user_version').fetchone()[0]==21
  assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
  assert db.execute('PRAGMA foreign_key_check').fetchall()==[]
 evidence['database_sha256']=hashlib.sha256(database.read_bytes()).hexdigest();shutil.copyfile(database,stem.with_suffix('.sqlite3'));stem.with_suffix('.zip').write_bytes(archive);stem.with_suffix('.json').write_text(json.dumps(evidence,indent=2)+'\n')
print('PASS genuine schema21 money, companion expense view, original receipts and native backup')
