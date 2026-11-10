# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Public native/schema/epoch forecast consumer. Disposable server; no SQL setup."""
import hashlib,io,json,os,subprocess,tempfile,urllib.request,urllib.error,uuid,zipfile
from pathlib import Path
from jsonschema import Draft202012Validator,FormatChecker
project=Path(__file__).resolve().parents[1];fixtures=project/'tests/fixtures';source=json.loads((fixtures/'forecast-source-26.10.json').read_text());archive=(fixtures/'forecast-source-26.10.zip').read_bytes();assert hashlib.sha256(archive).hexdigest()==source['archive_sha256']
with tempfile.TemporaryDirectory(prefix='forecast-native-') as directory:
 env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':str(project/'web/dist')};env.pop('ACTUALBUDGET_PUBLIC_ORIGIN',None)
 process=subprocess.Popen([str(project/'server/target/debug/actualbudget-server')],env=env,stdout=subprocess.PIPE,text=True)
 try:
  line=process.stdout.readline().strip();assert line.startswith('LISTENING '),line;url='http://'+line.removeprefix('LISTENING ')
  def call(path,body=None,headers=None,expected=200,method=None):
   raw=body if isinstance(body,bytes) else None if body is None else json.dumps(body).encode();request=urllib.request.Request(url+'/api/v1'+path,data=raw,method=method,headers={'Content-Type':'application/zip' if isinstance(body,bytes) else 'application/json','Idempotency-Key':str(uuid.uuid4()),**(headers or {})})
   try:response=urllib.request.urlopen(request,timeout=15)
   except urllib.error.HTTPError as error:response=error
   with response:
    value=json.load(response) if response.headers.get('Content-Type','').startswith('application/json') else response.read();assert response.status==expected,(path,response.status,value);return value
  contract=call('/openapi.json');route=contract['paths']['/api/v1/budgets/{budget}/reports/forecast']['post']
  def check(value,status=200,request=False):
   schema=route['requestBody']['content']['application/json']['schema'] if request else route['responses'][str(status)]['content']['application/json']['schema'];Draft202012Validator({**schema,'components':contract['components']},format_checker=FormatChecker()).validate(value)
  stage=call('/imports/actual',archive,expected=201);call('/imports/'+stage['data']['id']+'/activate',{},expected=201);root='/budgets/'+stage['data']['budget_id'];state=call(root+'/reporting')['data'];headers={'X-Actual-Budget-Epoch':state['epoch']};widget=next(d for d in state['documents'] if d['id']==source['widgets'][0]['id']);assert widget['config']['meta']==source['widgets'][0]['meta'];assert call(root+'/preferences')['data']['experimental']['balanceForecastReport']==True
  query={'version':1,'as_of':source['clock'][:10],'options':{**source['cases'][0]['options'],'granularity':'Daily'}};check(query,request=True);before=call(root+'/reports/forecast',query);check(before);assert before['data']['result']==source['cases'][0]['expected'];assert call(root+'/reports/forecast',query)['data']['cache']['hit']==True
  for body in [{**query,'version':2},{**query,'options':{**query['options'],'source':'tracking-budget'}}]:check(call(root+'/reports/forecast',body,expected=400),400)
  backup=call(root+'/backups',{'label':'Before forecast native round trip'},headers,201);data=call(root+'/backups/'+backup['data']['id']+'/download')
  def portable(bytes):
   with zipfile.ZipFile(io.BytesIO(bytes)) as contents:
    assert json.loads(contents.read('manifest.json'))['schema_version']==24;return json.loads(contents.read('budget.json'))
  original=portable(data)
  def report_clocks(value):
   table=value['tables']['field_versions'];return [dict(zip(table['columns'],row)) for row in table['rows'] if dict(zip(table['columns'],row))['entity']=='reporting_documents']
  duplicate=str(uuid.uuid4());call(root+'/duplicate',{'id':duplicate,'name':'Forecast native duplicate'},headers,201);assert call('/budgets/'+duplicate+'/reporting')['data']['documents']==state['documents'];check(call('/budgets/'+duplicate+'/reports/forecast',query))
  call(root+'/backups/'+backup['data']['id']+'/restore',{'confirm_name':original['budget']['name']},headers);restored=call(root+'/reporting')['data'];assert restored['epoch']!=state['epoch'];assert restored['documents']==state['documents'];after=call(root+'/reports/forecast',query);check(after);assert after['data']['cache']['hit']==False;assert after['data']['epoch']==restored['epoch'];assert after['data']['result']==before['data']['result']
  current_headers={'X-Actual-Budget-Epoch':restored['epoch']};second=call(root+'/backups',{'label':'After forecast restore'},current_headers,201);current=portable(call(root+'/backups/'+second['data']['id']+'/download'));assert report_clocks(current)==report_clocks(original)
  for amount in [-9007199254740991,9007199254740991]:
   boundary='/budgets/'+str(uuid.uuid4());call('/budgets',{'id':boundary.split('/')[-1],'name':'Forecast monetary boundary'},expected=201)
   boundary_headers={'X-Actual-Budget-Epoch':call(boundary)['data']['epoch']}
   call(boundary+'/accounts',{'id':str(uuid.uuid4()),'name':'Boundary','opening_balance':amount,'opening_date':'2026-11-01'},boundary_headers,201)
   call(boundary+'/preferences',{'experimental':{'balanceForecastReport':True}},boundary_headers,method='PATCH')
   result=call(boundary+'/reports/forecast',{'version':1,'as_of':'2026-11-02','options':{'startDate':'2026-11-02','endDate':'2026-11-03','source':'schedules','granularity':'Daily'}})
   assert result['data']['result']['lowestBalance']['balance']==amount
   assert all(point['balance']==amount for point in result['data']['result']['dataPoints'])
   check(result)
  print('PASS forecast served schemas including both safe-money boundaries, source opt-in/config, native24 duplicate/restore, retained reporting clocks and epoch-bound cache invalidation')
 finally:process.terminate();process.wait(timeout=15)
