#!/usr/bin/env python3
"""Small remote-only caller. No Actual engine, third-party dependencies or credential commands.

Financial batches/import commits use an immutable, atomically saved outbox.
Outbox files contain financial data; keep them private. See docs/PUBLIC-API.md.
"""
import argparse
import base64
import datetime
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import secrets
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import uuid

MAX_JSON = 65536
MAX_UPLOAD = 16 * 1024 * 1024
MAX_RESPONSE = 4 * 1024 * 1024
# UTF-8 response text may expand sixfold when JSON escapes control characters.
MAX_OUTBOX = 4 * ((MAX_UPLOAD + 2) // 3) + 12 * MAX_RESPONSE + 1024 * 1024
UUID = r"[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}"


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False,
                      separators=(",", ":")).encode("utf-8")


def read_bytes(path, maximum):
    with Path(path).open("rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        raise ValueError("Input exceeds its documented size limit")
    return data


def read_json(path, maximum=MAX_JSON):
    return json.loads(read_bytes(path, maximum))


def digest(request):
    return hashlib.sha256(encode(request)).hexdigest()


def save(path, value, exclusive=False):
    """Save attempted state before delivery; failure prevents network delivery."""
    target = Path(path)
    if exclusive:
        # Reserve identity before preparing the atomic replacement. Never replace
        # an earlier uncertain request merely because preparation was repeated.
        descriptor = os.open(target, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(descriptor)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="wb", dir=target.parent,
                                         prefix=".actual-api-", delete=False) as stream:
            temporary = stream.name
            os.chmod(temporary, 0o600)
            stream.write(encode(value))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, target)
        temporary = None
        descriptor = os.open(target.parent, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        if temporary is not None:
            os.unlink(temporary)
        # A failed preparation may leave its reserved empty file. It is visible,
        # never silently reused/replaced; inspect/remove it explicitly.


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        return None


def fetch(url, method="GET", body=None, headers=None):
    request = urllib.request.Request(url, data=body, method=method,
                                     headers=headers or {})
    try:
        response = urllib.request.build_opener(NoRedirect()).open(request, timeout=30)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        if 300 <= response.status < 400:
            raise ValueError("Redirect refused; captured server/method/body remain unchanged")
        data = response.read(MAX_RESPONSE + 1)
        if len(data) > MAX_RESPONSE:
            raise ValueError("Response exceeded4MiB; original outbox remains retained")
        try:
            value = json.loads(data)
        except (UnicodeDecodeError, json.JSONDecodeError):
            raise ValueError("Non-JSON response; original outbox remains retained") from None
        return response.status, value


def context(args):
    if not args.server:
        raise ValueError("Provide --server for a new read/intent")
    parsed = urllib.parse.urlsplit(args.server)
    if parsed.scheme not in ("http", "https") or not parsed.netloc or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError("Use an HTTP(S) server URL without credentials/query/fragment")
    api = args.server.rstrip("/") + "/api/v1"
    if args.budget:
        if not re.fullmatch(UUID, args.budget):
            raise ValueError("Provide a budget UUID, never a budget name")
        return api, api + "/budgets/" + args.budget
    return api, None


def relative(path):
    if not path.startswith("/") or path.startswith("//") or "#" in path or "://" in path or ".." in urllib.parse.unquote(path):
        raise ValueError("Provide a relative API resource path")
    return path


def next_clock(observed):
    if not isinstance(observed, str) or not re.fullmatch(
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z-[0-9A-F]{4}-[0-9a-fA-F]{16}", observed):
        raise ValueError("Server did not supply a canonical edit clock")
    date = datetime.datetime.fromisoformat(observed[:24].replace("Z", "+00:00"))
    epoch = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
    tick = datetime.timedelta(milliseconds=1)
    millis = (date - epoch) // tick
    now = (datetime.datetime.now(datetime.timezone.utc) - epoch) // tick
    if millis < 0 or millis - now > 300_000:
        raise ValueError("Observed clock exceeds five-minute skew bound; correct clocks before preparing")
    counter = int(observed[25:29], 16) + 1 if millis >= now else 0
    if counter > 65535:
        raise ValueError("Logical clock counter exhausted; wait for time to advance before preparing")
    date = epoch + max(now, millis) * tick
    return date.isoformat(timespec="milliseconds").replace("+00:00", "Z") + f"-{counter:04X}-" + secrets.token_hex(8)


def prepare(args, upload=False):
    _, root = context(args)
    if not root:
        raise ValueError("New mutations require --budget")
    status, checkpoint = fetch(root + "/checkpoint?include_clock=true")
    if status != 200:
        return status, checkpoint
    checkpoint = checkpoint["data"]
    if upload:
        if not re.fullmatch(UUID, args.account):
            raise ValueError("Provide an account UUID")
        path = "/accounts/" + args.account + "/transaction-imports?" + urllib.parse.urlencode({"filename": Path(args.file).name})
        body = read_bytes(args.file, MAX_UPLOAD)
        content_type = "application/octet-stream"
    else:
        path = relative(args.path)
        if not re.fullmatch(r"/transactions/batch|/transaction-imports/" + UUID + r"/commit", path):
            raise ValueError("This example outbox supports transaction batches/import commits only; credential commands are deliberately excluded")
        value = read_json(args.body)
        if not isinstance(value, dict):
            raise ValueError("Command must be a JSON object")
        if "snapshot" in value and value["snapshot"] != checkpoint["snapshot"]:
            raise ValueError("Supplied review snapshot is stale; original input file unchanged. Review before preparing a new intent")
        value.setdefault("snapshot", checkpoint["snapshot"])
        body = encode(value)
        if len(body) > MAX_JSON:
            raise ValueError("Command exceeds64KiB")
        content_type = "application/json"
    clock = next_clock(checkpoint.get("edit_clock"))
    owner = args.owner or str(uuid.uuid4())
    if not re.fullmatch(UUID, owner):
        raise ValueError("History owner must be a UUID")
    request = {"url": root + path, "method": "POST", "headers": {
        "Content-Type": content_type, "Idempotency-Key": str(uuid.uuid4()),
        "X-Actual-Budget-Epoch": checkpoint["epoch"],
        "X-Actual-Edit-Clock": clock, "X-Actual-History-Owner": owner,
    }, "body_base64": base64.b64encode(body).decode("ascii")}
    save(args.outbox, {"version": 1, "state": "prepared", "attempted": False,
                       "request": request, "request_sha256": digest(request),
                       "upload": upload}, exclusive=True)
    return 200, {"state": "prepared", "outbox": args.outbox,
                 "idempotency_key": request["headers"]["Idempotency-Key"]}


def deliver(args):
    box = read_json(args.outbox, MAX_OUTBOX)
    if box.get("version") != 1 or digest(box["request"]) != box["request_sha256"]:
        raise ValueError("Outbox identity changed; retained file requires explicit review")
    request = box["request"]
    headers = dict(request["headers"])
    if args.command != "send" and box.get("upload"):
        raise ValueError("Uploads have no receipt-only/cancel route. Resend the original upload; financial rows change only after a reviewed commit")
    if args.command == "receipt":
        headers["X-Actual-Receipt-Only"] = "true"
    elif args.command == "cancel":
        headers["X-Actual-Cancel-If-Uncommitted"] = "true"
    if args.command != "receipt":
        box["attempted"] = True
        if box.get("state") not in ("committed", "canceled"):
            box["state"] = "attempted"
        save(args.outbox, box)
    status, response = fetch(request["url"], request["method"],
                             base64.b64decode(request["body_base64"], validate=True), headers)
    if 200 <= status < 300:
        box["state"] = "committed"
        box.setdefault("committed_response", {"status": status, "body": response})
    elif response.get("error", {}).get("code") == "operation_canceled":
        box["state"] = "canceled"
    box["last_response"] = {"status": status, "body": response}
    save(args.outbox, box)
    return status, response


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server")
    parser.add_argument("--budget")
    commands = parser.add_subparsers(dest="command", required=True)
    read = commands.add_parser("read")
    read.add_argument("--path", default="/info")
    query = commands.add_parser("query")
    query.add_argument("--body", required=True)
    preview = commands.add_parser("preview")
    preview.add_argument("--path", required=True)
    preview.add_argument("--body", required=True)
    for name in ("prepare", "prepare-upload"):
        command = commands.add_parser(name)
        command.add_argument("--outbox", required=True)
        command.add_argument("--owner")
        if name == "prepare":
            command.add_argument("--path", required=True)
            command.add_argument("--body", required=True)
        else:
            command.add_argument("--account", required=True)
            command.add_argument("--file", required=True)
    for name in ("send", "receipt", "cancel"):
        commands.add_parser(name).add_argument("--outbox", required=True)
    args = parser.parse_args()
    if args.command in ("send", "receipt", "cancel"):
        if args.server or args.budget:
            raise ValueError("Delivery uses its captured server/budget; do not rebase pending work")
        status, result = deliver(args)
    elif args.command.startswith("prepare"):
        status, result = prepare(args, args.command == "prepare-upload")
    else:
        api, root = context(args)
        if args.command == "read":
            status, result = fetch((root or api) + relative(args.path))
        else:
            if not root:
                raise ValueError("Read-only queries/previews require --budget")
            path = "/query" if args.command == "query" else relative(args.path)
            if not re.fullmatch(r"/query|/transaction-imports/" + UUID + r"/preview|/transactions/merge/preview|/notes/read|/reports/(query|drilldown)", path):
                raise ValueError("Use a documented read-only query/preview route")
            status, result = fetch(root + path, "POST", read_bytes(args.body, MAX_JSON), {"Content-Type": "application/json"})
    print(json.dumps({"status": status, "body": result}, ensure_ascii=False))
    return 0 if 200 <= status < 300 or result.get("error", {}).get("code") == "operation_canceled" else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, urllib.error.URLError, http.client.HTTPException) as error:
        print("Caller stopped; original request retained: " + str(error), file=sys.stderr)
        sys.exit(1)
