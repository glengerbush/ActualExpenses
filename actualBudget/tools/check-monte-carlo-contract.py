# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Check the served Monte Carlo contract with real isolated HTTP responses."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import urllib.error
import urllib.request
import uuid
from jsonschema import Draft202012Validator, FormatChecker

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='monte-contract-') as directory:
    env = {**os.environ, 'ACTUALBUDGET_DATA_DIR': directory, 'ACTUALBUDGET_HOST': '127.0.0.1', 'ACTUALBUDGET_PORT': '0', 'ACTUALBUDGET_WEB_DIR': str(root / 'web/dist')}
    env.pop('ACTUALBUDGET_PUBLIC_ORIGIN', None)
    server = subprocess.Popen([str(root / 'server/target/debug/actualbudget-server')], env=env, stdout=subprocess.PIPE, text=True)
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(server.stdout, selectors.EVENT_READ)
            assert selector.select(15), 'Server start timed out'
            line = server.stdout.readline().strip()
        assert line.startswith('LISTENING ')
        base = 'http://' + line.removeprefix('LISTENING ')
        def call(path, body=None, method=None, expected=200):
            req = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), method=method, headers={'Content-Type':'application/json','Idempotency-Key':str(uuid.uuid4())})
            try: response = urllib.request.urlopen(req, timeout=30)
            except urllib.error.HTTPError as error: response = error
            with response:
                value = json.load(response)
                assert response.status == expected, (path, response.status, value)
                return value
        contract = call('/api/v1/openapi.json')
        fragment = json.loads((root / 'server/src/monte_carlo/openapi.json').read_text())
        for path, operations in fragment['paths'].items(): assert contract['paths'][path] == operations
        for name, schema in fragment['components']['schemas'].items(): assert contract['components']['schemas'][name] == schema
        def validate(schema, value):
            Draft202012Validator({**schema, 'components': contract['components']}, format_checker=FormatChecker()).validate(value)
        def component(name, value): validate({'$ref': '#/components/schemas/MonteCarlo' + name}, value)
        budget = str(uuid.uuid4())
        call('/api/v1/budgets', {'id':budget,'name':'Contract plan'}, expected=201)
        route = f'/api/v1/budgets/{budget}/reports/monte-carlo'
        source = json.loads((root / 'tests/fixtures/monte-carlo-baseline-source.json').read_text())
        # Independent source shapes include conditional cash-flow fields absent from the simple live plan.
        for case in source['cases']: component('Result', case['result'])
        test = next(case for case in source['cases'] if case['name'] == 'baseline-proportional')
        body = dict(version=1, request_id=str(uuid.uuid4()), config=test['meta'], **test['options'])
        component('Request', body)
        disabled = call(route, body, expected=409)
        validate({'$ref':'#/components/schemas/Error'}, disabled)
        call(f'/api/v1/budgets/{budget}/preferences', {'experimental':{'monteCarloReport':True}}, method='PATCH')
        result = call(route, body)
        component('Response', result)
        component('Response', call(route, {**body, 'request_id':str(uuid.uuid4())}))
        invalid = call(route, {**body, 'config':{'pots':[{'startingBalance':'wrong'}]}}, expected=400)
        validate({'$ref':'#/components/schemas/Error'}, invalid)
        missing = call(route + '/' + str(uuid.uuid4()), method='DELETE', expected=404)
        validate({'$ref':'#/components/schemas/Error'}, missing)
        print('PASS served Monte Carlo request/result schemas, source cash-flow variants, cache and structured error responses')
    finally:
        server.terminate()
        server.wait(timeout=20)
