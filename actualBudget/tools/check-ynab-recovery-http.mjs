// Running API and public native archive recovery; no direct database access.
import assert from "node:assert/strict";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { randomUUID, createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { startServer } from "../web/tests/server.mjs";
const external = !!process.env.ACTUALBUDGET_TEST_URL;
let directory = external ? null : await mkdtemp(tmpdir() + "/ynab-recovery-");
let server = await startServer(directory ? { dataDirectory: directory } : {}),
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
async function restart() {
  if (external) return;
  await server.stop();
  server = await startServer({ dataDirectory: directory });
  api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
}
async function call(path, body, headers = {}, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(
    r.status,
    status,
    `${path}: ${(await r.clone().text()).slice(0, 500)}`,
  );
  return r.json();
}
async function upload(kind, bytes, key = randomUUID(), status = 201) {
  const r = await fetch(
    api +
      `/imports/${kind}?filename=${kind === "ynab4" ? "recovery.ynab4.zip" : "recovery.json"}`,
    { method: "POST", headers: { "Idempotency-Key": key }, body: bytes },
  );
  assert.equal(r.status, status, (await r.clone().text()).slice(0, 500));
  return r.json();
}
function unpack(bytes) {
  return JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        "import zipfile,io,json,sys; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); d=json.loads(z.read('budget.json')); print(json.dumps({'state':d,'entries':{n:__import__('hashlib').sha256(z.read(n)).hexdigest() for n in z.namelist() if n!='budget.json'}}))",
      ],
      { input: bytes, maxBuffer: 8 * 1024 * 1024 },
    ),
  );
}
const financial = (response) => {
  const { snapshot, ...rest } = response;
  return rest;
};
try {
  for (const kind of ["ynab4", "nynab"]) {
    if (external)
      throw new Error(
        "YNAB recovery requires isolated local servers for each source format",
      );
    await server.stop();
    if (directory) await rm(directory, { recursive: true, force: true });
    directory = await mkdtemp(tmpdir() + "/ynab-recovery-");
    server = await startServer({ dataDirectory: directory });
    api = server.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
    const bytes = await readFile(
        new URL(
          `../tests/fixtures/ynab/${kind === "ynab4" ? "ynab4-demo.zip" : "nynab-demo.json"}`,
          import.meta.url,
        ),
      ),
      key = randomUUID(),
      staged = await upload(kind, bytes, key),
      id = staged.data.id;
    await upload(
      kind === "ynab4" ? "nynab" : "ynab4",
      bytes,
      randomUUID(),
      409,
    );
    await restart();
    assert.equal((await call("/imports/" + id)).data.id, id);
    assert.deepEqual(
      Buffer.from(
        await (await fetch(api + `/imports/${id}/original`)).arrayBuffer(),
      ),
      bytes,
    );
    assert.deepEqual(
      await upload(kind, bytes, key),
      staged,
      "Upload ACK loss survives restart",
    );
    const activateKey = randomUUID(),
      activated = await call(
        `/imports/${id}/activate`,
        {},
        { "Idempotency-Key": activateKey },
        201,
      ),
      b = activated.data.id,
      root = "/budgets/" + b,
      ledger = await call(root + "/transactions?limit=1000"),
      history = (
        await call(root + "/history", undefined, {
          "X-Actual-History-Owner": "27000000-0000-4000-8000-000000000001",
          "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
        })
      ).data;
    await restart();
    assert.deepEqual(
      await call(
        `/imports/${id}/activate`,
        {},
        { "Idempotency-Key": activateKey },
        201,
      ),
      activated,
      "Activation ACK loss survives restart",
    );
    assert.deepEqual(
      await call(root + "/transactions?limit=1000"),
      ledger,
      "Restart cannot reinstall or reorder imported rows",
    );
    assert.deepEqual(
      (
        await call(root + "/history", undefined, {
          "X-Actual-History-Owner": "27000000-0000-4000-8000-000000000001",
          "X-Actual-Budget-Epoch": (await call(root)).data.epoch,
        })
      ).data,
      history,
      "Imported historical edits are not fabricated undo actions",
    );
    const point = (
        await call(
          root + "/backups",
          { label: "YNAB original evidence" },
          {},
          201,
        )
      ).data,
      archive = Buffer.from(
        await (
          await fetch(api + root + `/backups/${point.id}/download`)
        ).arrayBuffer(),
      ),
      decoded = unpack(archive);
    const hash = createHash("sha256").update(bytes).digest("hex");
    assert.ok(
      Object.values(decoded.entries).includes(hash),
      "Native backup retains exact original source bytes",
    );
    const state = decoded.state;
    assert.ok(
      state.receipts.some(
        (r) => r.scope === `POST /imports/${kind}` && r.operation_id === key,
      ),
      "Original source upload receipt belongs to native budget archive",
    );
    assert.ok(
      state.receipts.some((r) => r.operation_id === activateKey),
      "Original activation receipt belongs to archive",
    );
    const checkpoint = await call(root + "/checkpoint");
    await call(
      root + "/duplicate",
      { id: randomUUID(), name: "Retired copy" },
      {},
      404,
    );
    assert.deepEqual(await call(root + "/checkpoint"), checkpoint);
    const imports = state.tables.staged_imports;
    const reportIndex = imports.columns.indexOf("report");
    assert.ok(
      imports.rows.some(
        (row) => JSON.parse(row[reportIndex]).source_format === kind,
      ),
      "Native backup retains the actual source format and original bytes",
    );
    const nativeKey = randomUUID(),
      nativeResponse = await fetch(api + "/restores", {
        method: "POST",
        headers: {
          "Idempotency-Key": nativeKey,
          "Content-Type": "application/zip",
        },
        body: archive,
      });
    assert.equal(
      nativeResponse.status,
      201,
      await nativeResponse.clone().text(),
    );
    const native = await nativeResponse.json(),
      epoch = (await call(root)).data.epoch;
    await call(
      root + `/restores/${native.data.id}/activate`,
      { confirm_name: activated.data.name },
      { "X-Actual-Budget-Epoch": epoch },
    );
    assert.deepEqual(
      financial(await call(root + "/transactions?limit=1000")),
      financial(ledger),
      "Native restore preserves all source financial rows",
    );
    assert.deepEqual(
      await upload(kind, bytes, key),
      staged,
      "Native restore keeps immutable original upload receipt",
    );
    assert.deepEqual(
      await call(
        `/imports/${id}/activate`,
        {},
        { "Idempotency-Key": activateKey },
        201,
      ),
      activated,
      "Native restore keeps immutable activation receipt",
    );
    assert.deepEqual(
      Buffer.from(
        await (await fetch(api + `/imports/${id}/original`)).arrayBuffer(),
      ),
      bytes,
    );
    if (!external) {
      const previousApi = api,
        portable = await startServer();
      try {
        api =
          portable.url.replace("actualbudget.test", "127.0.0.1") + "/api/v1";
        const response = await fetch(api + "/restores", {
          method: "POST",
          headers: {
            "Content-Type": "application/zip",
            "Idempotency-Key": randomUUID(),
          },
          body: archive,
        });
        assert.equal(response.status, 201, await response.clone().text());
        const stage = (await response.json()).data;
        const destination = (await call("/budget")).data;
        const portableRoot = "/budgets/" + destination.id;
        await call(
          portableRoot + `/restores/${stage.id}/activate`,
          {
            confirm_name: destination.name,
          },
          { "X-Actual-Budget-Epoch": destination.epoch },
        );
        assert.equal((await call("/budget")).data.id, destination.id);
        assert.equal((await call("/budgets")).data.length, 1);
        assert.deepEqual(
          financial(await call(portableRoot + "/transactions?limit=1000")),
          financial(ledger),
          "Cross-identity restore preserves complete financial rows in the singleton",
        );
        const freshUpload = await upload(kind, bytes, key);
        assert.notEqual(
          freshUpload.data.id,
          staged.data.id,
          "Cross-identity restore cannot resurrect the source upload receipt",
        );
        const current = (await call("/budget")).data;
        const repeated = await call(`/imports/${freshUpload.data.id}/merge`, {
          budget_id: current.id,
          epoch: current.epoch,
        });
        assert.equal(
          repeated.data.import_result.added,
          0,
          "Original import provenance prevents duplicate financial rows after cross-ID restore",
        );
        assert.deepEqual(
          financial(await call(portableRoot + "/transactions?limit=1000")),
          financial(ledger),
        );
        assert.deepEqual(
          Buffer.from(
            await (
              await fetch(api + `/imports/${freshUpload.data.id}/original`)
            ).arrayBuffer(),
          ),
          bytes,
        );
      } finally {
        api = previousApi;
        await portable.stop();
      }
    }
    console.log(
      "PASS " +
        kind +
        " interrupted staging/activation, original receipts, retired duplication and singleton native restore",
    );
  }
  const before = await call("/imports"),
    budgets = await call("/budgets");
  for (const bytes of [
    Buffer.from("{"),
    Buffer.from("{}"),
    Buffer.from(JSON.stringify({ budget: { name: "Invalid", accounts: [] } })),
  ]) {
    await upload("nynab", bytes, randomUUID(), 422);
  }
  await upload("ynab4", Buffer.from("not zip"), randomUUID(), 422);
  await upload(
    "nynab",
    Buffer.from(JSON.stringify({ accounts: Array(100001).fill({}) })),
    randomUUID(),
    422,
  );
  for (const kind of ["ynab4", "nynab"])
    await upload(
      kind,
      Buffer.alloc(16 * 1024 * 1024 + 1, 32),
      randomUUID(),
      413,
    );
  const unsafeZip = execFileSync("python3", [
    "-c",
    "import zipfile,io,sys; z=io.BytesIO(); a=zipfile.ZipFile(z,'w',zipfile.ZIP_DEFLATED); a.writestr('../unsafe','x'); a.close(); sys.stdout.buffer.write(z.getvalue())",
  ]);
  await upload("ynab4", unsafeZip, randomUUID(), 422);
  const expandedZip = execFileSync("python3", [
    "-c",
    "import zipfile,io,sys; z=io.BytesIO(); a=zipfile.ZipFile(z,'w',zipfile.ZIP_DEFLATED); a.writestr('oversized',b'x'*(64*1024*1024+1)); a.close(); sys.stdout.buffer.write(z.getvalue())",
  ]);
  await upload("ynab4", expandedZip, randomUUID(), 422);

  assert.deepEqual(await call("/imports"), before);
  assert.deepEqual(
    await call("/budgets"),
    budgets,
    "Malformed sources leave stages and active budgets unchanged",
  );
  console.log("PASS malformed YNAB sources are atomic");
} finally {
  await server.stop();
  if (directory) await rm(directory, { recursive: true, force: true });
}
