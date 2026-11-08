#!/usr/bin/env python3
"""Disposable public-HTTP scaling probe. Never opens an existing budget or SQLite file."""
import argparse
import http.client
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import uuid

parser=argparse.ArgumentParser()
parser.add_argument('--history',type=int,default=5000)
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='actualbudget-scaling-') as directory:
    process=subprocess.Popen([str(root/'server/target/debug/actualbudget-server')],env={**os.environ,'ACTUALBUDGET_DATA_DIR':directory,'ACTUALBUDGET_HOST':'127.0.0.1','ACTUALBUDGET_PORT':'0'},stdout=subprocess.PIPE,text=True)
    try:
        address=process.stdout.readline().strip().removeprefix('LISTENING ')
        connection=http.client.HTTPConnection(address,timeout=10)
        budget=str(uuid.uuid4());account=str(uuid.uuid4());other=str(uuid.uuid4())
        def call(method,path,body=None):
            headers={'Content-Type':'application/json','Idempotency-Key':str(uuid.uuid4())}
            started=time.perf_counter()
            connection.request(method,'/api/v1'+path,json.dumps(body) if body is not None else None,headers)
            response=connection.getresponse();data=json.loads(response.read());elapsed=(time.perf_counter()-started)*1000
            if response.status not in (200,201):raise RuntimeError(data)
            return elapsed,data
        call('POST','/budgets',{'id':budget,'name':'Disposable scale probe'})
        for identifier in (account,other):call('POST',f'/budgets/{budget}/accounts',{'id':identifier,'name':identifier,'opening_balance':0,'opening_date':'2026-01-01'})
        path=f'/budgets/{budget}/transactions'
        def append(account):
            identifier=str(uuid.uuid4())
            elapsed,_=call('POST',path,{'id':identifier,'account_id':account,'amount':1,'date':'2026-01-02','cleared':True})
            return elapsed,identifier
        def summary(samples):
            ordered=sorted(samples)
            return {'median_ms':round(statistics.median(samples),3),'p95_ms':round(ordered[int(len(samples)*.95)-1],3)}
        def measure():
            samples=[]
            for _ in range(100):elapsed,identifier=append(account);samples.append(elapsed)
            reads=[call('GET',path+f'?account_id={account}&limit=100')[0] for _ in range(30)]
            edits=[call('PATCH',path+'/'+identifier,{'notes':f'Note {index}'})[0] for index in range(30)]
            return {'append':summary(samples),'newest_page':summary(reads),'notes_edit':summary(edits)}
        results={'history_per_phase':args.history,'empty':measure()}
        for _ in range(args.history):append(other)
        results['unrelated_history']=measure()
        for _ in range(args.history):append(account)
        results['same_account_history']=measure()
        _,page=call('GET',path+f'?account_id={account}&limit=1')
        assert page['balances']['total']==args.history+300
        print(json.dumps(results,indent=2))
        connection.close()
    finally:
        process.terminate();process.wait(timeout=10)
