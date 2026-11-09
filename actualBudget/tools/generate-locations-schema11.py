"""Genuine schema11 public-API upgrade fixture, immutable 5a9efa1 binary supplied.

No SQLite/ZIP rewriting. Source locations were blocked before ticket46; verify
that upgrade refreshes the stage while preserving its exact old upload receipt.
"""
import io,json,os,shutil,subprocess,sys,tempfile,urllib.request,zipfile
from pathlib import Path
root=Path(__file__).resolve().parents[1];fixtures=root/'tests/fixtures'
with tempfile.TemporaryDirectory(prefix='locations-schema11-') as directory:
 p=subprocess.Popen([str(Path(sys.argv[1]).resolve())],env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':directory},stdout=subprocess.PIPE,text=True)
 try:
  url='http://'+p.stdout.readline().strip().removeprefix('LISTENING ')
  def call(path,body=None,key='read',method=None):
   raw=body if isinstance(body,bytes) else None if body is None else json.dumps(body).encode()
   request=urllib.request.Request(url+'/api/v1'+path,data=raw,method=method,headers={'Idempotency-Key':key,'Content-Type':'application/zip' if isinstance(body,bytes) else 'application/json'})
   with urllib.request.urlopen(request) as r:
    return json.load(r) if r.headers.get('Content-Type','').startswith('application/json') else r.read()
  body={'id':'46000000-0000-4000-8000-000000000001','name':'Schema11 native'}
  budget=call('/budgets',body,'native-budget');b=body['id'];base=f'/budgets/{b}'
  account={'id':'46000000-0000-4000-8000-000000000002','name':'Daily','opening_balance':12345,'opening_date':'2026-01-01'}
  created=call(base+'/accounts',account,'native-account')
  stage=call('/imports/actual',(fixtures/'payee-locations-import.zip').read_bytes(),'blocked-locations')
  assert stage['data']['status']=='blocked',stage
  backup=call(base+'/backups',{'label':'Schema11 original'},'backup')
  archive=call(base+'/backups/'+backup['data']['id']+'/download')
  assert json.loads(zipfile.ZipFile(io.BytesIO(archive)).read('manifest.json'))['schema_version']==11
  (fixtures/'locations-schema11.zip').write_bytes(archive)
  (fixtures/'locations-schema11.json').write_text(json.dumps({'source_commit':'5a9efa1','budget':budget,'account':created,'account_body':account,'stage':stage},indent=2)+'\n')
 finally:p.terminate();p.wait()
 shutil.copyfile(Path(directory)/'actualbudget.sqlite3',fixtures/'locations-schema11.sqlite3')
print('PASS genuine schema11 SQLite/ZIP and blocked location upload')
