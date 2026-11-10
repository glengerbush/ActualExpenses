# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Public preference edits and native recovery preserve source formula fallbacks."""
import hashlib, io, json, os, subprocess, tempfile, urllib.error, urllib.request, uuid, zipfile
from pathlib import Path
from jsonschema import Draft202012Validator
project = Path(__file__).resolve().parents[1]
fixture = json.loads((project / "tests/fixtures/formula-formatting-edits-source.json").read_text())
with tempfile.TemporaryDirectory(prefix="formula-formatting-recovery-") as directory:
    env = {**os.environ,"ACTUALBUDGET_DATA_DIR":directory,"ACTUALBUDGET_HOST":"127.0.0.1","ACTUALBUDGET_PORT":"0","ACTUALBUDGET_WEB_DIR":str(project / "web/dist")}
    env.pop("ACTUALBUDGET_PUBLIC_ORIGIN",None)
    process = subprocess.Popen([str(project / "server/target/debug/actualbudget-server")],env=env,stdout=subprocess.PIPE,text=True)
    try:
        line = process.stdout.readline().strip()
        assert line.startswith("LISTENING "), line
        base = "http://" + line.removeprefix("LISTENING ") + "/api/v1"
        def call(path, body=None, *, method=None, headers=None, status=200):
            raw = body if isinstance(body,bytes) else None if body is None else json.dumps(body).encode()
            request = urllib.request.Request(base+path,data=raw,method=method,headers={"Content-Type":"application/zip" if isinstance(body,bytes) else "application/json","Idempotency-Key":str(uuid.uuid4()),**(headers or {})})
            try: response = urllib.request.urlopen(request,timeout=20)
            except urllib.error.HTTPError as e: response = e
            with response:
                value = json.load(response) if response.headers.get("Content-Type","").startswith("application/json") else response.read()
                assert response.status == status, (path,response.status,value)
                return value
        contract = call("/openapi.json")
        def validate_preferences(value):
            schema = contract["paths"]["/api/v1/budgets/{budget}/preferences"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            Draft202012Validator({**schema,"components":contract["components"]}).validate(value)
        def load(archive):
            raw = (project / "tests/fixtures" / archive["filename"]).read_bytes()
            assert hashlib.sha256(raw).hexdigest() == archive["sha256"]
            stage = call("/imports/actual",raw,status=201)["data"]
            assert stage["status"] in ("ready","activated"), stage
            if stage["status"] == "ready": call("/imports/"+stage["id"]+"/activate",{},status=201)
            return "/budgets/"+stage["budget_id"]
        def headers(root): return {"X-Actual-Budget-Epoch":call(root+"/checkpoint")["data"]["epoch"]}
        def check(root, observations):
            preferences = call(root+"/preferences"); validate_preferences(preferences)
            before = call(root+"/checkpoint")["data"]["snapshot"]
            ledger = call(root+"/transactions")["data"]
            for item in observations:
                output = call(root+"/reports/formula",{"formula":item["formula"],"locale":item["locale"],"as_of":"2026-11-09","evaluated_at":fixture["clock"],"snapshot":before})
                assert output["data"]["output"] == item["output"], (item,output)
            assert call(root+"/checkpoint")["data"]["snapshot"] == before
            assert call(root+"/transactions")["data"] == ledger
        def backup(root):
            result = call(root+"/backups",{"label":"Formatting recovery"},headers=headers(root),status=201)["data"]
            archive = call(root+"/backups/"+result["id"]+"/download")
            with zipfile.ZipFile(io.BytesIO(archive)) as z: state = json.loads(z.read("budget.json"))
            table = state["tables"]["budget_preferences"]
            settings = json.loads(table["rows"][0][table["columns"].index("settings")])
            return result, state, settings
        root = load(fixture["seed"])
        original = call(root+"/transactions")["data"]
        check(root,fixture["initial"])
        unrelated = fixture["steps"][0]
        call(root+"/preferences",unrelated["changes"],method="PATCH",headers=headers(root))
        check(root,unrelated["observations"])
        saved, state, settings = backup(root)
        assert "number_format" not in settings and "currency_symbol_position" not in settings
        duplicate = str(uuid.uuid4())
        call(root+"/duplicate",{"id":duplicate,"name":"Formatting defaults"},headers=headers(root),status=201)
        check("/budgets/"+duplicate,fixture["initial"])
        # Explicit comma-dot equals the public display default, but must still
        # override the absent source preference. Then try another explicit style.
        for step in [fixture["steps"][2],fixture["steps"][1]]:
            call(root+"/preferences",step["changes"],method="PATCH",headers=headers(root))
            check(root,step["observations"])
        explicit, _, explicit_settings = backup(root)
        assert explicit_settings["number_format"] == "apostrophe-dot"
        assert explicit_settings["currency_symbol_position"] == "before"
        for point, observations in [(saved,fixture["initial"]),(explicit,fixture["steps"][1]["observations"])]:
            call(root+"/backups/"+point["id"]+"/restore",{"confirm_name":state["budget"]["name"]},headers=headers(root))
            check(root,observations)
            assert call(root+"/transactions")["data"] == original
        null_root = load(fixture["null_case"]["archive"])
        check(null_root,fixture["null_case"]["observations"])
        _, _, null_settings = backup(null_root)
        assert "number_format" not in null_settings and "currency_symbol_position" not in null_settings
        print("PASS source absence/null import, unrelated/explicit preference edits, served preference contract, native backup/duplicate/restore and unchanged financial rows")
    finally:
        process.terminate(); process.wait(timeout=20)
