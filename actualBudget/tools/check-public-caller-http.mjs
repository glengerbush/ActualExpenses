// Spawn a real stdlib-only Python caller against HTTP. No client/library mocks.
import assert from "node:assert/strict";
import { readFile, writeFile, mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createServer } from "node:http";
import { startServer } from "../web/tests/server.mjs";
import { cloneImportedTemplate } from "../web/tests/import-template.mjs";
const server = await startServer(),
  upstream = server.url.replace("actualbudget.test", "127.0.0.1"),
  api = upstream + "/api/v1",
  directory = await mkdtemp(tmpdir() + "/public-caller-"),
  tool = fileURLToPath(new URL("./public-api-client.py", import.meta.url)),
  batch = JSON.parse(
    await readFile(
      new URL("../tests/fixtures/public-batch-source.json", import.meta.url),
    ),
  ),
  manual = JSON.parse(
    await readFile(
      new URL(
        "../tests/fixtures/transaction-import-manual.json",
        import.meta.url,
      ),
    ),
  );
// Historical observations retain their bytes; these live projections follow completed-entry semantics.
batch.fields = batch.fields.filter((field) => field !== "notes");
for (const state of [batch.before, batch.after])
  for (const row of state) {
    delete row.notes;
    row.cleared = true;
    row.reconciled = false;
  }
for (const op of batch.operations) {
  if (op.transaction) delete op.transaction.notes;
  if (op.fields) delete op.fields.notes;
}
let loseAck = true,
  redirectNext = false,
  redirectRequests = 0,
  forwarded;
const attempts = [],
  proxyErrors = [];
