"""Generate legacy native archive using only a separately built 59345d1 public server."""
import json, os, subprocess, tempfile, urllib.request, uuid, sys
from pathlib import Path
binary=sys.argv[1]
with tempfile.TemporaryDirectory(prefix='category-schema6-') as directory:
    process=subprocess.Popen([binary],env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':directory},stdout=subprocess.PIPE,text=True)
    try:
        url='http://'+process.stdout.readline().strip().removeprefix('LISTENING ')
        def post(path,body,key):
            req=urllib.request.Request(url+'/api/v1'+path,data=json.dumps(body).encode(),headers={'Content-Type':'application/json','Idempotency-Key':key})
            return json.load(urllib.request.urlopen(req))['data']
        budget='16000000-0000-4000-8000-000000000001';category='16000000-0000-4000-8000-000000000002'
        post('/budgets',{'id':budget,'name':'Legacy categories'},'legacy-budget')
        post(f'/budgets/{budget}/categories',{'id':category,'name':'Legacy food'},'legacy-category')
        point=post(f'/budgets/{budget}/backups',{'label':'Original schema 6'},'legacy-backup')
        archive=urllib.request.urlopen(url+f'/api/v1/budgets/{budget}/backups/{point["id"]}/download').read()
        Path(__file__).resolve().parents[1].joinpath('tests/fixtures/category-schema6.zip').write_bytes(archive)
    finally:process.terminate();process.wait()
