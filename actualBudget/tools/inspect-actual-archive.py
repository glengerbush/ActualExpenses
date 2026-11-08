#!/usr/bin/env python3
"""Read-only fixture audit of a pinned Actual export; no replacement calculations."""
import hashlib
import json
import pathlib
import sqlite3
import sys
import tempfile
import zipfile


def inspect(path):
    archive = pathlib.Path(path).read_bytes()
    with zipfile.ZipFile(path) as source:
        assert source.namelist() == ["db.sqlite", "metadata.json"], "Expected public export layout"
        metadata = json.loads(source.read("metadata.json"))
        database = source.read("db.sqlite")
    with tempfile.TemporaryDirectory(prefix="actual-archive-audit-") as directory:
        snapshot = pathlib.Path(directory) / "source.sqlite3"
        snapshot.write_bytes(database)
        db = sqlite3.connect(f"file:{snapshot}?mode=ro", uri=True)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA query_only=ON")
        db.execute("PRAGMA trusted_schema=OFF")
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        tables = {}
        for name, sql in db.execute("SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name"):
            quoted = '"' + name.replace('"', '""') + '"'
            columns = [row[1] for row in db.execute(f"PRAGMA table_info({quoted})")]
            count = db.execute(f"SELECT COUNT(*) FROM {quoted}").fetchone()[0]
            tables[name] = {"count": count, "columns": columns, "sql": sql}
            if "tombstone" in columns:
                tables[name]["tombstones"] = db.execute(f"SELECT COUNT(*) FROM {quoted} WHERE tombstone=1").fetchone()[0]
        records = {}
        for name in ["accounts", "account_groups", "transactions", "payees", "payee_mapping", "categories", "category_groups", "category_mapping", "preferences", "messages_clock", "messages_pending"]:
            records[name] = [dict(row) for row in db.execute(f'SELECT * FROM "{name}" ORDER BY rowid')]
        clocks = {}
        for row in db.execute("SELECT dataset,row,column,timestamp,value FROM messages_crdt ORDER BY timestamp"):
            record = dict(row)
            if isinstance(record["value"], bytes):
                record["value"] = record["value"].decode("utf-8")
            clocks[(record["dataset"], record["row"], record["column"])] = record
        db.close()
    return {
        "archive_sha256": hashlib.sha256(archive).hexdigest(),
        "archive_bytes": len(archive),
        "database_sha256": hashlib.sha256(database).hexdigest(),
        "metadata": metadata,
        "tables": tables,
        "records": records,
        "latest_field_clocks": [clocks[key] for key in sorted(clocks)],
    }


if __name__ == "__main__":
    print(json.dumps(inspect(sys.argv[1]), ensure_ascii=False, sort_keys=True))
