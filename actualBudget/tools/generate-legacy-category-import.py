"""Public old-server fixture: metadata enrichment must preserve post-import edits."""
import json,os,subprocess,tempfile,urllib.request,sys,shutil
from pathlib import Path
root=Path(__file__).resolve().parents[1];binary=str(Path(sys.argv[1]).resolve());archive=root/'tests/fixtures/import-26.10.0.zip'
for mode in ['active','staged','blocked']:
 with tempfile.TemporaryDirectory(prefix='category-legacy-import-') as directory:
  process=subprocess.Popen([binary],env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':directory},stdout=subprocess.PIPE,text=True)
  try:
   url='http://'+process.stdout.readline().strip().removeprefix('LISTENING ')
   def call(path,body=None,key='read',method=None):
    if isinstance(body,bytes):data=body;content='application/zip'
    else:data=None if body is None else json.dumps(body).encode();content='application/json'
    request=urllib.request.Request(url+'/api/v1'+path,data=data,method=method,headers={'Content-Type':content,'Idempotency-Key':key})
    with urllib.request.urlopen(request) as response:return json.load(response)
   stage=call('/imports/actual',(root/'tests/fixtures/category-import-26.10.0.zip').read_bytes() if mode=='blocked' else archive.read_bytes(),'import')['data'];b=stage['budget_id'];i=stage['id'];base=f'/budgets/{b}'
   expected={'budget':b,'import':i,'name':'','native_category':'18000000-0000-4000-8000-000000000001'}
   if mode=='active':
    activated=call(f'/imports/{i}/activate',{},'activate')['data'];expected['name']=activated['name']
    call(base+'/categories',{'id':expected['native_category'],'name':'Added after import'},'native-category')
    transactions=call(base+'/transactions')['data'];transaction=next(t for t in transactions if not t['reconciled'] and not t['starting_balance']);expected['transaction']=transaction['id']
    call(base+'/transactions/'+transaction['id'],{'notes':'Changed after import','category_id':expected['native_category']},'edit-after-import','PATCH')
    expected['transactions']=call(base+'/transactions')['data'];expected['accounts']=call(base+'/accounts')['data']
    backup=call(base+'/backups',{'label':'Legacy imported categories'},'archive')['data']
    (root/'tests/fixtures/category-import-schema6.zip').write_bytes(urllib.request.urlopen(url+f'/api/v1{base}/backups/{backup["id"]}/download').read())
   (root/f'tests/fixtures/category-legacy-{mode}.json').write_text(json.dumps(expected,indent=2)+'\n')
  finally:process.terminate();process.wait()
  shutil.copyfile(Path(directory)/'actualbudget.sqlite3',root/f'tests/fixtures/category-legacy-{mode}.sqlite3')
