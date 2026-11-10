# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Local disposable public Sankey/schema/current-native recovery qualification.
No database mutations or fabricated old-schema fixture. Archive versions are
observed from the produced public manifest, never relabeled as migration input.
"""
import hashlib, io, json, os, subprocess, tempfile, urllib.error, urllib.request, uuid, zipfile
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
project = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="sankey-native-") as directory:
    env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0", "ACTUALBUDGET_WEB_DIR": str(project / "web/dist")}
    env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
    process = subprocess.Popen([str(project / "server/target/debug/actualbudget-server")], env=env, stdout=subprocess.PIPE, text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        url = "http://" + line.removeprefix("LISTENING ")
        def call(path, body=None, headers=None, expected=200, method=None):
            raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
            request = urllib.request.Request(url + "/api/v1" + path, data=raw, headers={"Content-Type": "application/zip" if isinstance(body, bytes) else "application/json", "Idempotency-Key": str(uuid.uuid4()), **(headers or {})}, method=method)
            try:
                response = urllib.request.urlopen(request, timeout=20)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                value = json.load(response) if response.headers.get("Content-Type", "").startswith("application/json") else response.read()
                assert response.status == expected, (path, response.status, value)
                return value
        contract = call("/openapi.json")
        def check(value, suffix="", status=200, request=False):
            route = contract["paths"]["/api/v1/budgets/{budget}/reports/sankey" + suffix]["post"]
            schema = route["requestBody"]["content"]["application/json"]["schema"] if request else route["responses"][str(status)]["content"]["application/json"]["schema"]
            Draft202012Validator({**schema, "components": contract["components"]}, format_checker=FormatChecker()).validate(value)
        def portable(data):
            with zipfile.ZipFile(io.BytesIO(data)) as contents:
                manifest = json.loads(contents.read("manifest.json"))
                assert isinstance(manifest["schema_version"], int) and manifest["schema_version"] > 0
                return manifest["schema_version"], json.loads(contents.read("budget.json"))
        def clocks(value):
            table = value["tables"]["field_versions"]
            return [dict(zip(table["columns"], row)) for row in table["rows"] if dict(zip(table["columns"], row))["entity"] == "reporting_documents"]
        for method in ["envelope", "tracking"]:
            stem = "sankey-" + method + "-source-26.10"
            source = json.loads((project / "tests/fixtures" / (stem + ".json")).read_text())
            archive = (project / "tests/fixtures" / (stem + ".zip")).read_bytes()
            assert hashlib.sha256(archive).hexdigest() == source["archive_sha256"]
            stage = call("/imports/actual", archive, expected=201)["data"]
            assert stage["status"] in ["ready", "activated"]
            if stage["status"] == "ready": call("/imports/" + stage["id"] + "/activate", {}, expected=201)
            root = "/budgets/" + stage["budget_id"]
            state = call(root + "/reporting")["data"]
            widget = next(document for document in state["documents"] if document["id"] == source["widgets"][0]["id"])
            assert widget["config"]["meta"] == source["widgets"][0]["meta"]
            assert call(root + "/preferences")["data"]["experimental"]["sankeyReport"] is True
            query = {"version": 1, "as_of": source["clock"][:10], "options": source["cases"][0]["options"]}
            check(query, request=True)
            companion_path = root + "/companion?account_id=" + source["ids"]["checking"]
            companion = call(companion_path)
            before = call(root + "/reports/sankey", query)
            check(before)
            assert before["data"]["result"] == source["cases"][0]["result"]
            assert call(companion_path) == companion
            body = {"query": query, "snapshot": before["data"]["snapshot"], "link": {"source": source["ids"]["expenses"], "target": source["ids"]["food"]}, "limit": 2}
            check(body, "/drilldown", request=True)
            check(call(root + "/reports/sankey/drilldown", body), "/drilldown")
            check(call(root + "/reports/sankey", {**query, "version": 2}, expected=400), status=400)
            headers = {"X-Actual-Budget-Epoch": state["epoch"]}
            backup = call(root + "/backups", {"label": "Before Sankey native round trip"}, headers, 201)["data"]
            version, original = portable(call(root + "/backups/" + backup["id"] + "/download"))
            duplicate = str(uuid.uuid4())
            call(root + "/duplicate", {"id": duplicate, "name": "Sankey native duplicate"}, headers, 201)
            assert call("/budgets/" + duplicate + "/reporting")["data"]["documents"] == state["documents"]
            copied = call("/budgets/" + duplicate + "/reports/sankey", query)
            check(copied)
            assert copied["data"]["result"] == before["data"]["result"]
            call(root + "/backups/" + backup["id"] + "/restore", {"confirm_name": original["budget"]["name"]}, headers)
            restored = call(root + "/reporting")["data"]
            assert restored["epoch"] != state["epoch"] and restored["documents"] == state["documents"]
            after = call(root + "/reports/sankey", query)
            check(after)
            assert after["data"]["epoch"] == restored["epoch"] and not after["data"]["cache"]["hit"]
            assert after["data"]["result"] == before["data"]["result"]
            check(call(root + "/reports/sankey/drilldown", body, expected=409), "/drilldown", status=409)
            body["snapshot"] = after["data"]["snapshot"]
            check(call(root + "/reports/sankey/drilldown", body), "/drilldown")
            backup = call(root + "/backups", {"label": "After Sankey restore"}, {"X-Actual-Budget-Epoch": restored["epoch"]}, 201)["data"]
            current_version, current = portable(call(root + "/backups/" + backup["id"] + "/download"))
            assert current_version == version and clocks(current) == clocks(original)
            print(f"PASS {method} served Sankey/detail/error schemas, native{version} duplicate/restore, preserved settings/clocks, readonly companion and epoch cache invalidation")
        # Native money boundary; no claim that the pinned source accepts this larger range.
        for signed in [9007199254740991, -9007199254740991]:
            budget, account, group, category, transaction = [str(uuid.uuid4()) for _ in range(5)]
            call("/budgets", {"id": budget, "name": "Native Sankey money boundary", "budget_type": "tracking"}, expected=201)
            root = "/budgets/" + budget
            epoch = call(root + "/reporting")["data"]["epoch"]
            headers = {"X-Actual-Budget-Epoch": epoch}
            call(root + "/accounts", {"id": account, "name": "Money account", "opening_balance": 0, "opening_date": "2026-11-01"}, headers, 201)
            call(root + "/category-groups", {"id": group, "name": "Income group", "is_income": True}, headers, 201)
            call(root + "/categories", {"id": category, "name": "Exact income", "is_income": True, "group_id": group}, headers, 201)
            call(root + "/preferences", {"experimental": {"sankeyReport": True}}, headers, method="PATCH")
            call(root + "/transactions", {"id": transaction, "date": "2026-11-02", "amount": signed, "account_id": account, "category_id": category}, headers, 201)
            query = {"version": 1, "as_of": "2026-11-09", "options": {"startDate": "2026-11-01", "endDate": "2026-11-30", "mode": "budgeted"}}
            value = call(root + "/reports/sankey", query)
            check(value)
            edge = next(node for node in value["data"]["base_graph"] if node["key"] == category)["to"][0]
            assert edge == ["available_income", signed]
            body = {"query": query, "snapshot": value["data"]["snapshot"], "link": {"source": category, "target": "available_income"}}
            detail = call(root + "/reports/sankey/drilldown", body)
            check(detail, "/drilldown")
            assert detail["data"]["total_amount"] == signed
            assert detail["data"]["rows"][0]["original_amount"] == signed
            assert detail["data"]["rows"][0]["amount"] == signed
        print("PASS both native +/-MAX_MONEY Sankey projections, metadata, source rows and totals validate against served schemas")
    finally:
        process.terminate()
        process.wait(timeout=20)
