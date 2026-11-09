#!/usr/bin/env python3
"""Disposable public HTTP probe, including process CPU/RSS observations. Never opens SQLite."""
import http.client,json,os,pathlib,statistics,subprocess,tempfile,time,uuid,urllib.parse,argparse
p=argparse.ArgumentParser();p.add_argument('--history',type=int,default=10000);args=p.parse_args();root=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='register-query-scale-') as directory:
 child=subprocess.Popen([str(root/'server/target/debug/actualbudget-server')],env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0'},stdout=subprocess.PIPE,text=True)
 try:
  address=child.stdout.readline().strip().removeprefix('LISTENING ');client=http.client.HTTPConnection(address,timeout=30)
  def call(method,path,body=None):
   client.request(method,'/api/v1'+path,json.dumps(body) if body is not None else None,{'Content-Type':'application/json','Idempotency-Key':str(uuid.uuid4())});r=client.getresponse();raw=r.read();data=json.loads(raw);assert r.status in (200,201),(r.status,data);return data,len(raw)
  b=str(uuid.uuid4());a=str(uuid.uuid4());call('POST','/budgets',{'id':b,'name':'Disposable register scale'});call('POST',f'/budgets/{b}/accounts',{'id':a,'name':'Spending','opening_balance':0,'opening_date':'2026-01-01'})
  path=f'/budgets/{b}/transactions'
  for n in range(args.history):call('POST',path,{'id':str(uuid.uuid4()),'account_id':a,'date':'2026-01-02','amount':-1,'notes':'Coffee' if n%2==0 else 'Other','cleared':n%2==0})
  def cpu():
   values=pathlib.Path(f'/proc/{child.pid}/stat').read_text().split();return (int(values[13])+int(values[14]))/os.sysconf('SC_CLK_TCK')
  results={'rows':args.history,'build':'debug','samples':{}}
  for label,query in [('unfiltered',None),('search',{'search':'Coffee'}),('regex',{'filter':{'conditions':[{'field':'notes','op':'matches','value':'^coffee$'}]}})]:
   samples=[];cpus=[];size=0
   for _ in range(8):
    before=cpu();started=time.perf_counter();result,size=call('GET',path+'?'+urllib.parse.urlencode({'account_id':a,'limit':100,**({'query':json.dumps(query)} if query else {})}));samples.append((time.perf_counter()-started)*1000);cpus.append((cpu()-before)*1000)
    assert len(result['data'])==100;assert result['filtered_balances']['total']==-(args.history if query is None else (args.history+1)//2)
   results['samples'][label]={'median_ms':round(statistics.median(samples),2),'max_ms':round(max(samples),2),'mean_process_cpu_ms':round(statistics.mean(cpus),2),'response_bytes':size}
  status=pathlib.Path(f'/proc/{child.pid}/status').read_text().splitlines();results['process_memory']={line.split(':')[0]:line.split(':')[1].strip() for line in status if line.startswith(('VmRSS:','VmHWM:'))};print(json.dumps(results,indent=2));client.close()
 finally:child.terminate();child.wait(timeout=10)
