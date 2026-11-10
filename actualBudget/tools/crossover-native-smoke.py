# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Local disposable public Crossover/schema/current-native recovery qualification.
No database mutations or fabricated old-schema fixture. Archive versions are
observed from the produced public manifest, never relabeled as migration input.
"""
import hashlib, io, json, os, subprocess, tempfile, urllib.error, urllib.request, uuid, zipfile
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
project = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="crossover-native-") as directory:
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
            route = contract["paths"]["/api/v1/budgets/{budget}/reports/crossover" + suffix]["post"]
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
        source = json.loads((project / "tests/fixtures/crossover-source-26.10.json").read_text())
        archive = (project / "tests/fixtures/crossover-source-26.10.zip").read_bytes()
        assert hashlib.sha256(archive).hexdigest() == source["archive_sha256"]
        stage = call("/imports/actual", archive, expected=201)["data"]
        if stage["status"] == "ready": call("/imports/" + stage["id"] + "/activate", {}, expected=201)
        root = "/budgets/" + stage["budget_id"]
        state = call(root + "/reporting")["data"]
        assert next(d for d in state["documents"] if d["id"] == source["widgets"][0]["id"])["config"]["meta"] == source["widgets"][0]["meta"]
        query = {"version":1,"as_of":source["clock"][:10],"options":source["cases"][0]["options"]}
        check(query, request=True)
        companion_path = root + "/companion?account_id=" + source["ids"]["checking"]
        companion = call(companion_path)
        before = call(root + "/reports/crossover", query)
        check(before)
        assert before["data"]["result"] == source["cases"][0]["result"]
        assert call(companion_path) == companion
        check(call(root + "/reports/crossover", {**query,"version":2}, expected=400), status=400)
        headers = {"X-Actual-Budget-Epoch":state["epoch"]}
        backup = call(root + "/backups", {"label":"Before crossover native round trip"}, headers, 201)["data"]
        version, original = portable(call(root + "/backups/" + backup["id"] + "/download"))
        duplicate = str(uuid.uuid4())
        call(root + "/duplicate", {"id":duplicate,"name":"Crossover native duplicate"}, headers, 201)
        copied_root = "/budgets/" + duplicate
        assert call(copied_root + "/reporting")["data"]["documents"] == state["documents"]
        copied = call(copied_root + "/reports/crossover", query)
        check(copied)
        assert copied["data"]["result"] == before["data"]["result"]
        call(root + "/backups/" + backup["id"] + "/restore", {"confirm_name":original["budget"]["name"]}, headers)
        restored = call(root + "/reporting")["data"]
        assert restored["epoch"] != state["epoch"] and restored["documents"] == state["documents"]
        after = call(root + "/reports/crossover", query)
        check(after)
        assert after["data"]["result"] == before["data"]["result"] and not after["data"]["cache"]["hit"]
        backup = call(root + "/backups", {"label":"After crossover restore"}, {"X-Actual-Budget-Epoch":restored["epoch"]}, 201)["data"]
        current_version, current = portable(call(root + "/backups/" + backup["id"] + "/download"))
        assert current_version == version and clocks(current) == clocks(original)
        print(f"PASS served crossover request/result/error schemas, native{version} duplicate/restore, preserved settings/clocks, readonly companion and epoch cache invalidation")
        for signed in [9007199254740991,-9007199254740991]:
            budget, account = str(uuid.uuid4()),str(uuid.uuid4())
            call("/budgets", {"id":budget,"name":"Native crossover money boundary","budget_type":"tracking"}, expected=201)
            root = "/budgets/" + budget
            epoch = call(root + "/reporting")["data"]["epoch"]
            call(root + "/accounts", {"id":account,"name":"Boundary account","opening_balance":signed,"opening_date":"2026-10-01"}, {"X-Actual-Budget-Epoch":epoch}, 201)
            query = {"version":1,"as_of":"2026-11-09","options":{"start":"2026-10","end":"2026-10","expenseCategoryIds":[],"incomeAccountIds":[account],"safeWithdrawalRate":0.04,"estimatedReturn":0,"expectedContribution":0,"projectionType":"mean"}}
            check(query,request=True)
            value = call(root + "/reports/crossover",query)
            check(value)
            assert value["data"]["result"]["lastKnownBalance"] == signed
            assert all(point["nestEgg"] == signed for point in value["data"]["result"]["graphData"]["data"])
        print("PASS native tracking +/-MAX_MONEY historical and projected points validate against served schema; source tolerance claim remains separate")
    finally:
        process.terminate()
        process.wait(timeout=20)
