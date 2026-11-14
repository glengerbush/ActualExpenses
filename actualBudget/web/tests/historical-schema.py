"""Rebuild a stopped test database with the actual DDL of an older release.

Copy fixture rows through shared columns; preserve that release's triggers and
constraints. Unlike changing user_version, this cannot retain newer schema.
"""
import pathlib
import re
import sqlite3
import sys

path = pathlib.Path(sys.argv[1])
version = int(sys.argv[2])
source = pathlib.Path(__file__).resolve().parents[2] / "server" / "src"
migrations = re.findall(r'include_str!\("([^"]+)"\)', (source / "migrations.rs").read_text())
assert 0 < version <= len(migrations)
old = sqlite3.connect(path)
old.execute("PRAGMA wal_checkpoint(TRUNCATE)")
rebuilt = sqlite3.connect(":memory:")
for migration in migrations[:version]:
    rebuilt.executescript((source / migration).read_text())
triggers = rebuilt.execute("SELECT name,sql FROM sqlite_master WHERE type='trigger'").fetchall()
for name, _ in triggers:
    rebuilt.execute(f'DROP TRIGGER "{name}"')
old_tables = {row[0] for row in old.execute("SELECT name FROM sqlite_master WHERE type='table'")}
for (table,) in rebuilt.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchall():
    # Keep the historical default policy: newer weekly intervals violate its old constraint.
    if table not in old_tables or table == "backup_policy":
        continue
    old_columns = {row[1] for row in old.execute(f'PRAGMA table_info("{table}")')}
    columns = [row[1] for row in rebuilt.execute(f'PRAGMA table_info("{table}")') if row[1] in old_columns]
    quoted = ",".join(f'"{column}"' for column in columns)
    rows = old.execute(f'SELECT {quoted} FROM "{table}"').fetchall()
    rebuilt.execute(f'DELETE FROM "{table}"')
    rebuilt.executemany(f'INSERT INTO "{table}" ({quoted}) VALUES ({",".join("?" for _ in columns)})', rows)
for _, sql in triggers:
    rebuilt.execute(sql)
rebuilt.execute(f"PRAGMA user_version={version}")
rebuilt.commit()
rebuilt.backup(old)
old.close()
rebuilt.close()
