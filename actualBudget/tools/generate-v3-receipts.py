#!/usr/bin/env python3
"""Capture genuine pre-concurrency HTTP receipts; no SQL writes or mocked server."""
import hashlib
import http.client
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BASELINE = '0b66dc61b61314abfa4bb871cae6e62a7451b10b'
FIXTURE = ROOT / 'actualBudget/tests/fixtures/server-v3.sqlite3'
MANIFEST = FIXTURE.with_suffix('.json')
BUDGET = '05300000-0000-4000-8000-000000000001'
ACCOUNT = '05300000-0000-4000-8000-000000000002'
ENTRY = '05300000-0000-4000-8000-000000000003'
DUPLICATE = '05300000-0000-4000-8000-000000000004'


def call(connection, method, path, body=None, key='fixture-read'):
    connection.request(method, '/api/v1' + path,
                       json.dumps(body) if body is not None else None,
                       {'Content-Type': 'application/json', 'Idempotency-Key': key})
    response = connection.getresponse()
    result = json.loads(response.read())
    assert response.status in (200, 201), (response.status, result)
    return response.status, result


with tempfile.TemporaryDirectory(prefix='actualbudget-v3-receipts-') as temporary:
    directory = Path(temporary)
    archive = subprocess.check_output(['git', 'archive', BASELINE, 'actualBudget/server'], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        source.extractall(directory / 'baseline', filter='data')
    manifest = directory / 'baseline/actualBudget/server/Cargo.toml'
    subprocess.run(['cargo', 'build', '--locked', '--manifest-path', str(manifest),
                    '--target-dir', str(directory / 'target')], check=True)
    data = directory / 'data'
    data.mkdir()
    child = subprocess.Popen([str(directory / 'target/debug/actualbudget-server')],
                             env={**os.environ, 'ACTUALBUDGET_DATA_DIR': str(data),
                                  'ACTUALBUDGET_HOST': '127.0.0.1', 'ACTUALBUDGET_PORT': '0'},
                             stdout=subprocess.PIPE, text=True)
    line = child.stdout.readline().strip()
    assert line.startswith('LISTENING '), line
    connection = http.client.HTTPConnection(line.removeprefix('LISTENING '), timeout=10)
    operations = [
        ('POST', '/budgets', 'v3-budget', {'id': BUDGET, 'name': 'Pre-concurrency receipts', 'budget_type': 'tracking'}),
        ('POST', f'/budgets/{BUDGET}/accounts', 'v3-account', {'id': ACCOUNT, 'name': 'Spending', 'opening_balance': 0, 'opening_date': '2024-02-29'}),
        ('POST', f'/budgets/{BUDGET}/transactions', 'v3-create', {'id': ENTRY, 'account_id': ACCOUNT, 'date': '2024-02-29', 'amount': -1234, 'notes': 'Original', 'cleared': True}),
        ('PATCH', f'/budgets/{BUDGET}/transactions/{ENTRY}', 'v3-edit', {'amount': -234, 'notes': 'Edited before concurrency upgrade'}),
        ('POST', f'/budgets/{BUDGET}/transactions/{ENTRY}/duplicate', 'v3-duplicate', {'id': DUPLICATE}),
        ('DELETE', f'/budgets/{BUDGET}/transactions/{DUPLICATE}', 'v3-delete', None),
    ]
    receipts = []
    try:
        for method, path, key, body in operations:
            status, response = call(connection, method, path, body, key)
            receipts.append({'method': method, 'path': path, 'key': key, 'body': body, 'status': status, 'response': response})
        _, register = call(connection, 'GET', f'/budgets/{BUDGET}/transactions?account_id={ACCOUNT}')
        assert len(register['data']) == 1 and register['balances']['total'] == -234
        # All responses must already replay exactly in the original binary.
        for receipt in receipts:
            status, response = call(connection, receipt['method'], receipt['path'], receipt['body'], receipt['key'])
            assert (status, response) == (receipt['status'], receipt['response'])
    finally:
        connection.close()
        child.terminate()
        child.wait(timeout=10)
    shutil.copyfile(data / 'actualbudget.sqlite3', FIXTURE)
    evidence = {'baseline_commit': BASELINE, 'schema_version': 3,
                'provenance': 'Genuine committed server binary; public HTTP writes; copied after graceful shutdown',
                'sha256': hashlib.sha256(FIXTURE.read_bytes()).hexdigest(),
                'budget_id': BUDGET, 'account_id': ACCOUNT, 'receipts': receipts, 'register': register}
    MANIFEST.write_text(json.dumps(evidence, indent=2) + '\n')
    print(f'PASS six genuine schema3 receipts and exact original replays; {FIXTURE}')
