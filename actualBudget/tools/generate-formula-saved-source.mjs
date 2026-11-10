import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { formulaSourceRuntime } from "./formula-source-runtime.mjs";
const require = createRequire(
    new URL(
      "../../actualExpenses/tools/compatibility/package.json",
      import.meta.url,
    ),
  ),
  api = require("@actual-app/api"),
  directory = await mkdtemp(tmpdir() + "/formula-saved-source-"),
  root = new URL("../tests/fixtures/", import.meta.url),
  file = new URL("formula-saved-source.json", root),
  archive = new URL("formula-saved-26.10.zip", root),
  check = process.argv.includes("--check"),
  corpus = JSON.parse(await readFile(new URL("formula-source.json", root))),
  base = JSON.parse(await readFile(new URL("reports-source-26.10.json", root))),
  query = corpus.observations.find((o) => o.name === "query-filtered"),
  sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
try {
  const engine = await api.init({ dataDir: directory });
  if (!check) {
    const bytes = await readFile(new URL(corpus.source.archive, root));
    assert.equal(sha(bytes), corpus.source.archive_sha256);
    await api.importBudget(bytes, { filename: "formula-seed.zip" });
    await engine.send("preferences/save", {
      id: "flags.formulaMode",
      value: "true",
    });
    await engine.send("dashboard-add-widget", {
      dashboard_page_id: base.source.dashboard_pages[0].id,
      type: "formula-card",
      x: 0,
      y: 24,
      width: 6,
      height: 3,
      meta: {
        name: "Saved formula café",
        formula: query.formula,
        queries: query.queries,
        queriesVersion: 7,
        colorFormula: '=IF(RESULT<0,"#a00","#060")',
        fontSizeMode: "static",
        staticFontSize: 36,
        fontSize: 28,
        showTitle: true,
      },
    });
    await writeFile(archive, await api.exportBudget(), { flag: "wx" });
  }
  const bytes = await readFile(archive);
  await api.importBudget(bytes, { filename: "formula-saved-reload.zip" });
  const widgets = (
    await api.aqlQuery(
      api.q("dashboard").filter({ type: "formula-card" }).select("*"),
    )
  ).data;
  assert.equal(widgets.length, 1);
  const widget = widgets[0],
    meta =
      typeof widget.meta === "string" ? JSON.parse(widget.meta) : widget.meta,
    runtime = formulaSourceRuntime(api, engine),
    output = await runtime.evaluate(meta.formula, meta.queries),
    color = await runtime.evaluate(meta.colorFormula, meta.queries, {
      RESULT: output.result ?? 0,
    });
  assert.deepEqual(output, query.output);
  const expected = {
    source_commit: corpus.source.commit,
    archive: { filename: "formula-saved-26.10.zip", sha256: sha(bytes) },
    widget: { ...widget, meta },
    output,
    color,
    source_hashes: runtime.hashes,
  };
  if (check) assert.deepEqual(expected, JSON.parse(await readFile(file)));
  else
    await writeFile(file, JSON.stringify(expected, null, 2) + "\n", {
      flag: "wx",
    });
  console.log(
    "PASS genuine saved formula settings, query and conditional color after source export/reload",
  );
} finally {
  await api.shutdown();
  await rm(directory, { recursive: true, force: true });
}
