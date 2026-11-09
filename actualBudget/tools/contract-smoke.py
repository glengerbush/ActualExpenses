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
            reconcile_path = accounts + "/" + account_id + "/reconciliation"
            reconcile_template = account_template + "/{id}/reconciliation"
            progress = check(reconcile_path, reconcile_template)["data"]
            check(reconcile_path, reconcile_template, "POST", {"action":"lock", "statement_balance":12345, "snapshot":progress["snapshot"], "date":"2026-10-15"}, {"Idempotency-Key":"reconcile"})
            entry = check(f"{budgets}/{budget_id}/transactions?account_id={account_id}", budgets + "/{budget}/transactions")["data"][0]
            unlock_path = f"{budgets}/{budget_id}/transactions/{entry['id']}/unlock"
            protection = check(f"{budgets}/{budget_id}/transactions/{entry['id']}/splits", budgets + "/{budget}/transactions/{id}/splits")["reconciliation_token"]
            check(unlock_path, budgets + "/{budget}/transactions/{id}/unlock", "POST", {"reconciliation_token":protection}, {"Idempotency-Key":"unlock"})
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
            check(category_groups + "/" + category_group, category_groups_template + "/{id}", "PATCH", {"hidden": True}, {"Idempotency-Key": "hide-category-group"})
            check(categories, categories_template, "POST", {"id": extra_category, "name": "Merge source", "group_id": category_group, "sort_order": None}, {"Idempotency-Key": "extra-category"}, 201)
            check(categories + "/" + extra_category, categories_template + "/{id}")
            check(categories + "/" + extra_category, categories_template + "/{id}", "PATCH", {"notes": "**Keep** notes"}, {"Idempotency-Key": "category-notes"})
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
            check(locations, location_template, "POST", {**location,"id":str(uuid.uuid4())}, {"Idempotency-Key":str(uuid.uuid4())}, 201)
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
                            (project / "tests/fixtures/rules-template-requirement-26.10.0.zip").read_bytes(),
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
            root = f"{budgets}/{budget_id}"
            root_template = budgets + "/{budget}"
            metadata = check(root, root_template)["data"]
            check(root + "/checkpoint", root_template + "/checkpoint")
            epoch = metadata["epoch"]
            point = check(root + "/backups", root_template + "/backups", "POST", {"label": "Contract backup"},
                          {"Idempotency-Key": "backup", "X-Actual-Budget-Epoch": epoch}, 201)["data"]
            check(root + "/backups", root_template + "/backups")
            download_path = root + "/backups/" + point["id"] + "/download"
            assert "application/zip" in contract["paths"][root_template + "/backups/{id}/download"]["get"]["responses"]["200"]["content"]
            with urllib.request.urlopen(base + download_path) as response:
                assert response.headers.get_content_type() == "application/zip"
                archive = response.read()
            check(root, root_template, "PATCH", {"name": "Changed contract"},
                  {"Idempotency-Key": "rename", "X-Actual-Budget-Epoch": epoch})
            check(root + "/duplicate", root_template + "/duplicate", "POST", {"id": str(uuid.uuid4()), "name": "Contract copy"},
                  {"Idempotency-Key": "copy", "X-Actual-Budget-Epoch": epoch}, 201)
            canceled = check(root, root_template, "PATCH", {"name": "Never saved"},
                             {"Idempotency-Key": "cancel-rename", "X-Actual-Budget-Epoch": epoch,
                              "X-Actual-Cancel-If-Uncommitted": "true"}, 409)
            assert canceled["error"]["code"] == "operation_canceled"
            restore_path = root + "/backups/" + point["id"] + "/restore"
            restored = check(restore_path, root_template + "/backups/{id}/restore", "POST", {"confirm_name": "Changed contract"},
                             {"Idempotency-Key": "restore", "X-Actual-Budget-Epoch": epoch})["data"]["budget"]
            check(root, root_template, "PATCH", {"name": "Stale rename"},
                  {"Idempotency-Key": "stale-rename", "X-Actual-Budget-Epoch": epoch}, 409)
            check(root, root_template, "DELETE", {"confirm_name": "Contract fixture"},
                  {"Idempotency-Key": "delete-budget", "X-Actual-Budget-Epoch": restored["epoch"]})
            check(root, root_template, expected=410)
            check(root + "?include_deleted=true", root_template)
            check(budgets + "?include_deleted=true", budgets)
            recovered = check(restore_path, root_template + "/backups/{id}/restore", "POST", {"confirm_name": "Contract fixture"},
                              {"Idempotency-Key": "recover-deleted", "X-Actual-Budget-Epoch": restored["epoch"]})["data"]["budget"]
            stage = check("/api/v1/restores", "/api/v1/restores", "POST", archive,
                          {"Content-Type": "application/zip", "Idempotency-Key": "native-stage"}, 201)["data"]
            check("/api/v1/restores/" + stage["id"], "/api/v1/restores/{id}")
            check(root + "/restores/" + stage["id"] + "/activate", root_template + "/restores/{id}/activate", "POST",
                  {"confirm_name": "Contract fixture"}, {"Idempotency-Key": "native-activate", "X-Actual-Budget-Epoch": recovered["epoch"]})
            check("/api/v1/restores", "/api/v1/restores", "POST", b"invalid ZIP",
                  {"Content-Type": "application/zip", "Idempotency-Key": "invalid-native-stage"}, 422)
            transfer_budget, cash, card = (str(uuid.uuid4()) for _ in range(3))
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
            reconciliation=transfer_root+"/accounts/"+cash+"/reconciliation"
            progress=check(reconciliation,account_template+"/{id}/reconciliation")["data"]
            check(reconciliation,account_template+"/{id}/reconciliation","POST",{"action":"lock","statement_balance":progress["balances"]["cleared"],"snapshot":progress["snapshot"],"date":"2026-02-28"},{"Idempotency-Key":"lock-protected-peer"})
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
            check(budgets,budgets,"POST",{"id":rb,"name":"Rule contract"},{"Idempotency-Key":"rule-budget"},201)
            check(rr+"/accounts",account_template,"POST",{"id":ra,"name":"Rule cash","opening_balance":0,"opening_date":"2026-01-01"},{"Idempotency-Key":"rule-account"},201)
            check(rr+"/payees",budgets+"/{budget}/payees","POST",{"id":rp,"name":"Rule shop"},{"Idempotency-Key":"rule-payee"},201)
            check(rr+"/categories",categories_template,"POST",{"id":rc,"name":"Rule food","is_income":False},{"Idempotency-Key":"rule-category"},201)
            rule={"id":rid,"stage":None,"conditionsOp":"and","conditions":[{"field":"payee","op":"is","value":rp}],"actions":[{"op":"set","field":"category","value":rc}]}
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules",rt,"POST",{**rule,"snapshot":snapshot},{"Idempotency-Key":"create-rule"},201)
            check(rr+"/rules/"+rid,rt+"/{id}")
            snapshot=check(rr+"/rules",rt)["snapshot"]
            check(rr+"/rules/"+rid,rt+"/{id}","PATCH",{"snapshot":snapshot,"actions":[{"op":"set","field":"category","value":rc},{"op":"append-notes","value":" checked"}]},{"Idempotency-Key":"edit-rule"})
            snapshot=check(rr+"/rules",rt)["snapshot"]
            transaction={"id":re,"account_id":ra,"date":"2026-01-01","amount":-100,"payee_id":rp,"notes":"Manual"}
            check(rr+"/rules/preview",rt+"/preview","POST",{"snapshot":snapshot,"transaction":transaction,"manual_entry":True})
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

            # Genuine released Android work validates the public handoff contract,
            # including dense order keys returned by the ordinary companion view.
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
