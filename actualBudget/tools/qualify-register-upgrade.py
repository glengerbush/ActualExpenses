#!/usr/bin/env python3
"""Exercise a genuine schema-1 binary, upgrade, and matching backup restore via HTTP."""
import http.client
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

root=Path(__file__).resolve().parents[2]
baseline='4600a70'
budget='20000000-0000-4000-8000-000000000001'
account='20000000-0000-4000-8000-000000000002'
def start(binary,data):
    child=subprocess.Popen([str(binary)],env={**os.environ,'ACTUALBUDGET_DATA_DIR':str(data),'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0'},stdout=subprocess.PIPE,text=True)
    line=child.stdout.readline().strip()
    if not line.startswith('LISTENING '):raise RuntimeError('Server failed to start')
    return child,http.client.HTTPConnection(line.removeprefix('LISTENING '),timeout=10)
def call(connection,method,path,body=None,key=None):
    connection.request(method,'/api/v1'+path,json.dumps(body) if body is not None else None,{'Content-Type':'application/json','Idempotency-Key':key or 'qualification'})
    response=connection.getresponse();result=json.loads(response.read())
    assert response.status in (200,201),(response.status,result)
    return result
with tempfile.TemporaryDirectory(prefix='actualbudget-upgrade-') as directory:
    directory=Path(directory)
    archive=subprocess.check_output(['git','archive',baseline,'actualBudget/server'],cwd=root)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(directory/'baseline',filter='data')
    manifest=directory/'baseline/actualBudget/server/Cargo.toml'
    subprocess.run(['cargo','build','--locked','--manifest-path',str(manifest),'--target-dir',str(directory/'target')],check=True,stdout=subprocess.DEVNULL)
    old=directory/'target/debug/actualbudget-server';new=root/'actualBudget/server/target/debug/actualbudget-server'
    data=directory/'data';data.mkdir()
    process,connection=start(old,data)
    try:
        call(connection,'POST','/budgets',{'id':budget,'name':'Upgrade fixture','budget_type':'tracking'},'budget')
        body={'id':account,'name':'Preserved account','opening_balance':-12345,'opening_date':'2024-02-29','notes':'Schema one data'}
        expected=call(connection,'POST',f'/budgets/{budget}/accounts',body,'account')['data']
    finally:connection.close();process.terminate();process.wait(timeout=10)
    fixture=root/'actualBudget/tests/fixtures/server-v1.sqlite3'
    if not fixture.exists():shutil.copyfile(data/'actualbudget.sqlite3',fixture)
    process,connection=start(new,data)
    try:
        actual=call(connection,'GET',f'/budgets/{budget}/accounts')['data'][0];assert actual==expected
        replay=call(connection,'POST',f'/budgets/{budget}/accounts',body,'account')['data'];assert replay==expected
        entry={'id':'20000000-0000-4000-8000-000000000003','account_id':account,'amount':345,'date':'2026-01-01','notes':'After upgrade'}
        call(connection,'POST',f'/budgets/{budget}/transactions',entry,'new-entry')
        assert call(connection,'GET',f'/budgets/{budget}/transactions?account_id={account}')['balances']['total']==-12000
    finally:connection.close();process.terminate();process.wait(timeout=10)
    backups=list((data/'backups').glob('before-v1-*.sqlite3'));assert len(backups)==1
    restored=directory/'restored';restored.mkdir();shutil.copyfile(backups[0],restored/'actualbudget.sqlite3')
    process,connection=start(old,restored)
    try:assert call(connection,'GET',f'/budgets/{budget}/accounts')['data']==[expected]
    finally:connection.close();process.terminate();process.wait(timeout=10)
    print(json.dumps({'baseline':baseline,'upgrade':'schema1→schema3','preexisting_balance':-12345,'upgraded_after_write':-12000,'restored_old_binary_balance':-12345,'original_idempotency_replay':'preserved','backup':'readable by original binary','fixture':str(fixture)},indent=2))
