#!/usr/bin/env python3
"""Measure unchanged public refreshes and Linux process I/O, using disposable data."""
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

parser = argparse.ArgumentParser()
parser.add_argument('--history', type=int, default=5000)
args = parser.parse_args()
assert args.history >= 0
root = Path(__file__).resolve().parents[1]

with tempfile.TemporaryDirectory(prefix='actualbudget-changes-scale-') as directory:
    child = subprocess.Popen([str(root / 'server/target/debug/actualbudget-server')],
                             env={**os.environ, 'ACTUALBUDGET_DATA_DIR': directory,
                                  'ACTUALBUDGET_HOST': '127.0.0.1', 'ACTUALBUDGET_PORT': '0'},
                             stdout=subprocess.PIPE, text=True)
    connection = None
    try:
        line = child.stdout.readline().strip()
        assert line.startswith('LISTENING '), line
        connection = http.client.HTTPConnection(line.removeprefix('LISTENING '), timeout=10)

        def call(method, path, body=None):
            start = time.perf_counter()
            connection.request(method, '/api/v1' + path,
                               json.dumps(body) if body is not None else None,
                               {'Content-Type': 'application/json', 'Idempotency-Key': str(uuid.uuid4())})
            response = connection.getresponse()
            raw = response.read()
            value = json.loads(raw)
            assert response.status in (200, 201), (response.status, value)
            return value, (time.perf_counter() - start) * 1000, len(raw)

        budget, account = str(uuid.uuid4()), str(uuid.uuid4())
        call('POST', '/budgets', {'id': budget, 'name': 'Disposable refresh probe'})
        call('POST', f'/budgets/{budget}/accounts', {'id': account, 'name': 'Spending', 'opening_balance': 0, 'opening_date': '2026-01-01'})
        register = f'/budgets/{budget}/transactions'

        def process_metrics():
            io = dict(line.split(': ') for line in Path(f'/proc/{child.pid}/io').read_text().splitlines())
            stat = Path(f'/proc/{child.pid}/stat').read_text().rsplit(') ', 1)[1].split()
            rss = int(Path(f'/proc/{child.pid}/statm').read_text().split()[1]) * os.sysconf('SC_PAGE_SIZE')
            ticks = int(stat[11]) + int(stat[12])
            return int(io['write_bytes']), ticks * 1000 / os.sysconf('SC_CLK_TCK'), rss

        def measure():
            page, _, _ = call('GET', register + '?limit=1')
            cursor = page['snapshot']
            path = f'/budgets/{budget}/changes?cursor={cursor}'
            for _ in range(5):
                call('GET', path)
            before = process_metrics()
            times, sizes = [], set()
            for _ in range(100):
                result, elapsed, size = call('GET', path)
                assert result['data'] == [] and result['next_cursor'] == cursor and not result['has_more']
                times.append(elapsed)
                sizes.add(size)
            after = process_metrics()
            assert after[0] == before[0], 'Unchanged refresh caused process storage writes'
            assert len(sizes) == 1
            return {'median_ms': round(statistics.median(times), 3),
                    'p95_ms': round(sorted(times)[94], 3), 'response_bytes': sizes.pop(),
                    'storage_write_bytes': after[0] - before[0],
                    'server_cpu_ms_100_refreshes': after[1] - before[1],
                    'server_rss_bytes': after[2]}

        results = {'history': args.history, 'empty': measure()}
        for _ in range(args.history):
            call('POST', register, {'id': str(uuid.uuid4()), 'account_id': account,
                                   'date': '2026-01-02', 'amount': 1, 'cleared': True})
        results['with_history'] = measure()
        final, _, _ = call('GET', register + '?limit=1')
        assert final['balances']['total'] == args.history
        print(json.dumps(results, indent=2))
    finally:
        if connection:
            connection.close()
        child.terminate()
        child.wait(timeout=10)
