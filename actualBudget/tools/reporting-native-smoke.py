# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Focused public HTTP/schema21→24/native/companion reporting qualification.
Uses a disposable copy of a genuine21 fixture. No SQL mutations or live volume.
"""
import hashlib,io,json,os,shutil,subprocess,tempfile,urllib.request,urllib.error,uuid,zipfile
from pathlib import Path
from jsonschema import Draft202012Validator,FormatChecker
project=Path(__file__).resolve().parents[1];fixtures=project/'tests/fixtures';evidence=json.loads((fixtures/'reporting-schema21.json').read_text())
for ext,key in [('.sqlite3','database_sha256'),('.zip','archive_sha256')]:assert hashlib.sha256((fixtures/('reporting-schema21'+ext)).read_bytes()).hexdigest()==evidence[key]
with tempfile.TemporaryDirectory(prefix='reporting-native-copy-') as directory:
 shutil.copyfile(fixtures/'reporting-schema21.sqlite3',Path(directory)/'actualbudget.sqlite3')
 env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0','ACTUALBUDGET_WEB_DIR':str(project/'web/dist')};env.pop('ACTUALBUDGET_PUBLIC_ORIGIN',None)
 process=subprocess.Popen([str(project/'server/target/debug/actualbudget-server')],env=env,stdout=subprocess.PIPE,text=True)
 try:
  line=process.stdout.readline().strip();assert line.startswith('LISTENING '),line;url='http://'+line.removeprefix('LISTENING ')
  def call(path,body=None,key=None,headers=None,expected=200,method=None):
   raw=body if isinstance(body,bytes) else None if body is None else json.dumps(body).encode()
   request=urllib.request.Request(url+'/api/v1'+path,data=raw,method=method,headers={'Content-Type':'application/zip' if isinstance(body,bytes) else 'application/json','Idempotency-Key':key or str(uuid.uuid4()),**(headers or {})})
   try:response=urllib.request.urlopen(request,timeout=15)
   except urllib.error.HTTPError as error:response=error
   with response:
    value=json.load(response) if response.headers.get('Content-Type','').startswith('application/json') else response.read();assert response.status==expected,(path,response.status,value);return value
  contract=call('/openapi.json');budget=evidence['created']['data']['id'];root='/budgets/'+budget;headers=evidence['headers']
  def check(path,result,method='GET',status=200):
   schema=contract['paths']['/api/v1/budgets/{budget}'+path][method.lower()]['responses'][str(status)]['content']['application/json']['schema'];Draft202012Validator({**schema,'components':contract['components']},format_checker=FormatChecker()).validate(result)
  initial=call(root+'/reporting');check('/reporting',initial);assert initial['data']['documents']==[];assert initial['data']['epoch']==evidence['metadata']['epoch']
  assert call(root+'/transactions',evidence['body'],evidence['key'],headers,201)==evidence['receipt']
  assert call(root+'/transactions')['balances']==evidence['register']['balances']
  before_companion=call(root+'/companion?account_id='+evidence['account']);assert before_companion==evidence['companion']
  page,widget=str(uuid.uuid4()),str(uuid.uuid4());body={'writes':[{'id':page,'kind':'page','revision':0,'config':{'name':'Upgrade history'},'tombstone':False},{'id':widget,'kind':'widget','revision':0,'config':{'dashboard_page_id':page,'type':'summary-card','x':0,'y':0,'width':6,'height':3,'meta':{'content':'{"type":"sum"}','timeFrame':{'start':'2026-11','end':'2026-11','mode':'static'}}},'tombstone':False}]}
  command=call(root+'/reporting/commands',body,'report21',headers);check('/reporting/commands',command,'POST');assert call(root+'/reporting/commands',body,'report21',headers)==command
  query={'version':1,'kind':'summary','as_of':'2026-11-09','options':{'start':'2026-11','end':'2026-11','summary':{'type':'sum'}}};projection=call(root+'/reports/query',query);check('/reports/query',projection,'POST');assert projection['data']['result']['total']==12000
  inspect={'query':query,'snapshot':projection['data']['snapshot'],'cell_id':projection['data']['cells'][0]['id'],'offset':0,'limit':1};inspected=call(root+'/reports/drilldown',inspect);check('/reports/drilldown',inspected,'POST');assert inspected['data']['total_count']==2;assert inspected['data']['total_amount']==12000
  custom={'version':1,'kind':'custom','as_of':'2026-11-09','options':{'startDate':'2026-11','endDate':'2026-11','groupBy':'Account','balanceTypeOp':'totalTotals','interval':'Monthly'}}
  custom_projection=call(root+'/reports/query',custom);check('/reports/query',custom_projection,'POST');assert custom_projection['data']['result']['totalTotals']==12000
  custom_inspect={'query':custom,'snapshot':custom_projection['data']['snapshot'],'cell_id':custom_projection['data']['cells'][0]['id'],'offset':0,'limit':2};custom_rows=call(root+'/reports/drilldown',custom_inspect);check('/reports/drilldown',custom_rows,'POST');assert custom_rows['data']['record_kind']=='transactions'
  planned={**custom,'options':{**custom['options'],'groupBy':'CategoryGroup','nested':True,'balanceTypeOp':'totalBudgeted'}};planned_projection=call(root+'/reports/query',planned);check('/reports/query',planned_projection,'POST');assert planned_projection['data']['recorded']==False;assert planned_projection['data']['basis']=='budget_allocations'
  invalid=call(root+'/reports/drilldown',{**inspect,'limit':201},expected=400);check('/reports/drilldown',invalid,'POST',400)
  cancel_body={'writes':[{'id':page,'kind':'page','revision':1,'config':{'name':'Canceled'},'tombstone':False}]};cancel_key='cancel21';canceled=call(root+'/reporting/commands',cancel_body,cancel_key,{**headers,'X-Actual-Cancel-If-Uncommitted':'true'},409);check('/reporting/commands',canceled,'POST',409);assert call(root+'/reporting/commands',cancel_body,cancel_key,headers,409)==canceled
  after_companion=call(root+'/companion?account_id='+evidence['account'])['data'];assert {k:v for k,v in after_companion.items() if k!='snapshot'}=={k:v for k,v in before_companion['data'].items() if k!='snapshot'}
  backup=call(root+'/backups',{'label':'Reporting22'},headers=headers,expected=201);archive=call(root+'/backups/'+backup['data']['id']+'/download')
  with zipfile.ZipFile(io.BytesIO(archive)) as contents:assert json.loads(contents.read('manifest.json'))['schema_version']==27
  duplicate=str(uuid.uuid4());call(root+'/duplicate',{'id':duplicate,'name':'History duplicate'},headers=headers,expected=201);assert call('/budgets/'+duplicate+'/reporting')['data']['documents']==command['data']['documents']
  # Restore the genuine21 ZIP through supported native upload/activation; reporting
  # is empty by construction and money/receipt bytes remain preserved in that ZIP.
  staged=call('/restores',(fixtures/'reporting-schema21.zip').read_bytes(),expected=201);stage=staged['data']['id'];call(root+'/restores/'+stage+'/activate',{'confirm_name':'Before reporting'},headers=headers)
  restored=call(root+'/reporting');assert restored['data']['documents']==[];assert restored['data']['epoch']!=headers['X-Actual-Budget-Epoch'];assert call(root+'/transactions')['balances']==evidence['register']['balances']
  stale=call(root+'/reporting/commands',body,'stale-epoch',headers,409);check('/reporting/commands',stale,'POST',409)
  points=call(root+'/backups')['data'];assert any(p['kind']=='upgrade' for p in points)
  # Public native exports expose the documented portable audit records; assert
  # source ordering is retained field-for-field, independent of replacement clocks.
  imported=call('/imports/actual',(fixtures/'reports-clock-source-26.10.zip').read_bytes(),expected=201);call('/imports/'+imported['data']['id']+'/activate',{},expected=201)
  source_root='/budgets/'+imported['data']['budget_id'];source_state=call(source_root+'/reporting')['data'];source_headers={'X-Actual-Budget-Epoch':source_state['epoch']}
  source_backup=call(source_root+'/backups',{'label':'Source reports22'},headers=source_headers,expected=201);source_archive=call(source_root+'/backups/'+source_backup['data']['id']+'/download')
  with zipfile.ZipFile(io.BytesIO(source_archive)) as contents:portable=json.loads(contents.read('budget.json'))
  def records(name):
   table=portable['tables'][name];return [dict(zip(table['columns'],row)) for row in table['rows']]
  clocks=[r for r in records('imported_field_clocks') if r['dataset'] in ['dashboard','dashboard_pages','custom_reports']];assert clocks
  versions={(r['entity_id'],r['field']):r['clock'] for r in records('field_versions') if r['entity']=='reporting_documents'}
  source_evidence=json.loads((fixtures/'reports-clock-source-26.10.json').read_text());latest={}
  for original in source_evidence['clocks']:
   key=(original['row'],original['column']);latest[key]=max(latest.get(key,''),original['timestamp'])
  assert versions==latest

  for original in clocks:assert versions[(original['row_id'],original['column_name'])]==original['edit_order']
  call(source_root+'/backups/'+source_backup['data']['id']+'/restore',{'confirm_name':portable['budget']['name']},headers=source_headers)
  assert call(source_root+'/reporting')['data']['documents']==source_state['documents']
  allocation_import=call('/imports/actual',(fixtures/'reports-source-26.10.zip').read_bytes(),expected=201);call('/imports/'+allocation_import['data']['id']+'/activate',{},expected=201)
  allocation_root='/budgets/'+allocation_import['data']['budget_id'];allocation_oracle=json.loads((fixtures/'custom-reports-source-26.10.json').read_text());allocation_options=next(c['options'] for c in allocation_oracle['cases'] if c['options']['balanceTypeOp']=='totalBudgeted' and c['options']['groupBy']=='Category')
  allocation_query={'version':1,'kind':'custom','as_of':'2026-11-09','options':allocation_options};allocation_projection=call(allocation_root+'/reports/query',allocation_query);check('/reports/query',allocation_projection,'POST');allocation_cell=next(c for c in allocation_projection['data']['cells'] if c['label']=='food total')
  allocation_rows=call(allocation_root+'/reports/drilldown',{'query':allocation_query,'snapshot':allocation_projection['data']['snapshot'],'cell_id':allocation_cell['id'],'offset':0,'limit':2});check('/reports/drilldown',allocation_rows,'POST');assert allocation_rows['data']['total_amount']==5000;assert all(r['account_id'] is None for r in allocation_rows['data']['rows'])
  print('PASS reporting genuine21 upgrade, schemas, receipts/cancel, companion money, duplication, native source-clock/layout round trip and genuine21 restore/epoch fence')
 finally:process.terminate();process.wait(timeout=15)
