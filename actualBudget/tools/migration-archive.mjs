// Read exported public archive formats; never open the running server database.
import { execFileSync } from "node:child_process";

export function sourceArchive(bytes) {
  return JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `
import sys,io,zipfile,json,sqlite3,tempfile,pathlib
z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))
with tempfile.TemporaryDirectory() as directory:
 p=pathlib.Path(directory)/'source.sqlite'
 p.write_bytes(z.read('db.sqlite'))
 db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
 db.row_factory=sqlite3.Row
 tables={}
 for (name,) in db.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"):
  quoted='"'+name.replace('"','""')+'"'
  tables[name]=[dict(row) for row in db.execute('SELECT * FROM '+quoted)]
 db.close()
 print(json.dumps({'metadata':json.loads(z.read('metadata.json')),'tables':tables},default=lambda v:{'blob_hex':v.hex()}))
`,
      ],
      { input: bytes, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    ),
  );
}

export function nativeArchive(bytes) {
  return JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `
import sys,io,zipfile,json
z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))
print(json.dumps({'manifest':json.loads(z.read('manifest.json')),'state':json.loads(z.read('budget.json'))}))
`,
      ],
      { input: bytes, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    ),
  );
}

export function tableRows(archive, table) {
  const value = archive.state.tables[table];
  if (!value) throw new Error("Missing archive table: " + table);
  return value.rows.map((row) =>
    Object.fromEntries(
      value.columns.map((column, index) => [column, row[index]]),
    ),
  );
}
