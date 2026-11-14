# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Validate real public API responses against the served OpenAPI schemas.

Run with `uv run actualBudget/tools/contract-smoke.py` after building the server.
Starts its own disposable server; never connects to a configured/live budget.
Financial semantics are covered separately by the pinned Actual oracle tests.
"""

import datetime
import base64
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import urllib.error
import urllib.parse
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
            import_fragment = json.loads((project / 'server/src/transaction_imports/openapi.json').read_text())
            for path, operations in import_fragment['paths'].items():
                assert contract['paths'][path] == operations
            for name, schema in import_fragment['components']['schemas'].items():
                assert contract['components']['schemas'][name] == schema
            query_fragment = json.loads((project / 'server/src/public_query/openapi.json').read_text())
            for path, operations in query_fragment['paths'].items():
                assert contract['paths'][path] == operations
            for name, schema in query_fragment['components']['schemas'].items():
                assert contract['components']['schemas'][name] == schema
            for schema in contract["components"]["schemas"].values():
                Draft202012Validator.check_schema(schema)

            def check(path, template, method="GET", body=None, headers=None, expected=200):
                status, result = request(base, path, method, body, headers)
                assert status == expected, (path, status, result)
                schema = contract["paths"][template][method.lower()]["responses"][str(status)]["content"]["application/json"]["schema"]
                document = {**schema, "components": contract["components"]}
                Draft202012Validator(document, format_checker=FormatChecker()).validate(result)
                return result

            def fresh_installation():
                nonlocal process, base
                process.terminate()
                process.wait(timeout=10)
                env["ACTUALBUDGET_DATA_DIR"] = str(Path(data) / str(uuid.uuid4()))
                process = subprocess.Popen([str(project / "server/target/debug/actualbudget-server")], env=env, stdout=subprocess.PIPE, text=True)
                line = process.stdout.readline().strip()
                assert line.startswith("LISTENING "), line
                base = "http://" + line.removeprefix("LISTENING ")

            check("/api/v1/info", "/api/v1/info")
            # Retained YNAB files use the same typed stage/activation contract.
            for source_kind, filename in [("ynab4", "ynab4-demo.zip"), ("nynab", "nynab-demo.json")]:
                fresh_installation()
                original = (project / "tests/fixtures/ynab" / filename).read_bytes()
                path = "/api/v1/imports/" + source_kind
                upload_path = path + "?filename=" + urllib.parse.quote(filename)
                headers = {"Idempotency-Key": "ynab-contract-" + source_kind}
                staged = check(upload_path, path, "POST", original, headers, 201)
                assert staged["data"]["report"]["source_format"] == source_kind
                assert staged["data"]["status"] == "ready", (source_kind, staged["data"]["report"])
                assert staged["data"]["report"]["checks"]["sqlite_integrity"] is None
                assert check(upload_path, path, "POST", original, headers, 201) == staged
                sid = staged["data"]["id"]
                with urllib.request.urlopen(base + f"/api/v1/imports/{sid}/original", timeout=5) as response:
                    assert response.read() == original
                check(f"/api/v1/imports/{sid}", "/api/v1/imports/{id}")
                activated = check(f"/api/v1/imports/{sid}/activate", "/api/v1/imports/{id}/activate", "POST", {}, {"Idempotency-Key": "ynab-activate-" + source_kind}, 201)
                assert activated["data"]["id"] == staged["data"]["budget_id"]
                check(upload_path, path, "POST", b"not a budget", {"Idempotency-Key": "ynab-invalid-" + source_kind}, 422)
            budgets = "/api/v1/budgets"
            budget_id, account_id = str(uuid.uuid4()), str(uuid.uuid4())
            budget = {"id": budget_id, "name": "Contract fixture", "budget_type": "tracking"}
            fresh_installation()
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
            recovery_path = f"{budgets}/{budget_id}/recovery"
            recovery_template = budgets + "/{budget}/recovery"
            recovery = check(recovery_path, recovery_template)["data"]
            assert recovery["can_apply"] is False and recovery["backup_restorable"] is True
            repair = {"preview_token": recovery["preview_token"], "snapshot": recovery["snapshot"], "confirm_name": budget["name"]}
            repair_headers = {"Idempotency-Key": "recovery-contract", "X-Actual-Budget-Epoch": recovery["epoch"]}
            check(recovery_path, recovery_template, "POST", repair, repair_headers, 409)
            cancelled = check(recovery_path, recovery_template, "POST", repair,
                              {**repair_headers, "X-Actual-Cancel-If-Uncommitted": "true"}, 409)
            assert cancelled["error"]["code"] == "operation_canceled"
            assert check(recovery_path, recovery_template, "POST", repair, repair_headers, 409) == cancelled
            assert check(recovery_path, recovery_template)["data"] == recovery
            check(f"{budgets}/{uuid.uuid4()}/accounts", account_template, expected=404)
            check(f"{budgets}/{budget_id}/transactions?account_id={account_id}", budgets + "/{budget}/transactions")
            for path in [accounts + "/" + account_id + "/reconciliation"]:
                assert request(base, path)[0] == 404
            register_settings_path=f"{budgets}/{budget_id}/register-settings"
            register_settings_template=budgets+"/{budget}/register-settings"
            register_settings=check(register_settings_path,register_settings_template)["data"]
            check(register_settings_path,register_settings_template,"PATCH",{"revision":0,"settings":{"views":{},"saved_filters":[]}}, {"Idempotency-Key":"register-settings","X-Actual-Budget-Epoch":register_settings["epoch"]})
            check(register_settings_path,register_settings_template,"PATCH",{"revision":0,"settings":{"views":{},"saved_filters":[]}}, {"Idempotency-Key":"stale-register-settings","X-Actual-Budget-Epoch":register_settings["epoch"]},409)
            group_path = f"{budgets}/{budget_id}/account-groups"
            group_template = budgets + "/{budget}/account-groups"
            group_id = str(uuid.uuid4())
            check(group_path, group_template, "POST", {"id": group_id, "name": "Account group"}, {"Idempotency-Key": "group"}, 201)
            check(group_path, group_template)
            check(group_path + "/" + group_id, group_template + "/{id}", "PATCH", {"name": "Renamed group"}, {"Idempotency-Key": "group-name"})
            check(accounts + "/" + account_id, account_template + "/{id}", "PATCH", {"notes": "Account contract", "group_id": group_id}, {"Idempotency-Key": "account-edit"})
            check(accounts + "/" + account_id, account_template + "/{id}")
            check(accounts + "/" + account_id + "/move", account_template + "/{id}/move", "POST", {"before_id": None}, {"Idempotency-Key": "account-move"})
            check(group_path + "/" + group_id + "/move", group_template + "/{id}/move", "POST", {"before_id": None}, {"Idempotency-Key": "group-move"})
            check(group_path + "/" + group_id, group_template + "/{id}", "DELETE", {}, {"Idempotency-Key": "group-delete"})
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
            locations = f"{budgets}/{budget_id}/payee-locations"
            location_template = budgets + "/{budget}/payee-locations"
            location = {"id": str(uuid.uuid4()), "payee_id": references["payees"], "latitude": 40.123456, "longitude": -74.123456}
            check(locations, location_template, "POST", location, {"Idempotency-Key": "location"}, 201)
            check(locations, location_template)
            check(locations + "/" + location["id"], location_template + "/{id}")
            check(locations + "/" + location["id"], location_template + "/{id}", "PATCH", {"deleted": True}, {"Idempotency-Key": "forget-location"})
            check(locations, location_template, "POST", {**location,"latitude":91}, {"Idempotency-Key": "bad-location"}, 400)
            check(locations, location_template, "POST", {**location,"id":str(uuid.uuid4())}, {"Idempotency-Key": "cancel-location", "X-Actual-Cancel-If-Uncommitted":"true"}, 409)
            categories = f"{budgets}/{budget_id}/categories"
            categories_template = budgets + "/{budget}/categories"
            category_groups = f"{budgets}/{budget_id}/category-groups"
            category_groups_template = budgets + "/{budget}/category-groups"
            category_group, extra_category = str(uuid.uuid4()), str(uuid.uuid4())
            check(category_groups, category_groups_template, "POST", {"id": category_group, "name": "Category contract"}, {"Idempotency-Key": "category-group"}, 201)
            check(category_groups, category_groups_template)
            check(category_groups + "/" + category_group, category_groups_template + "/{id}")
            check(category_groups + "/" + category_group, category_groups_template + "/{id}", "PATCH", {"name": "Renamed category group"}, {"Idempotency-Key": "rename-category-group"})
            check(categories, categories_template, "POST", {"id": extra_category, "name": "Merge source", "group_id": category_group, "sort_order": None}, {"Idempotency-Key": "extra-category"}, 201)
            check(categories + "/" + extra_category, categories_template + "/{id}")
            check(categories + "/" + extra_category, categories_template + "/{id}", "PATCH", {"name": "Renamed category"}, {"Idempotency-Key": "category-name"})
            check(categories + "/" + extra_category + "/transactions", categories_template + "/{id}/transactions")
            check(categories + "/" + extra_category, categories_template + "/{id}", "DELETE", {"replacement_id": references["categories"]}, {"Idempotency-Key": "category-merge"})
            check(category_groups + "/" + category_group, category_groups_template + "/{id}", "DELETE", {"replacement_id": None}, {"Idempotency-Key": "category-group-delete"})
            entries = f"{budgets}/{budget_id}/transactions"
            entries_template = budgets + "/{budget}/transactions"
            transaction = {
                "id": str(uuid.uuid4()), "account_id": account_id, "date": "2026-10-16",
                "amount": -123, "cleared": True, "notes": "Contract expense",
                "payee_id": references["payees"], "category_id": references["categories"],
            }
            miss = check(entries, entries_template, "POST", transaction, {"Idempotency-Key": "entry", "X-Actual-Receipt-Only": "true"}, 404)
            assert miss["error"]["code"] == "operation_uncommitted"
            created = check(entries, entries_template, "POST", transaction, {"Idempotency-Key": "entry"}, 201)
            assert check(entries, entries_template, "POST", transaction, {"Idempotency-Key": "entry", "X-Actual-Receipt-Only": "true"}, 201) == created
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
            check(preferences, preferences_template, "PATCH", {"number_format": "dot-comma"}, {"Idempotency-Key": "preferences"})
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
            check(locations, location_template, "POST", {**location,"id":str(uuid.uuid4())}, {"Idempotency-Key":str(uuid.uuid4())}, 201)
            check(changes_path + "?cursor=" + page["snapshot"], changes_template)
            check(ordered_path, entry_template, "PATCH", {"date": "2026-10-16"},
                  {"Idempotency-Key": "ordered-new", "X-Actual-Edit-Clock": clock2})
            check(ordered_path, entry_template, "PATCH", {"date": "2026-10-17", "amount": -10},
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
                  {"Idempotency-Key": "activate"}, 409)
            check(imports + "/actual", imports + "/actual", "POST", b"invalid zip",
                  {"Content-Type": "application/zip", "Idempotency-Key": "invalid-stage"}, 422)
            templated = check(imports + "/actual", imports + "/actual", "POST",
                            (project / "tests/fixtures/rules-template-requirement-26.10.0.zip").read_bytes(),
                            {"Content-Type": "application/zip", "Idempotency-Key": "template-stage"}, 201)["data"]
            assert templated["status"] == "ready"
            check(imports + "/" + templated["id"] + "/activate", imports + "/{id}/activate", "POST", {},
                  {"Idempotency-Key": "template-activate"}, 409)

            for path in ["/api/v1/themes/catalog", "/api/v1/themes/compile", "/api/v1/themes/convert-actual"]:
                assert path not in contract["paths"]
                assert request(base, path, "POST", {})[0] == 404
            root = f"{budgets}/{budget_id}"
            root_template = budgets + "/{budget}"
            metadata = check(root, root_template)["data"]
            checkpoint = check(root + "/checkpoint", root_template + "/checkpoint")
            clock_checkpoint = check(root + "/checkpoint?include_clock=true", root_template + "/checkpoint")
            assert set(clock_checkpoint["data"]) == set(checkpoint["data"]) | {"edit_clock"}
            assert {k: v for k, v in clock_checkpoint["data"].items() if k != "edit_clock"} == checkpoint["data"]
            assert check(root + "/checkpoint", root_template + "/checkpoint") == checkpoint
            epoch = metadata["epoch"]
            point = check(root + "/backups", root_template + "/backups", "POST", {"label": "Contract backup"},
                          {"Idempotency-Key": "backup", "X-Actual-Budget-Epoch": epoch}, 201)["data"]
            check(root + "/backups", root_template + "/backups")
            download_path = root + "/backups/" + point["id"] + "/download"
            assert "application/zip" in contract["paths"][root_template + "/backups/{id}/download"]["get"]["responses"]["200"]["content"]
            with urllib.request.urlopen(base + download_path) as response:
                assert response.headers.get_content_type() == "application/zip"
                archive = response.read()
            canceled = check(root + "/backups", root_template + "/backups", "POST", {"label":"Never saved"},
                             {"Idempotency-Key":"cancel-backup","X-Actual-Budget-Epoch":epoch,"X-Actual-Cancel-If-Uncommitted":"true"},409)
            assert canceled["error"]["code"] == "operation_canceled"
            restore_path = root + "/backups/" + point["id"] + "/restore"
            restored = check(restore_path, root_template + "/backups/{id}/restore", "POST", {"confirm_name": "Contract fixture"},
                             {"Idempotency-Key": "restore", "X-Actual-Budget-Epoch": epoch})["data"]["budget"]
            check(restore_path, root_template + "/backups/{id}/restore", "POST", {"confirm_name":"Contract fixture"},
                  {"Idempotency-Key":"stale-restore","X-Actual-Budget-Epoch":epoch},409)
            check("/api/v1/budget", "/api/v1/budget")
            recovered = check(restore_path, root_template + "/backups/{id}/restore", "POST", {"confirm_name": "Contract fixture"},
                              {"Idempotency-Key": "restore-again", "X-Actual-Budget-Epoch": restored["epoch"]})["data"]["budget"]
            stage = check("/api/v1/restores", "/api/v1/restores", "POST", archive,
                          {"Content-Type": "application/zip", "Idempotency-Key": "native-stage"}, 201)["data"]
            check("/api/v1/restores/" + stage["id"], "/api/v1/restores/{id}")
            check(root + "/restores/" + stage["id"] + "/activate", root_template + "/restores/{id}/activate", "POST",
                  {"confirm_name": "Contract fixture"}, {"Idempotency-Key": "native-activate", "X-Actual-Budget-Epoch": recovered["epoch"]})
            check("/api/v1/restores", "/api/v1/restores", "POST", b"invalid ZIP",
                  {"Content-Type": "application/zip", "Idempotency-Key": "invalid-native-stage"}, 422)
            transfer_budget, cash, card = (str(uuid.uuid4()) for _ in range(3))
            fresh_installation()
            check(budgets, budgets, "POST", {"id": transfer_budget, "name": "Transfer contracts"}, {"Idempotency-Key": "transfer-budget"}, 201)
            transfer_root = f"{budgets}/{transfer_budget}"
            for identity, name in [(cash,"Cash"),(card,"Card")]:
                check(transfer_root+"/accounts", account_template, "POST", {"id": identity,"name":name,"opening_balance":0,"opening_date":"2026-01-01"}, {"Idempotency-Key":identity}, 201)
            first, second = str(uuid.uuid4()), str(uuid.uuid4())
            pair = {"id":first,"counterpart_id":second,"account_id":cash,"to_account_id":card,"amount":-100,"date":"2026-01-01"}
            check(transfer_root+"/transfers", root_template+"/transfers", "POST", pair, {"Idempotency-Key":"pair"}, 201)
            check(transfer_root+"/transactions/"+first, entries_template+"/{id}", "PATCH", {"date":"2026-02-01","sync_transfer_date":True}, {"Idempotency-Key":"pair-date"})
            check(transfer_root+"/transactions/"+first, entries_template+"/{id}", "DELETE", None, {"Idempotency-Key":"pair-delete"})
            first, second = str(uuid.uuid4()), str(uuid.uuid4())
            for identity, account, amount in [(first,cash,-100),(second,card,100)]:
                check(transfer_root+"/transactions", entries_template, "POST", {"id":identity,"account_id":account,"amount":amount,"date":"2026-01-01"}, {"Idempotency-Key":identity}, 201)
            check(transfer_root+"/transfers/link", root_template+"/transfers/link", "POST", {"id":first,"counterpart_id":second}, {"Idempotency-Key":"pair-link"})
            check(transfer_root+"/accounts/"+cash+"/close", account_template+"/{id}/close", "POST", {"id":str(uuid.uuid4()),"counterpart_id":str(uuid.uuid4()),"to_account_id":card,"date":"2026-02-01","expected_balance":-100}, {"Idempotency-Key":"transfer-close"})
            parent, child, peer, sibling = (str(uuid.uuid4()) for _ in range(4))
            check(transfer_root+"/transactions", entries_template, "POST", {"id":parent,"account_id":card,"amount":-1000,"date":"2026-01-01"}, {"Idempotency-Key":"split-transfer-parent"},201)
            # Reopen the source account for a child transfer destination.
            check(transfer_root+"/accounts/"+cash+"/reopen",account_template+"/{id}/reopen","POST",{}, {"Idempotency-Key":"reopen-for-split"})
            split_path=transfer_root+"/transactions/"+parent+"/splits"
            split_template=entries_template+"/{id}/splits"
            snapshot=check(split_path,split_template)["snapshot"]
            check(split_path,split_template,"POST",{"snapshot":snapshot,"children":[{"id":child,"amount":-600,"transfer_account_id":cash,"counterpart_id":peer},{"id":sibling,"amount":-400}]},{"Idempotency-Key":"combined-split"})
            check(transfer_root+"/transactions/"+peer,entries_template+"/{id}","PATCH",{"amount":700},{"Idempotency-Key":"combined-pair-patch"})
            check(transfer_root+"/transactions/"+peer,entries_template+"/{id}","PATCH",{"notes":"Queued expense"},{"Idempotency-Key":"expense-only","X-Actual-Expense-Only":"true"},409)
            check(transfer_root+"/transactions/"+peer,entries_template+"/{id}","PATCH",{"cleared":True},{"Idempotency-Key":"clear-protected-peer"})
            assert request(base, transfer_root+"/accounts/"+cash+"/reconciliation")[0] == 404
            proof=check(split_path,split_template)
            confirmed={"amount":750,"confirm_reconciled":True,"transfer_protection_token":proof["transfer_protection_token"]}
            Draft202012Validator({**contract["components"]["schemas"]["TransactionPatch"],"components":contract["components"]},format_checker=FormatChecker()).validate(confirmed)
            check(transfer_root+"/transactions/"+peer,entries_template+"/{id}","PATCH",confirmed,{"Idempotency-Key":"confirmed-pair"})
            history_owner=str(uuid.uuid4())
            owned={"Idempotency-Key":"owned-confirmed-pair","X-Actual-History-Owner":history_owner}
            proof=check(split_path,split_template)
            confirmed={"amount":775,"confirm_reconciled":True,"transfer_protection_token":proof["transfer_protection_token"]}
            check(transfer_root+"/transactions/"+peer,entries_template+"/{id}","PATCH",confirmed,owned)
            history_path=transfer_root+"/history"
            history_template=root_template+"/history"
            history=check(history_path,history_template,headers={"X-Actual-History-Owner":history_owner})["data"]
            check(history_path+"/undo",history_template+"/{direction}","POST",{"action_id":history["undo"]["id"]},{"Idempotency-Key":"owned-undo","X-Actual-History-Owner":history_owner})
            history=check(history_path,history_template,headers={"X-Actual-History-Owner":history_owner})["data"]
            check(history_path+"/redo",history_template+"/{direction}","POST",{"action_id":history["redo"]["id"]},{"Idempotency-Key":"owned-redo","X-Actual-History-Owner":history_owner})
            # Rules use a separate disposable budget, preserving earlier financial examples.
            rb, ra, rp, rc, rid, re = (str(uuid.uuid4()) for _ in range(6))
            rr=f"{budgets}/{rb}"
            rt=budgets+"/{budget}/rules"
            fresh_installation()
            check(budgets,budgets,"POST",{"id":rb,"name":"Rule contract"},{"Idempotency-Key":"rule-budget"},201)
            check(rr+"/accounts",account_template,"POST",{"id":ra,"name":"Rule cash","opening_balance":0,"opening_date":"2026-01-01"},{"Idempotency-Key":"rule-account"},201)
            check(rr+"/payees",budgets+"/{budget}/payees","POST",{"id":rp,"name":"Rule shop"},{"Idempotency-Key":"rule-payee"},201)
            check(rr+"/categories",categories_template,"POST",{"id":rc,"name":"Rule food","is_income":False},{"Idempotency-Key":"rule-category"},201)
            rule={"id":rid,"stage":None,"conditionsOp":"and","conditions":[{"field":"payee","op":"is","value":rp}],"actions":[{"op":"set","field":"category","value":rc}]}
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules",rt,"POST",{**rule,"snapshot":snapshot},{"Idempotency-Key":"create-rule"},201)
            check(rr+"/rules/"+rid,rt+"/{id}")
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/"+rid,rt+"/{id}","PATCH",{"snapshot":snapshot,"actions":[{"op":"set","field":"category","value":rc},{"op":"set","field":"date","value":"2026-01-02"}]},{"Idempotency-Key":"edit-rule"})
            snapshot=check(rr+"/rules",rt)["snapshot"]
            transaction={"id":re,"account_id":ra,"date":"2026-01-01","amount":-100,"payee_id":rp,"notes":"Manual"}
            check(rr+"/rules/preview",rt+"/preview","POST",{"snapshot":snapshot,"transaction":transaction,"manual_entry":True})
            calculated = {**rule, "actions": [
                {"op":"set","field":"amount","value":0,"options":{"template":"-12.5 pounds"}},
                {"op":"set","field":"amount","value":0,"options":{"formula":"=1/0"}}]}
            check(rr+"/rules/preview",rt+"/preview","POST",{
                "snapshot":snapshot,"transaction":transaction,"rule":calculated}, expected=400)
            check(rr+"/rules/preview",rt+"/preview","POST",{
                "snapshot":snapshot,"transaction":transaction,"rule":{**rule,"actions":[
                    {"op":"set-split-amount","value":None,"options":{"method":"formula","splitIndex":1,"formula":"=parent_amount/200"}}]}}, expected=400)
            check(rr+"/rules/transactions",rt+"/transactions","POST",{**transaction,"snapshot":snapshot},{"Idempotency-Key":"automatic-entry"},201)
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/preview",rt+"/preview","POST",{"snapshot":snapshot,"ids":[re]})
            check(rr+"/rules/apply",rt+"/apply","POST",{"snapshot":snapshot,"ids":[re]},{"Idempotency-Key":"rule-apply"})
            check(rr+"/rules/associations",rt+"/associations")
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/learning",rt+"/learning","POST",{"snapshot":snapshot,"enabled":False},{"Idempotency-Key":"rule-learning"})
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/rename-payee",rt+"/rename-payee","POST",{"snapshot":snapshot,"payee_id":rp,"imported_payees":["RAW SHOP"]},{"Idempotency-Key":"rule-rename"})
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/"+rid,rt+"/{id}","DELETE",{"snapshot":snapshot},{"Idempotency-Key":"delete-rule"})

            # Schedule requests and actual responses share one served contract.
            # Independent financial/calendar parity is qualified by source fixtures.
            schedule_path = rr + "/schedules"
            schedule_template = budgets + "/{budget}/schedules"
            command_path = schedule_path + "/commands"
            command_template = schedule_template + "/commands"
            schedule_schema = contract["paths"][command_template]["post"]["requestBody"]["content"]["application/json"]["schema"]
            schedule_validator = Draft202012Validator(
                {**schedule_schema, "components": contract["components"]}, format_checker=FormatChecker())

            def schedule_command(payload, key, expected=200, extra_headers=None):
                schedule_validator.validate(payload)
                return check(command_path, command_template, "POST", payload,
                             {"Idempotency-Key": key, "X-Actual-Budget-Epoch": schedule_epoch,
                              **(extra_headers or {})}, expected)

            schedule_due = check(schedule_path + "/due", schedule_template + "/due")["data"]
            schedule_epoch = schedule_due["epoch"]
            schedule_today = schedule_due["today"]
            schedule_cursor = schedule_due["snapshot"]
            preview_path = "/api/v1/schedules/preview-dates"
            preview_body = {"config": {"start": schedule_today, "frequency": "weekly", "endMode": "after_n_occurrences", "endOccurrences": 2},
                            "from": schedule_today, "count": 2, "upcoming_length": "oneMonth"}
            preview_schema = contract["paths"][preview_path]["post"]["requestBody"]["content"]["application/json"]["schema"]
            Draft202012Validator({**preview_schema, "components": contract["components"]},
                                 format_checker=FormatChecker()).validate(preview_body)
            check(preview_path, preview_path, "POST", preview_body)
            check(preview_path, preview_path, "POST", {**preview_body, "count": 0}, expected=400)
            sid = str(uuid.uuid4())
            schedule_create = {"action": "create", "id": sid, "snapshot": schedule_cursor,
                               "name": "Contract schedule", "posts_transaction": False,
                               "conditions": [{"field": "account", "op": "is", "value": ra},
                                              {"field": "amount", "op": "is", "value": -100},
                                              {"field": "date", "op": "is", "value": schedule_today}]}
            assert not schedule_validator.is_valid({**schedule_create, "unexpected": True})
            assert not schedule_validator.is_valid({k: v for k, v in schedule_create.items() if k != "snapshot"})
            schedule_command(schedule_create, "schedule-create", 404, {"X-Actual-Receipt-Only": "true"})
            created = schedule_command(schedule_create, "schedule-create", 201)
            assert schedule_command(schedule_create, "schedule-create", 201) == created
            schedule_page = check(schedule_path + "?limit=1&offset=0&account_id=" + ra, schedule_template)["data"]
            occurrence_path = schedule_path + "/" + sid + "/occurrences"
            occurrence_template = schedule_template + "/{id}/occurrences"
            check(occurrence_path + "?" + urllib.parse.urlencode({"from": schedule_today, "through": schedule_today,
                                                               "limit": 1, "snapshot": schedule_page["snapshot"]}), occurrence_template)
            schedule_command({"action": "update", "id": sid, "snapshot": schedule_page["snapshot"],
                              "custom_upcoming_length": "2-week", "name": "Edited schedule"}, "schedule-update")
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "skip", "id": sid, "snapshot": schedule_page["snapshot"],
                              "occurrence_date": schedule_today}, "schedule-skip")
            schedule_page = check(schedule_path, schedule_template)["data"]
            posted = schedule_command({"action": "post", "id": sid, "snapshot": schedule_page["snapshot"],
                                       "occurrence_date": schedule_today}, "schedule-post")
            assert posted["data"]["transaction"]["schedule_id"] == sid
            occurrences = check(occurrence_path + "?from=" + schedule_today + "&through=" + schedule_today, occurrence_template)["data"]
            assert occurrences["occurrences"][0]["status"] == "posted"
            assert occurrences["occurrences"][0]["transaction_id"] == posted["data"]["transaction"]["id"]
            check(occurrence_path + "?limit=0", occurrence_template, expected=400)
            check(rr + "/transactions?account_id=" + ra, budgets + "/{budget}/transactions")
            schedule_command({"action": "catch-up", "through": schedule_today}, "schedule-catch-up")
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "settings", "snapshot": schedule_page["snapshot"],
                              "upcoming_length": "14"}, "schedule-settings")
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "update", "id": sid, "snapshot": schedule_page["snapshot"],
                              "use_global_upcoming_length": True, "completed": True}, "schedule-complete")
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "delete", "id": sid, "snapshot": schedule_page["snapshot"]}, "schedule-delete")
            check(rr + "/changes?cursor=" + schedule_cursor, budgets + "/{budget}/changes")
            check(schedule_path + "?limit=0", schedule_template, expected=400)
            check(schedule_path + "?snapshot=" + schedule_cursor, schedule_template, expected=409)
            check(budgets + "/" + str(uuid.uuid4()) + "/schedules/due", schedule_template + "/due", expected=404)
            canceled = schedule_command({"action": "catch-up", "through": schedule_today}, "schedule-cancel", 409,
                                        {"X-Actual-Cancel-If-Uncommitted": "true"})
            assert canceled["error"]["code"] == "operation_canceled"
            assert schedule_command({"action": "catch-up", "through": schedule_today}, "schedule-cancel", 409,
                                    {"X-Actual-Receipt-Only": "true"}) == canceled
            # First transfer schedule creates its managed payee without a dummy transfer.
            schedule_peer, transfer_schedule = str(uuid.uuid4()), str(uuid.uuid4())
            check(rr + "/accounts", account_template, "POST",
                  {"id": schedule_peer, "name": "Schedule destination", "opening_balance": 0, "opening_date": schedule_today},
                  {"Idempotency-Key": "schedule-destination"}, 201)
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "create", "id": transfer_schedule, "snapshot": schedule_page["snapshot"],
                              "transfer_account_id": schedule_peer,
                              "conditions": [{"field": "account", "op": "is", "value": ra},
                                             {"field": "amount", "op": "is", "value": -25},
                                             {"field": "date", "op": "is", "value": schedule_today}]}, "schedule-transfer-create", 201)
            schedule_page = check(schedule_path, schedule_template)["data"]
            schedule_command({"action": "update", "id": transfer_schedule, "snapshot": schedule_page["snapshot"],
                              "transfer_account_id": schedule_peer, "name": "Scheduled transfer"}, "schedule-transfer-update")
            schedule_page = check(schedule_path, schedule_template)["data"]
            transferred = schedule_command({"action": "post", "id": transfer_schedule, "snapshot": schedule_page["snapshot"],
                                            "occurrence_date": schedule_today}, "schedule-transfer-post")
            counterpart = transferred["data"]["transaction"]["transfer_id"]
            assert counterpart is not None
            check(rr + "/transactions/" + counterpart, budgets + "/{budget}/transactions/{id}")
            check(rr + "/transactions?account_id=" + schedule_peer, budgets + "/{budget}/transactions")

            match_template = schedule_template + "/matches"
            match_path = schedule_path + "/matches"
            match_schema = contract["paths"][match_template]["post"]["requestBody"]["content"]["application/json"]["schema"]
            match_validator = Draft202012Validator(
                {**match_schema, "components": contract["components"]}, format_checker=FormatChecker())
            match_checkpoint = check(rr + "/checkpoint", budgets + "/{budget}/checkpoint")
            match_body = {"snapshot": match_checkpoint["data"]["snapshot"],
                          "conditions": [{"field": "account", "op": "is", "value": ra},
                                         {"field": "amount", "op": "is", "value": -25}],
                          "limit": 1, "offset": 0}
            match_validator.validate(match_body)
            matched = check(match_path, match_template, "POST", match_body)["data"]
            assert transferred["data"]["transaction"]["id"] in [row["id"] for row in matched["transactions"]]
            assert matched["snapshot"] == match_body["snapshot"]
            assert check(rr + "/checkpoint", budgets + "/{budget}/checkpoint") == match_checkpoint
            for invalid in ({**match_body, "limit": 0}, {**match_body, "limit": 1001},
                            {**match_body, "offset": -1}, {**match_body, "snapshot": None},
                            {**match_body, "conditions": []}):
                assert not match_validator.is_valid(invalid)
                check(match_path, match_template, "POST", invalid, expected=400)
            check(match_path, match_template, "POST", {**match_body, "snapshot": schedule_cursor}, expected=409)
            assert check(rr + "/checkpoint", budgets + "/{budget}/checkpoint") == match_checkpoint

            # Link mutations keep transfer pairs together and replay their original result.
            transfer_id = transferred["data"]["transaction"]["id"]
            for action in ("unlink", "link"):
                schedule_page = check(schedule_path, schedule_template)["data"]
                link_body = {"action": action, "id": transfer_schedule,
                             "snapshot": schedule_page["snapshot"], "transaction_ids": [transfer_id]}
                assert not schedule_validator.is_valid({**link_body, "transaction_ids": []})
                outcome = schedule_command(link_body, "schedule-" + action)
                assert set(outcome["data"]["transaction_ids"]) == {transfer_id, counterpart}
                assert outcome["data"]["affected_count"] == 2
                assert schedule_command(link_body, "schedule-" + action, extra_headers={"X-Actual-Receipt-Only": "true"}) == outcome
                for transaction_id in (transfer_id, counterpart):
                    row = check(rr + "/transactions/" + transaction_id, budgets + "/{budget}/transactions/{id}")["data"]
                    assert row.get("schedule_id") == (transfer_schedule if action == "link" else None)
                check(command_path, command_template, "POST",
                      {**link_body, "snapshot": outcome["snapshot"], "transaction_ids": []},
                      {"Idempotency-Key": "empty-schedule-" + action}, expected=400)

            # Discover actual history, then accept and link it in one recorded command.
            discovery_payee = str(uuid.uuid4())
            check(rr + "/payees", budgets + "/{budget}/payees", "POST",
                  {"id": discovery_payee, "name": "Contract weekly bill"},
                  {"Idempotency-Key": "discovery-payee"}, 201)
            history_ids = []
            for date in ("2026-10-12", "2026-10-19", "2026-10-26", "2026-11-02"):
                history_id = str(uuid.uuid4())
                history_ids.append(history_id)
                check(rr + "/transactions", budgets + "/{budget}/transactions", "POST",
                      {"id": history_id, "account_id": ra, "payee_id": discovery_payee,
                       "amount": -765, "date": date}, {"Idempotency-Key": "discovery-" + date}, 201)
            discovery_checkpoint = check(rr + "/checkpoint", budgets + "/{budget}/checkpoint")
            discovery_template = schedule_template + "/discovery"
            discovery_query = {"snapshot": discovery_checkpoint["data"]["snapshot"],
                               "as_of": "2026-11-09", "account_id": ra, "offset": 0, "limit": 100}
            discovery_path = schedule_path + "/discovery"
            discovered = check(discovery_path + "?" + urllib.parse.urlencode(discovery_query), discovery_template)["data"]
            suggestion = next(row for row in discovered["suggestions"] if row["payee_id"] == discovery_payee)
            assert suggestion["amount"] == -765 and suggestion["date"]["frequency"] == "weekly"
            assert discovered["as_of"] == "2026-11-09"
            assert check(rr + "/checkpoint", budgets + "/{budget}/checkpoint") == discovery_checkpoint
            for invalid in ({**discovery_query, "limit": 0}, {**discovery_query, "as_of": "invalid"},
                            {key: value for key, value in discovery_query.items() if key != "snapshot"}):
                check(discovery_path + "?" + urllib.parse.urlencode(invalid), discovery_template, expected=400)
            discovered_schedule = str(uuid.uuid4())
            accept_body = {"action": "create", "id": discovered_schedule,
                           "snapshot": discovery_checkpoint["data"]["snapshot"],
                           "conditions": suggestion["conditions"], "posts_transaction": False,
                           "transaction_ids": history_ids}
            accepted = schedule_command(accept_body, "accept-discovery", 201)
            assert schedule_command(accept_body, "accept-discovery", 201,
                                    {"X-Actual-Receipt-Only": "true"}) == accepted
            linked_template = schedule_template + "/{id}/transactions"
            linked_path = schedule_path + "/" + discovered_schedule + "/transactions"
            linked = check(linked_path + "?" + urllib.parse.urlencode({"snapshot": accepted["snapshot"], "limit": 1}), linked_template)["data"]
            assert linked["total_count"] == 4 and linked["next_offset"] == 1
            assert linked["transactions"][0]["id"] in history_ids
            assert linked["transactions"][0]["schedule_id"] == discovered_schedule
            check(linked_path, linked_template, expected=400)
            check(discovery_path + "?" + urllib.parse.urlencode(discovery_query), discovery_template, expected=409)

            conversion_template = schedule_template + "/convert-preview"
            conversion_path = schedule_path + "/convert-preview"
            conversion_schema = contract["paths"][conversion_template]["post"]["requestBody"]["content"]["application/json"]["schema"]
            conversion_validator = Draft202012Validator(
                {**conversion_schema, "components": contract["components"]}, format_checker=FormatChecker())
            future_date = (datetime.date.fromisoformat(schedule_today) + datetime.timedelta(days=8)).isoformat()
            future_draft = {"id": str(uuid.uuid4()), "account_id": ra, "amount": -333,
                            "date": future_date}
            conversion_checkpoint = check(rr + "/checkpoint", budgets + "/{budget}/checkpoint")
            preview_body = {"snapshot": conversion_checkpoint["data"]["snapshot"], "transaction": future_draft,
                            "as_of": schedule_today, "upcoming_length": "7"}
            conversion_validator.validate(preview_body)
            preview = check(conversion_path, conversion_template, "POST", preview_body)["data"]
            assert preview["is_future"] and preview["is_beyond_window"]
            assert preview["days_until_transaction"] == 8 and preview["upcoming_days"] == 7
            assert preview["affected_transactions"] == [] and preview["protected_count"] == 0
            assert check(rr + "/checkpoint", budgets + "/{budget}/checkpoint") == conversion_checkpoint
            for invalid in ({"snapshot": preview_body["snapshot"]},
                            {**preview_body, "transaction_id": future_draft["id"]}):
                assert not conversion_validator.is_valid(invalid)
                check(conversion_path, conversion_template, "POST", invalid, expected=400)
            convert_body = {"action": "convert", "id": str(uuid.uuid4()),
                            "snapshot": preview_body["snapshot"], "transaction": future_draft}
            converted = schedule_command(convert_body, "convert-future-draft", 201)
            assert schedule_command(convert_body, "convert-future-draft", 201,
                                    extra_headers={"X-Actual-Receipt-Only": "true"}) == converted
            assert converted["data"]["days_until_transaction"] == 8
            after_conversion = check(rr + "/checkpoint", budgets + "/{budget}/checkpoint")
            check(rr + "/transactions", budgets + "/{budget}/transactions", "POST", future_draft,
                  {"Idempotency-Key": "late-original-draft-save"}, expected=409)
            assert check(rr + "/checkpoint", budgets + "/{budget}/checkpoint") == after_conversion
            assert future_draft["id"] not in [row["id"] for row in check(rr + "/transactions", budgets + "/{budget}/transactions")["data"]]
            existing_future = {**future_draft, "id": str(uuid.uuid4()), "amount": -444}
            check(rr + "/transactions", budgets + "/{budget}/transactions", "POST", existing_future,
                  {"Idempotency-Key": "existing-future-expense"}, 201)
            existing_checkpoint = check(rr + "/checkpoint", budgets + "/{budget}/checkpoint")
            existing_preview = {"snapshot": existing_checkpoint["data"]["snapshot"], "transaction_id": existing_future["id"]}
            conversion_validator.validate(existing_preview)
            existing_preview_result = check(conversion_path, conversion_template, "POST", existing_preview)["data"]
            assert existing_future["id"] in [row["id"] for row in existing_preview_result["affected_transactions"]]
            existing_conversion = schedule_command({"action": "convert", "id": str(uuid.uuid4()), **existing_preview},
                                                   "convert-existing-future", 201)
            assert existing_future["id"] in existing_conversion["data"]["removed_transaction_ids"]

            # Genuine released Android work validates the public handoff contract,
            # including dense order keys returned by the ordinary companion view.
            fresh_installation()
            legacy_dir = project / "tests/fixtures/android-upgrade"
            legacy = json.loads((legacy_dir / "pending.state.json").read_text())
            staged = check(imports + "/actual", imports + "/actual", "POST",
                           (legacy_dir / "pending.zip").read_bytes(),
                           {"Idempotency-Key": "android-source", "Content-Type": "application/zip"}, 201)["data"]
            check(imports + "/" + staged["id"] + "/activate", imports + "/{id}/activate", "POST", {},
                  {"Idempotency-Key": "android-activate"}, 201)
            discovery = "/api/v1/android-upgrade/sources"
            source = {"file_id": legacy["budget"]["fileId"], "group_id": legacy["budget"]["groupId"]}
            candidates = check(discovery + "?" + urllib.parse.urlencode(source), discovery)["data"]
            candidate = next(item for item in candidates if item["id"] == staged["budget_id"])
            check(discovery + "?file_id=invalid&group_id=" + source["group_id"], discovery, expected=400)
            upgrade_root = budgets + "/" + candidate["id"]
            upgrade_path = upgrade_root + "/android-upgrade/operations"
            upgrade_template = budgets + "/{budget}/android-upgrade/operations"
            body_schema = contract["paths"][upgrade_template]["post"]["requestBody"]["content"]["application/json"]["schema"]
            body_validator = Draft202012Validator({**body_schema, "components": contract["components"]}, format_checker=FormatChecker())
            for operation in legacy["pending"]:
                handoff = {"source": source, "epoch": candidate["epoch"], "operation": operation}
                body_validator.validate(handoff)
                headers = {"Idempotency-Key": operation["id"]}
                check(upgrade_path, upgrade_template, "POST", handoff,
                      {**headers, "X-Actual-Receipt-Only": "true"}, 404)
                outcome = check(upgrade_path, upgrade_template, "POST", handoff, headers)
                replay = check(upgrade_path, upgrade_template, "POST", handoff,
                               {**headers, "X-Actual-Receipt-Only": "true"})
                assert replay == outcome, "Original operation receipt changed"
            check(upgrade_root + "/companion?account_id=" + legacy["accountId"], budgets + "/{budget}/companion")
            check(upgrade_path, upgrade_template, "POST", handoff,
                  {**headers, "X-Actual-Edit-Clock": "2026-11-08T23:50:10.000Z-0000-bbbbbbbbbbbbbbbb"}, 400)
            # Retired envelope/template routes stay unavailable; tracking allocation
            # contracts and their financial parity remain exercised independently.
            for suffix in ["/budget/templates", "/budget/templates/parse", "/budget/templates/projection",
                           "/budget/templates/preview", "/budget/templates/commands", "/budget/templates/migration-preview"]:
                assert budgets + "/{budget}" + suffix not in contract["paths"]
                assert request(base, rr + suffix, "POST", {})[0] == 404
            # Isolated import fixture: every expectation crosses the served public contract.
            import_id, import_account = str(uuid.uuid4()), str(uuid.uuid4())
            import_root = budgets + '/' + import_id
            fresh_installation()
            check(budgets, budgets, 'POST', {'id': import_id, 'name': 'Import contract', 'budget_type': 'tracking'}, {'Idempotency-Key': 'import-budget'}, 201)
            check(import_root + '/accounts', account_template, 'POST', {'id': import_account, 'name': 'Bank', 'opening_balance': 0, 'opening_date': '2026-11-01'}, {'Idempotency-Key': 'import-account'}, 201)
            checkpoint_template = budgets + '/{budget}/checkpoint'
            transaction_template = budgets + '/{budget}/transactions'
            cp = check(import_root + '/checkpoint', checkpoint_template)['data']
            file_template = budgets + '/{budget}/transaction-imports/{id}'
            upload_template = budgets + '/{budget}/accounts/{account}/transaction-imports'
            csv = b'Date,Payee,Amount,Notes\n2026-11-09,Cafe,-12.34,Contract purchase\n'
            upload_path = import_root + '/accounts/' + import_account + '/transaction-imports?filename=contract.csv'
            uploaded = check(upload_path, upload_template, 'POST', csv, {'Content-Type': 'application/octet-stream', 'Idempotency-Key': 'import-upload'}, 201)
            assert check(upload_path, upload_template, 'POST', csv, {'Content-Type': 'application/octet-stream', 'Idempotency-Key': 'import-upload'}, 201) == uploaded
            retained = uploaded['data']
            import_path = import_root + '/transaction-imports/' + retained['id']
            assert check(import_path, file_template)['data'] == retained

            def validate_request(template, body):
                schema = contract['paths'][template]['post']['requestBody']['content']['application/json']['schema']
                Draft202012Validator({**schema, 'components': contract['components']}, format_checker=FormatChecker()).validate(body)

            def binary(path, template, media, body=None):
                if body is not None:
                    validate_request(template, body)
                req = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers={'Content-Type': 'application/json'}, method='GET' if body is None else 'POST')
                with urllib.request.urlopen(req, timeout=5) as response:
                    assert response.status == 200 and response.headers['Content-Type'].startswith(media)
                    assert response.headers['Cache-Control'] == 'no-store'
                    assert 'attachment;' in response.headers['Content-Disposition']
                    value = response.read()
                schema = contract['paths'][template]['get' if body is None else 'post']['responses']['200']['content'][media]['schema']
                assert schema['format'] == 'binary' and len(value) <= schema['maxLength']
                return value

            assert binary(import_path + '/original', file_template + '/original', 'application/octet-stream') == csv
            parsed = check(import_path + '/parse', file_template + '/parse', 'POST', {'options': {'has_header_row': True}})['data']
            assert parsed['format'] == 'csv' and parsed['errors'] == [] and parsed['transactions'][0]['Amount'] == '-12.34'
            parse_error = check(import_path + '/parse', file_template + '/parse', 'POST', {'options': {'delimiter': 'xx'}})['data']
            assert parse_error['errors'] and parse_error['transactions'] == [] and 'format' not in parse_error
            check(import_path + '/parse', file_template + '/parse', 'POST', {'unexpected': True}, expected=400)
            mapping = {'date': 'Date', 'payee': 'Payee', 'amount': 'Amount', 'notes': 'Notes'}
            review = {'snapshot': cp['snapshot'], 'mapping': mapping, 'options': {'has_header_row': True}, 'remember': True}
            validate_request(file_template + '/preview', review)
            preview = check(import_path + '/preview', file_template + '/preview', 'POST', review)['data']
            assert preview['errors'] == [] and len(preview['rows']) == 1
            assert preview['rows'][0]['transaction']['amount'] == -1234
            assert check(import_root + '/checkpoint', checkpoint_template)['data'] == cp
            commit = {**review, 'rows': [{'row': 0, 'action': 'auto'}]}
            validate_request(file_template + '/commit', commit)
            identity = {'Idempotency-Key': 'import-commit', 'X-Actual-Budget-Epoch': cp['epoch'], 'X-Actual-History-Owner': str(uuid.uuid4())}
            check(import_path + '/commit', file_template + '/commit', 'POST', commit, {**identity, 'X-Actual-Receipt-Only': 'true'}, 404)
            imported = check(import_path + '/commit', file_template + '/commit', 'POST', commit, identity)
            assert len(imported['data']['added']) == 1
            assert check(import_path + '/commit', file_template + '/commit', 'POST', commit, identity) == imported
            assert check(import_path + '/commit', file_template + '/commit', 'POST', commit, {**identity, 'X-Actual-Receipt-Only': 'true'}) == imported
            saved = check(import_root + '/transactions', transaction_template)
            assert saved['balances']['total'] == -1234
            settings_path = import_root + '/accounts/' + import_account + '/transaction-import-settings'
            settings_template = budgets + '/{budget}/accounts/{account}/transaction-import-settings'
            settings = check(settings_path, settings_template)['data']
            assert settings['mapping']['amount'] == 'Amount' and settings['field_versions']
            check(settings_path + '?format=bad', settings_template, expected=400)
            export_template = budgets + '/{budget}/transactions/export'
            export_body = {'snapshot': settings['snapshot'], 'account_id': import_account, 'layout': 'standard'}
            exported = binary(import_root + '/transactions/export', export_template, 'text/csv', export_body)
            assert b'Contract purchase' not in exported and b'-12.34' in exported
            assert binary(import_root + '/transactions/export', export_template, 'text/csv', {**export_body, 'query': {'search': 'No matching row'}}) == b''
            check(import_root + '/transactions/export', export_template, 'POST', {**export_body, 'layout': 'bad'}, expected=400)
            canceled_body = {**commit, 'snapshot': settings['snapshot']}
            cancel_identity = {**identity, 'Idempotency-Key': 'import-cancel'}
            canceled = check(import_path + '/commit', file_template + '/commit', 'POST', canceled_body, {**cancel_identity, 'X-Actual-Cancel-If-Uncommitted': 'true'}, 409)
            assert canceled['error']['code'] == 'operation_canceled'
            assert check(import_path + '/commit', file_template + '/commit', 'POST', canceled_body, cancel_identity, 409) == canceled
            check(import_path + '/preview', file_template + '/preview', 'POST', review, expected=409)
            assert check(import_root + '/transactions', transaction_template) == saved

            duplicate = {'id': str(uuid.uuid4()), 'account_id': import_account, 'date': '2026-11-09', 'amount': -1234, 'notes': 'Manual duplicate'}
            check(import_root + '/transactions', transaction_template, 'POST', duplicate, {'Idempotency-Key': 'merge-duplicate'}, 201)
            merge_cp = check(import_root + '/checkpoint', checkpoint_template)['data']
            merge_body = {'snapshot': merge_cp['snapshot'], 'ids': [imported['data']['added'][0], duplicate['id']]}
            merge_template = budgets + '/{budget}/transactions/merge'
            validate_request(merge_template + '/preview', merge_body)
            proposal = check(import_root + '/transactions/merge/preview', merge_template + '/preview', 'POST', merge_body)['data']
            assert len(proposal['before']) == 2 and len(proposal['after']) == 1
            assert check(import_root + '/checkpoint', checkpoint_template)['data'] == merge_cp
            check(import_root + '/transactions/merge/preview', merge_template + '/preview', 'POST', {**merge_body, 'ids': [duplicate['id'], duplicate['id']]}, expected=400)
            merge_identity = {'Idempotency-Key': 'merge-cancel', 'X-Actual-Budget-Epoch': merge_cp['epoch']}
            fenced = check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, {**merge_identity, 'X-Actual-Cancel-If-Uncommitted': 'true'}, 409)
            assert fenced['error']['code'] == 'operation_canceled'
            assert check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, merge_identity, 409) == fenced
            assert check(import_root + '/checkpoint', checkpoint_template)['data'] == merge_cp
            merge_identity['Idempotency-Key'] = 'merge-success'
            check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, {**merge_identity, 'X-Actual-Receipt-Only': 'true'}, 404)
            merged = check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, merge_identity)
            assert merged['data']['keeper_id'] == proposal['keeper_id']
            assert check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, merge_identity) == merged
            assert check(import_root + '/transactions/merge', merge_template, 'POST', merge_body, {**merge_identity, 'X-Actual-Receipt-Only': 'true'}) == merged
            final = check(import_root + '/transactions', transaction_template)
            assert final['balances']['total'] == -1234 and len(final['data']) == 1
            print('PASS: import parse/plan/commit/settings/original/export and merge served schemas, exact receipts, cancellation and financial totals')

            assert not any('/banking' in path for path in contract['paths'])
            assert request(base, import_root + '/banking')[0] == 404
            print('PASS: external-bank routes absent; manual imported ledger remains available')

            query_caps = check('/api/v1/query-capabilities', '/api/v1/query-capabilities')['data']
            assert 'budget_cells' in query_caps['tables'] and query_caps['max_page'] == 1000
            qid, qaccount = str(uuid.uuid4()), str(uuid.uuid4())
            qroot = budgets + '/' + qid
            fresh_installation()
            check(budgets, budgets, 'POST', {'id':qid,'name':'Remote automation contract'}, {'Idempotency-Key':'query-budget'}, 201)
            check(qroot+'/accounts',account_template,'POST',{'id':qaccount,'name':'Remote account','opening_balance':0,'opening_date':'2026-11-09'}, {'Idempotency-Key':'query-account'},201)
            epoch=request(base,qroot)[1]['data']['epoch']
            qpath=qroot+'/query';qtemplate=budgets+'/{budget}/query'
            query={'version':1,'table':'accounts','select':['id','name','closed'],'order_by':['id']}
            found=check(qpath,qtemplate,'POST',query)
            assert found['data']==[{'id':qaccount,'name':'Remote account','closed':False}]
            check(qpath,qtemplate,'POST',{**query,'table':'operations'},expected=400)
            check(qpath,qtemplate,'POST',{**query,'group_by':[{'$sum':'$id'}]},expected=400)
            check(qpath,qtemplate,'POST',query,{'X-Actual-Receipt-Only':'true'},400)
            cp=request(base,qroot+'/checkpoint')[1]['data']
            note_read=qroot+'/notes/read';note_read_template=budgets+'/{budget}/notes/read'
            note_path=qroot+'/notes/commands';note_template=budgets+'/{budget}/notes/commands'
            assert check(note_read,note_read_template,'POST',{'id':'contract:arbitrary-note'})['data'] is None
            note={'id':'contract:arbitrary-note','note':'Retained remote note','snapshot':cp['snapshot']}
            note_headers={'Idempotency-Key':'remote-note','X-Actual-Edit-Clock':datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')+'-0000-4747474747474747','X-Actual-Budget-Epoch':epoch,'X-Actual-History-Owner':str(uuid.uuid4())}
            saved_note=check(note_path,note_template,'POST',note,note_headers)
            assert saved_note['data']=={'id':note['id'],'note':note['note']}
            assert check(note_path,note_template,'POST',note,{**note_headers,'X-Actual-Receipt-Only':'true'})==saved_note
            check(note_path,note_template,'POST',note,{**note_headers,'X-Actual-Budget-Epoch':'0'*32},409)
            note_rows=check(qpath,qtemplate,'POST',{'version':1,'table':'notes','select':['id','note']})
            assert note_rows['data']==[saved_note['data']]
            cp=request(base,qroot+'/checkpoint')[1]['data']
            check(qpath,qtemplate,'POST',{**query,'snapshot':found['snapshot']},expected=409)
            batch_path=qroot+'/transactions/batch';batch_template=budgets+'/{budget}/transactions/batch'
            txid=str(uuid.uuid4())
            batch={'snapshot':cp['snapshot'],'max_graph_members':10,'operations':[{'action':'create','transaction':{'id':txid,'account_id':qaccount,'amount':-321,'date':'2026-11-09','notes':'Remote batch'}}]}
            batch_headers={**note_headers,'Idempotency-Key':'remote-batch','X-Actual-Edit-Clock':datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')+'-0001-4747474747474747'}
            saved_batch=check(batch_path,batch_template,'POST',batch,batch_headers)
            assert saved_batch['data']['operation_count']==1 and saved_batch['data']['results'][0]['id']==txid
            assert check(batch_path,batch_template,'POST',batch,{**batch_headers,'X-Actual-Receipt-Only':'true'})==saved_batch
            check(batch_path,batch_template,'POST',batch,{**batch_headers,'X-Actual-Budget-Epoch':'0'*32},409)
            check(batch_path,batch_template,'POST',{'snapshot':saved_batch['snapshot'],'operations':[]},{**batch_headers,'Idempotency-Key':'remote-empty'},400)
            check(batch_path,batch_template,'POST',batch,{**batch_headers,'Idempotency-Key':'remote-unknown','X-Actual-Receipt-Only':'true'},404)
            canceled=check(batch_path,batch_template,'POST',batch,{**batch_headers,'Idempotency-Key':'remote-cancel','X-Actual-Cancel-If-Uncommitted':'true'},409)
            assert canceled['error']['code']=='operation_canceled'
            assert check(batch_path,batch_template,'POST',batch,{**batch_headers,'Idempotency-Key':'remote-cancel'},409)==canceled
            result=check(qpath,qtemplate,'POST',{'version':1,'table':'transactions','select':[{'total':{'$sum':'$amount'}}],'calculation':True})
            assert result['data']==-321 and result['next_offset'] is None
            print('PASS: served query/catalog, arbitrary/domain notes, atomic batch, original clock/epoch/lookup/cancel and strict resource errors')

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
