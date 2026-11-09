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
    payload = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
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
            references = {}
            for kind in ("payees", "categories"):
                path = f"{budgets}/{budget_id}/{kind}"
                template = budgets + "/{budget}/" + kind
                reference = {"id": str(uuid.uuid4()), "name": f"Contract {kind}"}
                if kind == "categories":
                    reference["is_income"] = False
                check(path, template, "POST", reference, {"Idempotency-Key": kind}, 201)
                check(path, template)
                references[kind] = reference["id"]
            entries = f"{budgets}/{budget_id}/transactions"
            entries_template = budgets + "/{budget}/transactions"
            transaction = {
                "id": str(uuid.uuid4()), "account_id": account_id, "date": "2026-10-16",
                "amount": -123, "cleared": True, "notes": "Contract expense",
                "payee_id": references["payees"], "category_id": references["categories"],
            }
            check(entries, entries_template, "POST", transaction, {"Idempotency-Key": "entry"}, 201)
            entry = entries + "/" + transaction["id"]
            entry_template = entries_template + "/{id}"
            check(entry, entry_template)
            check(entry, entry_template, "PATCH", {"notes": "Contract edit"}, {"Idempotency-Key": "edit"})
            check(entry, entry_template, "PATCH", {"amount": "invalid"}, {"Idempotency-Key": "invalid-edit"}, 400)
            duplicate = {"id": str(uuid.uuid4())}
            check(entry + "/duplicate", entry_template + "/duplicate", "POST", duplicate, {"Idempotency-Key": "duplicate"}, 201)
            page = check(entries + f"?account_id={account_id}&limit=1", entries_template)
            continuation = entries + f"?account_id={account_id}&limit=1&offset=1&snapshot={page['snapshot']}"
            check(continuation, entries_template)
            check(entries + "?as_of=2026-10-15", entries_template)
            check(entry, entry_template, "DELETE", headers={"Idempotency-Key": "delete"})
            check(continuation, entries_template, expected=409)
            check(entry, entry_template, "DELETE", headers={"Idempotency-Key": "delete"})
            check(entry, entry_template, expected=404)
            calculator = "/api/v1/amounts/evaluate"
            check(calculator, calculator, "POST", {"expression": "2.675", "decimal_places": 2})
            check(calculator, calculator, "POST", {"expression": "1/0"}, expected=400)
            preferences = f"{budgets}/{budget_id}/preferences"
            preferences_template = budgets + "/{budget}/preferences"
            check(preferences, preferences_template)
            check(preferences, preferences_template, "PATCH", {"number_format": "dot-comma", "experimental": {"currency": True}}, {"Idempotency-Key": "preferences"})
            check(preferences, preferences_template, "PATCH", {"unknown": True}, {"Idempotency-Key": "invalid-preferences"}, 400)
            conversion = "/api/v1/preferences/convert-actual"
            check(conversion, conversion, "POST", {"synced": {"numberFormat": "space-comma", "flags.customThemes": "true"}, "global": {"theme": "midnight", "language": "fr"}})

            # New clocked operations retain the original body, clock and receipt.
            ordered = {**transaction, "id": str(uuid.uuid4())}
            zero = "1970-01-01T00:00:00.000Z-0000-0000000000000000"
            clock1 = "2026-01-01T00:00:00.000Z-0000-0000000000000001"
            clock2 = "2026-01-02T00:00:00.000Z-0000-0000000000000001"
            check(entries, entries_template, "POST", ordered,
                  {"Idempotency-Key": "ordered-create", "X-Actual-Edit-Clock": zero}, 201)
            ordered_path = entries + "/" + ordered["id"]
            page = check(entries + "?limit=1", entries_template)
            changes_path = f"{budgets}/{budget_id}/changes"
            changes_template = budgets + "/{budget}/changes"
            check(changes_path + "?cursor=" + page["snapshot"], changes_template)
            check(ordered_path, entry_template, "PATCH", {"notes": "Newer"},
                  {"Idempotency-Key": "ordered-new", "X-Actual-Edit-Clock": clock2})
            check(ordered_path, entry_template, "PATCH", {"notes": "Older", "amount": -10},
                  {"Idempotency-Key": "ordered-old", "X-Actual-Edit-Clock": clock1})
            check(ordered_path, entry_template, "DELETE", headers={
                  "Idempotency-Key": "ordered-delete", "X-Actual-Edit-Clock": clock1}, expected=409)
            check(changes_path + "?cursor=" + page["snapshot"], changes_template)
            check(changes_path + "?cursor=invalid", changes_template, expected=409)

            # Cancellation either replays a receipt or durably fences delayed delivery.
            cancel = {"X-Actual-Cancel-If-Uncommitted": "true"}
            check(entries, entries_template, "POST", ordered,
                  {"Idempotency-Key": "ordered-create", "X-Actual-Edit-Clock": zero, **cancel}, 201)
            canceled = {**transaction, "id": str(uuid.uuid4())}
            operations = [
                (entries, entries_template, "POST", canceled),
                (ordered_path, entry_template, "PATCH", {"notes": "Canceled edit"}),
                (ordered_path, entry_template, "DELETE", None),
                (ordered_path + "/duplicate", entry_template + "/duplicate", "POST", {"id": str(uuid.uuid4())}),
            ]
            for index, (path, template, method, body) in enumerate(operations):
                headers = {"Idempotency-Key": f"cancel-{index}", "X-Actual-Edit-Clock": clock2}
                outcome = check(path, template, method, body, {**headers, **cancel}, 409)
                assert outcome["error"]["code"] == "operation_canceled", outcome
                assert check(path, template, method, body, headers, 409) == outcome
            check(entries + "/" + canceled["id"], entry_template, expected=404)
            check(entries, entries_template, "POST", canceled,
                  {"Idempotency-Key": "invalid-cancel", "X-Actual-Cancel-If-Uncommitted": "false"}, 400)

            companion = f"{budgets}/{budget_id}/companion"
            companion_template = budgets + "/{budget}/companion"
            view = check(companion + f"?account_id={account_id}&retain={ordered['id']}",
                         companion_template)["data"]
            assert len(view["recent"]) <= 10
            check(companion + f"?account_id={account_id}&snapshot={view['snapshot']}", companion_template)
            check(companion + f"?account_id={account_id}&retain=invalid", companion_template, expected=400)

            # Import HTTP responses use the unchanged pinned public export fixture.
            imports = "/api/v1/imports"
            stage = check(imports + "/actual", imports + "/actual", "POST",
                          (project / "tests/fixtures/import-26.10.0.zip").read_bytes(),
                          {"Content-Type": "application/zip", "Idempotency-Key": "stage"}, 201)["data"]
            check(imports, imports)
            stage_path = imports + "/" + stage["id"]
            check(stage_path, imports + "/{id}")
            check(stage_path + "/activate", imports + "/{id}/activate", "POST", {},
                  {"Idempotency-Key": "activate"}, 201)
            check(imports + "/actual", imports + "/actual", "POST", b"invalid zip",
                  {"Content-Type": "application/zip", "Idempotency-Key": "invalid-stage"}, 422)
            blocked = check(imports + "/actual", imports + "/actual", "POST",
                            (project / "tests/fixtures/import-unsupported-rule-26.10.0.zip").read_bytes(),
                            {"Content-Type": "application/zip", "Idempotency-Key": "blocked-stage"}, 201)["data"]
            check(imports + "/" + blocked["id"] + "/activate", imports + "/{id}/activate", "POST", {},
                  {"Idempotency-Key": "blocked-activate"}, 409)

            themes = "/api/v1/themes"
            check(themes + "/catalog", themes + "/catalog")
            for css in (":root { --color-pageText: #123456; }", "body { color: red; }"):
                check(themes + "/compile", themes + "/compile", "POST", {"css": css})
            check(themes + "/compile", themes + "/compile", "POST", {}, expected=400)
            check(themes + "/convert-actual", themes + "/convert-actual", "POST", {"global": {
                "installed-custom-theme": "{bad", "custom-css-override": ":root { --color-pageText: #123456; }",
            }})
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
