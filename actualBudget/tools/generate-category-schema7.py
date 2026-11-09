"""Generate genuine accounts-schema7 fixtures through immutable 4772f40 HTTP API.

Build a private git archive of 4772f40, then pass its server executable here.
No ZIP/SQLite mutation: snapshots are downloaded or copied after server shutdown.
"""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BINARY = str(Path(sys.argv[1]).resolve())
FIXTURES = ROOT / 'tests/fixtures'

def identity(n):
    return f'17000000-0000-4000-8000-{n:012d}'

for mode in ['native', 'import']:
    with tempfile.TemporaryDirectory(prefix='category-schema7-') as directory:
        process = subprocess.Popen([BINARY], env={**os.environ,
            'ACTUALBUDGET_DATA_DIR': directory, 'ACTUALBUDGET_HOST': '127.0.0.1',
            'ACTUALBUDGET_PORT': '0', 'ACTUALBUDGET_WEB_DIR': directory},
            stdout=subprocess.PIPE, text=True)
        try:
            url = 'http://' + process.stdout.readline().strip().removeprefix('LISTENING ')
            receipts = []
            def call(path, body=None, key='read', method=None, retain=False):
                data = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
                request = urllib.request.Request(url + '/api/v1' + path, data=data, method=method,
                    headers={'Content-Type': 'application/zip' if isinstance(body, bytes) else 'application/json', 'Idempotency-Key': key})
                with urllib.request.urlopen(request) as response:
                    result = json.load(response)
                    if retain:
                        receipts.append({'path': path, 'body': body, 'key': key, 'method': method or 'POST', 'status': response.status, 'response': result})
                    return result['data']
            if mode == 'import':
                stage = call('/imports/actual', (FIXTURES / 'import-26.10.0.zip').read_bytes(), 'schema7-import')
                budget = call(f'/imports/{stage["id"]}/activate', {}, 'schema7-activate', retain=True)
            else:
                budget = call('/budgets', {'id': identity(1), 'name': 'Schema 7 account metadata'}, 'schema7-budget')
            base = f'/budgets/{budget["id"]}'
            category = call(base + '/categories', {'id': identity(2), 'name': 'Native category after import'}, 'schema7-category', retain=True)
            if mode == 'native':
                for n, name in [(3, 'Spending'), (4, 'Reserve')]:
                    call(base + '/accounts', {'id': identity(n), 'name': name, 'opening_balance': 0, 'opening_date': '2026-01-01'}, 'schema7-account-' + str(n))
                transaction = call(base + '/transactions', {'id': identity(5), 'account_id': identity(3), 'category_id': category['id'], 'amount': -1250, 'date': '2026-01-01', 'notes': 'Original schema 7 purchase'}, 'schema7-transaction', retain=True)
            accounts = call(base + '/accounts')
            group = call(base + '/account-groups', {'id': identity(6), 'name': 'Everyday accounts'}, 'schema7-group', retain=True)
            call(base + '/account-groups', {'id': identity(7), 'name': 'Other accounts'}, 'schema7-other-group')
            call(base + '/account-groups/' + group['id'] + '/move', {'before_id': None}, 'schema7-group-order', retain=True)
            for account in accounts:
                call(base + '/accounts/' + account['id'], {'group_id': group['id'], 'notes': 'Post-import account note\nPreserve this metadata'}, 'schema7-account-note-' + account['id'], 'PATCH', True)
            call(base + '/accounts/' + accounts[0]['id'] + '/move', {'before_id': None}, 'schema7-account-order', retain=True)
            if mode == 'import':
                transaction = next(row for row in call(base + '/transactions') if not row['reconciled'] and not row['starting_balance'])
                call(base + '/transactions/' + transaction['id'], {'category_id': category['id'], 'notes': 'Post-import native category and note'}, 'schema7-post-import-edit', 'PATCH', True)
            expected = {'source_commit': '4772f40', 'mode': mode, 'budget': budget,
                'native_category': category['id'], 'accounts': call(base + '/accounts'),
                'account_groups': call(base + '/account-groups'), 'transactions': call(base + '/transactions'),
                'checkpoint': call(base + '/checkpoint'), 'receipts': receipts}
            backup = call(base + '/backups', {'label': 'Original accounts schema 7'}, 'schema7-backup')
            archive = urllib.request.urlopen(url + '/api/v1' + base + '/backups/' + backup['id'] + '/download').read()
            manifest = json.loads(zipfile.ZipFile(io.BytesIO(archive)).read('manifest.json'))
            assert manifest['schema_version'] == 7, manifest
            (FIXTURES / f'category-{mode}-schema7.zip').write_bytes(archive)
            (FIXTURES / f'category-{mode}-schema7.json').write_text(json.dumps(expected, indent=2) + '\n')
        finally:
            process.terminate()
            process.wait()
        shutil.copyfile(Path(directory) / 'actualbudget.sqlite3', FIXTURES / f'category-{mode}-schema7.sqlite3')
