# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Disposable public formula contract/source import/native recovery checks."""
import hashlib, io, json, os, subprocess, tempfile, urllib.error, urllib.request, uuid, zipfile
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
project = Path(__file__).resolve().parents[1]
source = json.loads((project / "tests/fixtures/formula-saved-source.json").read_text())
with tempfile.TemporaryDirectory(prefix="formula-native-") as directory:
    env = {**os.environ, "ACTUALBUDGET_DATA_DIR": directory, "ACTUALBUDGET_HOST": "127.0.0.1", "ACTUALBUDGET_PORT": "0", "ACTUALBUDGET_WEB_DIR": str(project / "web/dist")}
    env.pop("ACTUALBUDGET_PUBLIC_ORIGIN", None)
    process = subprocess.Popen([str(project / "server/target/debug/actualbudget-server")], env=env, stdout=subprocess.PIPE, text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        base = "http://" + line.removeprefix("LISTENING ") + "/api/v1"
        def call(path, body=None, headers=None, status=200):
            raw = body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
            req = urllib.request.Request(base + path, data=raw, headers={"Content-Type": "application/zip" if isinstance(body, bytes) else "application/json", "Idempotency-Key": str(uuid.uuid4()), **(headers or {})})
            try: response = urllib.request.urlopen(req, timeout=20)
            except urllib.error.HTTPError as e: response = e
            with response:
                value = json.load(response) if response.headers.get("Content-Type", "").startswith("application/json") else response.read()
                assert response.status == status, (path, response.status, value)
                return value
        contract = call("/openapi.json")
        css = ":root { --color-warningText: #123456; }"
        compiled = call("/themes/compile", {"css":css})["data"]
        theme_schema = contract["components"]["schemas"]["CompiledTheme"]
        theme_validator = Draft202012Validator({**theme_schema,"components":contract["components"]})
        theme_validator.validate(compiled)
        assert compiled["version"] == 2 and compiled["source_hash"] == hashlib.sha256(css.encode()).hexdigest()
        assert compiled["variables"]["--color-warningText"] == "#123456"
        legacy = {key:value for key,value in compiled.items() if key != "source_hash"}
        legacy["version"] = 1
        theme_validator.validate(legacy)
        route = contract["paths"]["/api/v1/budgets/{budget}/reports/formula"]["post"]
        def validate(value, request=False, status=200):
            schema = route["requestBody"]["content"]["application/json"]["schema"] if request else route["responses"][str(status)]["content"]["application/json"]["schema"]
            Draft202012Validator({**schema, "components": contract["components"]}, format_checker=FormatChecker()).validate(value)
        raw = (project / "tests/fixtures" / source["archive"]["filename"]).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == source["archive"]["sha256"]
        stage = call("/imports/actual", raw, status=201)["data"]
        if stage["status"] == "ready": call("/imports/" + stage["id"] + "/activate", {}, status=201)
        root = "/budgets/" + stage["budget_id"]
        state = call(root + "/reporting")["data"]
        widget = next(d for d in state["documents"] if d["id"] == source["widget"]["id"])
        meta = source["widget"]["meta"]
        assert widget["config"]["meta"] == meta
        assert call(root + "/preferences")["data"]["experimental"]["formulaMode"]
        checkpoint = call(root + "/checkpoint")["data"]["snapshot"]
        ledger = call(root + "/transactions")
        query = {"formula":meta["formula"], "queries":meta["queries"], "snapshot":checkpoint, "as_of":"2026-11-09", "evaluated_at":"2026-11-09T12:00:00Z"}
        validate(query, request=True)
        result = call(root + "/reports/formula", query); validate(result)
        assert result["data"]["output"] == source["output"]
        color = {**query, "formula":meta["colorFormula"], "named_expressions":{"RESULT":source["output"]["result"]}}
        validate(color, request=True)
        result = call(root + "/reports/formula", color); validate(result)
        assert result["data"]["output"] == source["color"]
        validate(call(root + "/reports/formula", {**query,"evaluated_at":"invalid"}, status=400), status=400)
        validate(call(root + "/reports/formula", {**query,"snapshot":"c1" + "f"*48}, status=409), status=409)
        assert call(root + "/transactions") == ledger and call(root + "/checkpoint")["data"]["snapshot"] == checkpoint
        headers = {"X-Actual-Budget-Epoch":state["epoch"]}
        backup = call(root + "/backups", {"label":"Before formula recovery"}, headers, 201)["data"]
        archive = call(root + "/backups/" + backup["id"] + "/download")
        with zipfile.ZipFile(io.BytesIO(archive)) as z:
            version = json.loads(z.read("manifest.json"))["schema_version"]
            original = json.loads(z.read("budget.json"))
        duplicate = str(uuid.uuid4())
        call(root + "/duplicate", {"id":duplicate,"name":"Formula native copy"}, headers, 201)
        copy = "/budgets/" + duplicate
        assert call(copy + "/reporting")["data"]["documents"] == state["documents"]
        copied = call(copy + "/reports/formula", {**query,"snapshot":call(copy + "/checkpoint")["data"]["snapshot"]})
        validate(copied); assert copied["data"]["output"] == source["output"]
        call(root + "/backups/" + backup["id"] + "/restore", {"confirm_name":original["budget"]["name"]}, headers)
        restored = call(root + "/reporting")["data"]
        assert restored["epoch"] != state["epoch"] and restored["documents"] == state["documents"]
        validate(call(root + "/reports/formula", query, status=409), status=409)
        result = call(root + "/reports/formula", {**query,"snapshot":call(root + "/checkpoint")["data"]["snapshot"]})
        validate(result); assert result["data"]["output"] == source["output"]
        # Saving a report publishes an ordinary feed record. Exercise live
        # create/update/delete consumption so a reporting write cannot strand
        # browser or companion clients behind an unreadable checkpoint.
        feed_schema = contract["components"]["schemas"]["ChangePage"]
        cursor = call(root + "/checkpoint")["data"]["snapshot"]
        extra = {**widget, "id":str(uuid.uuid4()), "revision":0}
        for revision, tombstone in [(0,False),(1,False),(2,True)]:
            extra = {**extra, "revision":revision, "tombstone":tombstone}
            written = call(root + "/reporting/commands", {"writes":[extra]}, {"X-Actual-Budget-Epoch":restored["epoch"]})["data"]
            page = call(root + "/changes?cursor=" + cursor + "&limit=1")
            Draft202012Validator({**feed_schema,"components":contract["components"]}, format_checker=FormatChecker()).validate(page)
            row = next(d for d in written["documents"] if d["id"] == extra["id"])
            assert page["data"] == [{"entity":"reporting_documents","id":extra["id"],"data":row}]
            assert page["next_cursor"] > cursor and not page["has_more"]
            cursor = page["next_cursor"]
        assert call(root + "/transactions")["data"] == ledger["data"]
        print(f"PASS source formula widget/query/style/opt-in import, exact value/color, served schemas, readonly state and native{version} duplicate/restore with stale snapshot refusal")
    finally:
        process.terminate(); process.wait(timeout=20)
