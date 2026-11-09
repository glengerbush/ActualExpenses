#!/usr/bin/env python3
"""Generate genuine schema-7 migration evidence using ae3911b's public HTTP API.
Pass --binary pointing to an isolated ae3911b build. No SQL mutations.
"""
import argparse,json,pathlib,sqlite3,subprocess,tempfile,urllib.request
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);args=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[1];fixtures=root/'tests/fixtures'
with tempfile.TemporaryDirectory(prefix='payees-schema7-') as tmp:
 import os
 env={**os.environ,'ACTUALBUDGET_DATA_DIR':tmp,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0'}
 process=subprocess.Popen([str(pathlib.Path(args.binary).resolve())],env=env,stdout=subprocess.PIPE,text=True)
 try:
  url='http://'+process.stdout.readline().strip().removeprefix('LISTENING ')
  def call(path,body=None,key=None,method=None,clock=None):
   headers={};data=None
   if isinstance(body,bytes):data=body;headers['Content-Type']='application/zip'
   elif body is not None:data=json.dumps(body,separators=(',',':')).encode();headers['Content-Type']='application/json'
   if key:headers['Idempotency-Key']=key
   if clock:headers['X-Actual-Edit-Clock']=clock
   with urllib.request.urlopen(urllib.request.Request(url+'/api/v1'+path,data,headers,method=method)) as r:
    raw=r.read();return json.loads(raw) if r.headers.get('Content-Type','').startswith('application/json') else raw
  stage=call('/imports/actual',(fixtures/'import-26.10.0.zip').read_bytes(),'upload7');iid=stage['data']['id'];b=stage['data']['budget_id'];base='/budgets/'+b
  activate=call('/imports/'+iid+'/activate',{},'activate7');payee='11000000-0000-4000-8000-000000000099'
  created=call(base+'/payees',{'id':payee,'name':'Native payee after import'},'native-payee7')
  tid='04000000-0000-4000-8000-000000000002';clock='2026-11-08T20:00:00.000Z-0000-0000000000000011'
  patch={'notes':'Native note #Upgrade after import','payee_id':payee,'amount':-1400}
  receipt=call(base+'/transactions/'+tid,patch,'native-edit7','PATCH',clock)
  backup=call(base+'/backups',{'label':'Schema seven payee fixture'},'backup7');archive=call(base+'/backups/'+backup['data']['id']+'/download')
  (fixtures/'payees-schema7-native.zip').write_bytes(archive)
  manifest={'source_commit':'ae3911b','budget_id':b,'import_id':iid,'payee_id':payee,'transaction_id':tid,'clock':clock,'patch':patch,'receipt':receipt,'payee_receipt':created,'activation_receipt':activate,'accounts':call(base+'/accounts'),'transaction':call(base+'/transactions/'+tid)}
  (fixtures/'payees-schema7.json').write_text(json.dumps(manifest,indent=2)+'\n')
 finally:process.terminate();process.wait()
 with sqlite3.connect(pathlib.Path(tmp)/'actualbudget.sqlite3') as source, sqlite3.connect(fixtures/'payees-schema7.sqlite3') as target: source.backup(target)
 print('Generated genuine schema7 SQLite and native archive through public HTTP')
