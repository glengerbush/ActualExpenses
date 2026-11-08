# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Validate real public API responses against the served OpenAPI schemas.

Run with `uv run actualBudget/tools/contract-smoke.py` after building the server.
Starts its own disposable server; never connects to a configured/live budget.
Financial semantics are covered separately by the pinned Actual oracle tests.
"""

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


def request(base, path, method="GET", body=None, headers=None):
    payload = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        base + path, data=payload, method=method,
        headers={"Content-Type": "application/json", **(headers or {})},
    )
    try:
        response = urllib.request.urlopen(req, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, json.load(response)


def main():
    project = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="actualbudget-contract-") as data:
        env = dict(os.environ, ACTUALBUDGET_DATA_DIR=data,
                   ACTUALBUDGET_HOST="127.0.0.1", ACTUALBUDGET_PORT="0",
                   ACTUALBUDGET_WEB_DIR=str(project / "web/dist"))
        env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
        process = subprocess.Popen(
            [str(project / "server/target/debug/actualbudget-server")],
            env=env, stdout=subprocess.PIPE, text=True,
        )
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                if not selector.select(15):
                    raise RuntimeError("Disposable server did not start within 15 seconds")
                line = process.stdout.readline().strip()
            assert line.startswith("LISTENING "), line
            base = "http://" + line.removeprefix("LISTENING ")
            status, contract = request(base, "/api/v1/openapi.json")
            assert status == 200 and contract["openapi"] == "3.1.0"
            for schema in contract["components"]["schemas"].values():
                Draft202012Validator.check_schema(schema)

            def check(path, template, method="GET", body=None, headers=None, expected=200):
                status, result = request(base, path, method, body, headers)
                assert status == expected, (path, status, result)
                schema = contract["paths"][template][method.lower()]["responses"][str(status)]["content"]["application/json"]["schema"]
                document = {**schema, "components": contract["components"]}
                Draft202012Validator(document, format_checker=FormatChecker()).validate(result)
                return result

            check("/api/v1/info", "/api/v1/info")
            budgets = "/api/v1/budgets"
            budget_id, account_id = str(uuid.uuid4()), str(uuid.uuid4())
            budget = {"id": budget_id, "name": "Contract fixture", "budget_type": "envelope"}
            check(budgets, budgets, "POST", budget, {"Idempotency-Key": "budget"}, 201)
            check(budgets, budgets)
            check(budgets, budgets, "POST", {**budget, "name": "Changed"}, {"Idempotency-Key": "budget"}, 409)
            check(budgets, budgets, "POST", {}, {"Idempotency-Key": "invalid"}, 400)
            check(budgets, budgets, "POST", budget, {"Idempotency-Key": "cross-origin", "Origin": "http://other.invalid"}, 403)
            accounts = f"{budgets}/{budget_id}/accounts"
            account_template = budgets + "/{budget}/accounts"
            account = {"id": account_id, "name": "Cash", "opening_balance": 12345, "opening_date": "2026-10-15"}
            check(accounts, account_template, "POST", account, {"Idempotency-Key": "account"}, 201)
            check(accounts, account_template)
            check(f"{budgets}/{uuid.uuid4()}/accounts", account_template, expected=404)
            check(f"{budgets}/{budget_id}/transactions?account_id={account_id}", budgets + "/{budget}/transactions")
            print("PASS: served OpenAPI schemas validate real success, validation, conflict, origin and missing-resource responses")
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            process.stdout.close()


if __name__ == "__main__":
    main()
