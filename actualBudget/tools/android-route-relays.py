#!/usr/bin/env python3
"""Expose only this session's disposable fixture through explicit local interface addresses."""
import argparse
import json
import pathlib
import select
import signal
import socket
import socketserver
import threading
from urllib.parse import urlsplit
p = argparse.ArgumentParser()
p.add_argument('--fixture', type=pathlib.Path, required=True)
p.add_argument('--address', action='append', required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
a = p.parse_args()
c = json.loads(a.fixture.read_text())
target = urlsplit(c['proxy_url'])
assert target.hostname == '127.0.0.1'

class Relay(socketserver.BaseRequestHandler):

    def handle(self):
        with socket.create_connection((target.hostname, target.port), timeout=30) as upstream:
            peers = [self.request, upstream]
            while True:
                readable, _, _ = select.select(peers, [], [], 30)
                if not readable:
                    return
                for source in readable:
                    data = source.recv(16384)
                    if not data:
                        return
                    (upstream if source is self.request else self.request).sendall(data)

class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True
servers = []
try:
    for address in a.address:
        socket.inet_aton(address)
        server = Server((address, 0), Relay)
        servers.append(server)
        threading.Thread(target=server.serve_forever, daemon=True).start()
    result = {**c, 'route_urls': [f'http://{address}:{server.server_address[1]}' for address, server in zip(a.address, servers)]}
    a.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        pass
finally:
    for server in servers:
        server.shutdown()
        server.server_close()
