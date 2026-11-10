# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Local disposable public analysis/schema/current-native recovery qualification.
No database mutations or fabricated old-schema fixture. Archive versions are
observed from the produced public manifest, never relabeled as migration input.
"""
import hashlib, io, json, os, subprocess, tempfile, urllib.error, urllib.request, uuid, zipfile
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
project = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="analysis-native-") as directory:
    env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0", "ACTUALBUDGET_WEB_DIR": str(project / "web/dist")}
    env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
    process = subprocess.Popen([str(project / "server/target/debug/actualbudget-server")], env=env, stdout=subprocess.PIPE, text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        url = "http://" + line.removeprefix("LISTENING ")
        def call(path, body=None, headers=None, expected=200):
            raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
            request = urllib.request.Request(url + "/api/v1" + path, data=raw, headers={"Content-Type": "application/zip" if isinstance(body, bytes) else "application/json", "Idempotency-Key": str(uuid.uuid4()), **(headers or {})})
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
            route = contract["paths"]["/api/v1/budgets/{budget}/reports/budget-analysis" + suffix]["post"]
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
            stem = "budget-analysis-" + method + "-source-26.10"
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
            assert call(root + "/preferences")["data"]["experimental"]["budgetAnalysisReport"] is True
            query = {"version": 1, "as_of": source["clock"][:10], "options": source["cases"][0]["options"]}
            check(query, request=True)
            companion_path = root + "/companion?account_id=" + source["ids"]["checking"]
            companion = call(companion_path)
            before = call(root + "/reports/budget-analysis", query)
            check(before)
            assert before["data"]["result"] == source["cases"][0]["expected"]
            assert call(companion_path) == companion
            for metric in ["spent", "budgeted"]:
                body = {"query": query, "snapshot": before["data"]["snapshot"], "month": "2026-11", "metric": metric, "limit": 2}
                check(body, "/drilldown", request=True)
                check(call(root + "/reports/budget-analysis/drilldown", body), "/drilldown")
            check(call(root + "/reports/budget-analysis", {**query, "version": 2}, expected=400), status=400)
            headers = {"X-Actual-Budget-Epoch": state["epoch"]}
            backup = call(root + "/backups", {"label": "Before analysis native round trip"}, headers, 201)["data"]
            version, original = portable(call(root + "/backups/" + backup["id"] + "/download"))
            duplicate = str(uuid.uuid4())
            call(root + "/duplicate", {"id": duplicate, "name": "Analysis native duplicate"}, headers, 201)
            assert call("/budgets/" + duplicate + "/reporting")["data"]["documents"] == state["documents"]
            copied = call("/budgets/" + duplicate + "/reports/budget-analysis", query)
            check(copied)
            assert copied["data"]["result"] == before["data"]["result"]
            call(root + "/backups/" + backup["id"] + "/restore", {"confirm_name": original["budget"]["name"]}, headers)
            restored = call(root + "/reporting")["data"]
            assert restored["epoch"] != state["epoch"] and restored["documents"] == state["documents"]
            after = call(root + "/reports/budget-analysis", query)
            check(after)
            assert after["data"]["epoch"] == restored["epoch"] and not after["data"]["cache"]["hit"]
            assert after["data"]["result"] == before["data"]["result"]
            backup = call(root + "/backups", {"label": "After analysis restore"}, {"X-Actual-Budget-Epoch": restored["epoch"]}, 201)["data"]
            current_version, current = portable(call(root + "/backups/" + backup["id"] + "/download"))
            assert current_version == version and clocks(current) == clocks(original)
            print(f"PASS {method} served analysis/detail/error schemas, native{version} duplicate/restore, preserved settings/clocks, readonly companion and epoch cache invalidation")
    finally:
        process.terminate()
        process.wait(timeout=20)
