#!/usr/bin/env python3
"""Disposable native upgrade API + deterministic lost-ACK gate. No existing data/URL accepted."""
import argparse
import hashlib
import http.client
import json
import os
import pathlib
import signal
import socket
import subprocess
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=pathlib.Path, required=True)
parser.add_argument('--output', type=pathlib.Path, required=True)
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
if (output / 'data').exists():
    raise SystemExit('Use a fresh disposable output directory')
data = output / 'data'
data.mkdir()
fixtures = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/android-upgrade'
child = subprocess.Popen([str(args.binary.resolve())], env={**os.environ, 'ACTUALBUDGET_HOST': '127.0.0.1', 'ACTUALBUDGET_PORT': '0', 'ACTUALBUDGET_DATA_DIR': str(data), 'ACTUALBUDGET_WEB_DIR': str(data)}, stdout=subprocess.PIPE, text=True)
line = child.stdout.readline().strip()
assert line.startswith('LISTENING '), line
backend = 'http://' + line.split()[1]
backend_port = urlsplit(backend).port

def request(path, body=None, archive=False, key=None):
    headers = {'Content-Type': 'application/zip' if archive else 'application/json', 'Idempotency-Key': key or path.replace('/', '-')}
    raw = body if archive else None if body is None else json.dumps(body).encode()
    return json.load(urllib.request.urlopen(urllib.request.Request(backend + '/api/v1' + path, data=raw, headers=headers)))
try:
    stage = request('/imports/actual', (fixtures / 'pending.zip').read_bytes(), True, 'device-import')['data']
    budget = stage['budget_id']
    request('/imports/' + stage['id'] + '/activate', {}, key='device-activate')
    legacy = json.loads((fixtures / 'pending.state.json').read_text())
    account = legacy['accountId']
    original_ops = {op['id']: op for op in legacy['pending']}
    expected = json.loads((fixtures / 'pending.delivered.oracle.json').read_text())
    state = {'dropped': False, 'blocked': False, 'events': []}
    lock = threading.Lock()

    def persist():
        (output / 'events.json').write_text(json.dumps(state, indent=2) + '\n')

    def verify():
        register = request(f'/budgets/{budget}/transactions?account_id={account}&limit=1000')
        rows = register['data']
        wanted = expected['transactions']
        assert len(rows) == len(wanted), (rows, wanted)
        actual = {r['id']: r for r in rows}
        assert set(actual) == {r['id'] for r in wanted}
        mapping = {'account_id': 'account', 'payee_id': 'payee', 'category_id': 'category', 'amount': 'amount', 'date': 'date', 'notes': 'notes', 'cleared': 'cleared', 'reconciled': 'reconciled'}
        for old in wanted:
            for native, source in mapping.items():
                assert actual[old['id']].get(native) == old.get(source), (old['id'], native, actual[old['id']].get(native), old.get(source))
        seen = {}
        for event in state['events']:
            if not event['path'].endswith('/android-upgrade/operations'):
                continue
            op = event['headers'].get('Idempotency-Key')
            body = event['body']
            assert json.loads(body)['operation'] == original_ops[op], 'Original operation/messages changed'
            assert event['headers'].get('X-Actual-Edit-Clock') is None, 'Batch clock replaced original messages'
            identity = (body, event['headers'].get('X-Actual-Budget-Epoch'))
            if op in seen:
                assert seen[op] == identity, 'Retry identity changed'
            seen[op] = identity
        assert set(seen) == set(original_ops), 'Some original operations never reconciled'
        totals = register['balances']
        assert totals['total'] == sum((r['amount'] for r in wanted))
        assert totals['cleared'] == sum((r['amount'] for r in wanted if r['cleared']))
        assert totals['uncleared'] == sum((r['amount'] for r in wanted if not r['cleared']))
        return {'pass': True, 'rows': rows, 'balances': totals, 'original_operations': len(seen), 'dropped_ack': state['dropped']}

    class Handler(BaseHTTPRequestHandler):

        def log_message(self, *_):
            pass

        def reply(self, status, body):
            raw = json.dumps(body).encode()
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(raw)))
            self.send_header('Connection', 'close')
            self.close_connection = True
            self.end_headers()
            self.wfile.write(raw)

        def handle_request(self):
            if self.path == '/__fixture__/allow':
                with lock:
                    state['blocked'] = False
                    persist()
                return self.reply(200, {'allowed': True})
            if self.path == '/__fixture__/status':
                return self.reply(200, state)
            if self.path == '/__fixture__/verify':
                try:
                    result = verify()
                    (output / 'verified.json').write_text(json.dumps(result, indent=2) + '\n')
                    return self.reply(200, result)
                except Exception as error:
                    return self.reply(500, {'error': str(error)})
            body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
            headers = {k: self.headers.get(k) for k in ['Idempotency-Key', 'X-Actual-Edit-Clock', 'X-Actual-Budget-Epoch', 'X-Actual-Receipt-Only'] if self.headers.get(k) is not None}
            event = {'method': self.command, 'path': self.path, 'headers': headers, 'body': body.decode()}
            migration = self.path.endswith('/android-upgrade/operations')
            with lock:
                if migration and state['blocked']:
                    event['status'] = 503
                    event['blocked'] = True
                    state['events'].append(event)
                    persist()
                    return self.reply(503, {'error': {'code': 'fixture_offline', 'message': 'Delivery paused by disposable fixture'}})
            connection = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=30)
            forwarded = dict(self.headers)
            forwarded['Host'] = f'127.0.0.1:{backend_port}'
            forwarded['Connection'] = 'close'
            connection.request(self.command, self.path, body=body, headers=forwarded)
            response = connection.getresponse()
            raw = response.read()
            response_headers = response.getheaders()
            connection.close()
            with lock:
                event['status'] = response.status
                state['events'].append(event)
                drop = migration and response.status == 200 and (not state['dropped']) and (self.headers.get('X-Actual-Receipt-Only') is None)
                if drop:
                    state['dropped'] = True
                    state['blocked'] = True
                    event['lost_ack'] = True
                persist()
            if drop:
                self.connection.shutdown(socket.SHUT_RDWR)
                self.connection.close()
                return
            self.send_response(response.status)
            for key, value in response_headers:
                if key.lower() not in ['connection', 'transfer-encoding', 'content-length']:
                    self.send_header(key, value)
            self.send_header('Content-Length', str(len(raw)))
            self.send_header('Connection', 'close')
            self.close_connection = True
            self.end_headers()
            self.wfile.write(raw)
        do_GET = do_POST = do_PATCH = do_DELETE = handle_request
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    config = {'proxy_url': f'http://127.0.0.1:{server.server_port}', 'emulator_url': f'http://10.0.2.2:{server.server_port}', 'backend_url': backend, 'budget': budget, 'account': account, 'epoch': request(f'/budgets/{budget}/checkpoint')['data']['epoch'], 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'pid': os.getpid(), 'backend_pid': child.pid}
    (output / 'config.json').write_text(json.dumps(config, indent=2) + '\n')
    print(json.dumps(config), flush=True)
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
finally:
    child.terminate()
    child.wait(timeout=15)