const proxy = createServer(async (req, res) => {
  try {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const bytes = Buffer.concat(chunks),
      headers = Object.fromEntries(
        Object.entries(req.headers).filter(
          ([k]) => !["host", "connection", "content-length"].includes(k),
        ),
      ),
      isBatch =
        req.method === "POST" && req.url.endsWith("/transactions/batch");
    if (isBatch) attempts.push({ bytes, headers });
    if (req.url.endsWith("/unexpected-redirect")) redirectRequests++;
    if (isBatch && redirectNext) {
      redirectNext = false;
      res.writeHead(302, {
        Location: "/unexpected-redirect",
        Connection: "close",
      });
      res.end();
      return;
    }
    const reply = await fetch(upstream + req.url, {
        method: req.method,
        headers,
        ...(!["GET", "HEAD"].includes(req.method) ? { body: bytes } : {}),
      }),
      body = Buffer.from(await reply.arrayBuffer());
    if (isBatch && loseAck) {
      assert.equal(reply.status, 200, body.toString());
      forwarded = JSON.parse(body);
      loseAck = false;
      req.socket.destroy();
      return;
    }
    res.writeHead(reply.status, {
      "Content-Type": reply.headers.get("content-type") ?? "application/json",
      "Content-Length": body.length,
      Connection: "close",
    });
    res.end(body);
  } catch (e) {
    proxyErrors.push(String(e));
    res.destroy();
  }
});
await new Promise((resolve) => proxy.listen(0, "127.0.0.1", resolve));
const address = "http://127.0.0.1:" + proxy.address().port;
async function python(args, expected = 0) {
  return new Promise((resolve, reject) =>
    execFile(
      "python3",
      [tool, ...args],
      { maxBuffer: 8 * 1024 * 1024 },
      (error, stdout, stderr) => {
        try {
          assert.equal(
            error?.code ?? 0,
            expected,
            JSON.stringify(args) + ": " + stderr + " " + stdout.slice(0, 1200),
          );
          resolve(stdout.trim() ? JSON.parse(stdout) : null);
        } catch (e) {
          reject(e);
        }
      },
    ),
  );
}
async function call(path, body, status = 200) {
  const r = await fetch(api + path, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  assert.equal(r.status, status, (await r.clone().text()).slice(0, 800));
  return r.json();
}
async function fixtureBudget(archive) {
  const bytes = await readFile(
    new URL("../tests/fixtures/" + archive.filename, import.meta.url),
  );
  assert.equal(
    createHash("sha256").update(bytes).digest("hex"),
    archive.sha256,
  );
  const upload = await fetch(api + "/imports/actual", {
    method: "POST",
    headers: {
      "Content-Type": "application/zip",
      "Idempotency-Key": randomUUID(),
    },
    body: bytes,
  });
  assert.equal(upload.status, 201);
  const stage = (await upload.json()).data,
    b = (await call("/imports/" + stage.id + "/activate", {}, 201)).data;
  return cloneImportedTemplate(api, b.id);
}
async function jsonFile(name, value) {
  const path = directory + "/" + name;
  await writeFile(path, JSON.stringify(value));
  return path;
}
async function rows(b, fields = batch.fields) {
  return (
    await call("/budgets/" + b + "/query", {
      version: 1,
      table: "transactions",
      splits: "all",
      select: fields,
      order_by: ["id"],
    })
  ).data;
}
try {
  const b = await fixtureBudget(batch.archive),
    root = "/budgets/" + b,
    common = ["--server", address, "--budget", b],
    body = await jsonFile("batch.json", { operations: batch.operations }),
    outbox = directory + "/batch.outbox.json";
  assert.deepEqual(await rows(b), batch.before);
  await python([
    ...common,
    "prepare",
    "--path",
    "/transactions/batch",
    "--body",
    body,
    "--outbox",
    outbox,
  ]);
  const prepared = JSON.parse(await readFile(outbox));
  assert.equal((await stat(outbox)).mode & 0o777, 0o600);
  await python(
    [
      ...common,
      "prepare",
      "--path",
      "/transactions/batch",
      "--body",
      body,
      "--outbox",
      outbox,
    ],
    1,
  );
  assert.deepEqual(
    JSON.parse(await readFile(outbox)),
    prepared,
    "Preparation cannot overwrite retained identity",
  );
  await python(["send", "--outbox", outbox], 1);
  assert.ok(forwarded, "Lost ACK occurs after confirmed200 commit");
  assert.deepEqual(
    await rows(b),
    batch.after,
    "Server committed exact genuine source outcome before retry",
  );
  const pending = JSON.parse(await readFile(outbox));
  assert.equal(pending.attempted, true);
  assert.equal(pending.state, "attempted");
  assert.deepEqual(pending.request, prepared.request);
  const committed = (await python(["send", "--outbox", outbox])).body;
  assert.deepEqual(committed, forwarded);
  assert.equal(attempts.length, 2);
  assert.deepEqual(
    attempts[1],
    attempts[0],
    "Exact original JSON bytes/key/HLC/epoch/owner across process restart",
  );
  assert.deepEqual(
    JSON.parse(await readFile(outbox)).request,
    prepared.request,
  );
  assert.deepEqual(
    (await python(["receipt", "--outbox", outbox])).body,
    committed,
  );
  assert.deepEqual(await rows(b), batch.after);
  const queryFile = await jsonFile("query.json", {
    version: 1,
    table: "transactions",
    splits: "all",
    select: batch.fields,
    order_by: ["id"],
  });
  assert.deepEqual(
    (await python([...common, "query", "--body", queryFile])).body.data,
    batch.after,
  );
  await python(["--server", address, "send", "--outbox", outbox], 1);
  assert.deepEqual(
    await rows(b),
    batch.after,
    "Delivery refuses server/budget rebasing",
  );
  const redirected = await jsonFile("redirected.json", {
      operations: [
        {
          action: "create",
          transaction: { ...batch.operations[0].transaction, id: randomUUID() },
        },
      ],
    }),
    redirectBox = directory + "/redirect.outbox.json";
  await python([
    ...common,
    "prepare",
    "--path",
    "/transactions/batch",
    "--body",
    redirected,
    "--outbox",
    redirectBox,
  ]);
  const redirectIdentity = JSON.parse(await readFile(redirectBox)).request;
  redirectNext = true;
  await python(["send", "--outbox", redirectBox], 1);
  assert.equal(
    redirectRequests,
    0,
    "Captured mutation never follows a changed origin/method redirect",
  );
  assert.deepEqual(
    JSON.parse(await readFile(redirectBox)).request,
    redirectIdentity,
  );
  assert.deepEqual(await rows(b), batch.after);
  const canceled = (await python(["cancel", "--outbox", redirectBox])).body;
  assert.equal(canceled.error.code, "operation_canceled");
  assert.deepEqual(
    (await python(["send", "--outbox", redirectBox])).body,
    canceled,
  );
  assert.equal(JSON.parse(await readFile(redirectBox)).state, "canceled");
  assert.deepEqual(
    JSON.parse(await readFile(redirectBox)).request,
    redirectIdentity,
  );
  assert.deepEqual(await rows(b), batch.after);
  console.log(
    "PASS real external caller read/update; lost committed ACK exact durable replay, immutable origin and0600 outbox",
  );
  const imported = await fixtureBudget(manual.archive),
    importRoot = "/budgets/" + imported,
    importCommon = ["--server", address, "--budget", imported],
    csv = directory + "/" + manual.input.filename,
    uploadBox = directory + "/upload.outbox.json";
  await writeFile(csv, manual.input.text);
  assert.equal(
    createHash("sha256")
      .update(await readFile(csv))
      .digest("hex"),
    manual.input.sha256,
  );
  await python([
    ...importCommon,
    "prepare-upload",
    "--account",
    manual.ids.account,
    "--file",
    csv,
    "--outbox",
    uploadBox,
  ]);
  const stage = (await python(["send", "--outbox", uploadBox])).body.data;
  const original = await fetch(
    api + importRoot + "/transaction-imports/" + stage.id + "/original",
  );
  assert.equal(original.status, 200);
  assert.equal(
    Buffer.from(await original.arrayBuffer()).toString(),
    manual.input.text,
  );
  const cp = (await call(importRoot + "/checkpoint")).data,
    mapping = {
      snapshot: cp.snapshot,
      mapping: Object.fromEntries(
        Object.entries(manual.input.mapping).filter(([, v]) => v !== null),
      ),
      options: {
        has_header_row: true,
        date_format: manual.input.date_format,
        import_notes: true,
        clear_on_import: true,
      },
    },
    mappingFile = await jsonFile("mapping.json", mapping),
    before = await call(importRoot + "/transactions"),
    preview = (
      await python([
        ...importCommon,
        "preview",
        "--path",
        "/transaction-imports/" + stage.id + "/preview",
        "--body",
        mappingFile,
      ])
    ).body.data;
  assert.deepEqual(preview.errors, []);
  assert.deepEqual(
    await call(importRoot + "/transactions"),
    before,
    "External preview has no financial writes",
  );
  assert.equal(preview.rows[0].match_id, manual.ids.manual);
  const choices = preview.rows.map((row) => ({
      row: row.row,
      action: row.match_id ? "merge" : "add",
      ...(row.match_id ? { match_id: row.match_id } : {}),
    })),
    reviewed = await jsonFile("reviewed.json", {
      ...mapping,
      rows: choices,
      remember: false,
    }),
    importBox = directory + "/commit.outbox.json";
  await python([
    ...importCommon,
    "prepare",
    "--path",
    "/transaction-imports/" + stage.id + "/commit",
    "--body",
    reviewed,
    "--outbox",
    importBox,
  ]);
  const importedReceipt = (await python(["send", "--outbox", importBox])).body;
  assert.deepEqual(
    (await python(["receipt", "--outbox", importBox])).body,
    importedReceipt,
  );
  const expected = manual.after.transactions.map((row) =>
    Object.fromEntries(
      batch.fields.map((k) => [
        k,
        k === "cleared" ? true : k === "reconciled" ? false : (row[k] ?? null),
      ]),
    ),
  );
  assert.deepEqual(
    await rows(imported),
    expected,
    "Independent source manual-match ID, money, notes, refs, flags and graph",
  );
  assert.deepEqual(
    (await python(["send", "--outbox", importBox])).body,
    importedReceipt,
  );
  assert.deepEqual(await rows(imported), expected);
  // Exercise saved-state capacity independently of network/upload size caps.
  await new Promise((resolve, reject) =>
    execFile(
      "python3",
      [
        "-c",
        `
import base64, importlib.util, pathlib, sys
spec=importlib.util.spec_from_file_location("caller",sys.argv[1])
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
p=pathlib.Path(sys.argv[2])/"maximum-saved-state.json"
value={"request":{"body_base64":base64.b64encode(b"x"*m.MAX_UPLOAD).decode()},
       "committed_response":{"body":"x"*m.MAX_RESPONSE},
       "last_response":{"body":"x"*m.MAX_RESPONSE}}
m.save(p,value)
assert p.stat().st_size > 24*1024*1024
assert m.read_json(p,m.MAX_OUTBOX)==value
try: m.next_clock("2999-01-01T00:00:00.000Z-0000-1111111111111111")
except ValueError: pass
else: raise AssertionError("Future skew accepted")
import datetime
future=(datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=60)).isoformat(timespec="milliseconds").replace("+00:00","Z")
try: m.next_clock(future+"-FFFF-1111111111111111")
except ValueError: pass
else: raise AssertionError("Counter overflow accepted")
`,
        tool,
        directory,
      ],
      (error, stdout, stderr) =>
        error ? reject(new Error(stderr)) : resolve(),
    ),
  );
  // A fresh review must observe server HLCs, including valid future edits.
  const futureClock =
      new Date(Date.now() + 60_000).toISOString() + "-0010-1111111111111111",
    target = manual.ids.manual,
    seeded = await fetch(api + importRoot + "/transactions/" + target, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomUUID(),
        "X-Actual-Budget-Epoch": (await call(importRoot + "/checkpoint")).data
          .epoch,
        "X-Actual-Edit-Clock": futureClock,
      },
      body: JSON.stringify({ date: "2026-11-10" }),
    });
  assert.equal(seeded.status, 200, await seeded.clone().text());
  const plainPoint = (await call(importRoot + "/checkpoint")).data,
    clockPoint = (await call(importRoot + "/checkpoint?include_clock=true"))
      .data;
  assert.deepEqual(Object.keys(plainPoint).sort(), [
    "epoch",
    "generation",
    "snapshot",
  ]);
  const { edit_clock: observedClock, ...samePoint } = clockPoint;
  assert.ok(observedClock >= futureClock);
  assert.deepEqual(samePoint, plainPoint);
  assert.deepEqual((await call(importRoot + "/checkpoint")).data, plainPoint);
  const clockBody = await jsonFile("clock-update.json", {
      operations: [
        {
          action: "update",
          id: target,
          fields: { date: "2026-11-11" },
        },
      ],
    }),
    clockBox = directory + "/clock.outbox.json";
  await python([
    ...importCommon,
    "prepare",
    "--path",
    "/transactions/batch",
    "--body",
    clockBody,
    "--outbox",
    clockBox,
  ]);
  const clockRequest = JSON.parse(await readFile(clockBox)).request;
  assert.ok(
    clockRequest.headers["X-Actual-Edit-Clock"] > futureClock,
    "Preparation observes global server HLC before minting once",
  );
  const clockReceipt = (await python(["send", "--outbox", clockBox])).body;
  assert.deepEqual(
    (await python(["send", "--outbox", clockBox])).body,
    clockReceipt,
  );
  assert.deepEqual(JSON.parse(await readFile(clockBox)).request, clockRequest);
  assert.equal(
    (await rows(imported)).find((row) => row.id === target).date,
    "2026-11-11",
  );
  assert.deepEqual(proxyErrors, []);
  console.log(
    "PASS real external caller original-byte upload/read-only review/import/exact retry against genuine source manual matching",
  );
} finally {
  await new Promise((resolve) => proxy.close(resolve));
  await server.stop();
  await rm(directory, { recursive: true, force: true });
}
