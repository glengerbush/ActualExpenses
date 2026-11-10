// Unchanged pinned frontend spreadsheet/date functions, real pinned AQL engine.
// Only presentation (React elements, palette, localization) and browser transport
// are adapted; no financial query, reducer or date algorithm is replaced.
import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";
export function sourceRuntime(api, engine, presentation = {}) {
  const dependencies = createRequire(
      new URL(
        "../../actualExpenses/tools/compatibility/package.json",
        import.meta.url,
      ),
    ),
    web = createRequire(new URL("../web/package.json", import.meta.url));
  const ts = web("typescript"),
    base = fileURLToPath(
      new URL("../reference/actual-26.10.0/", import.meta.url),
    );
  const modules = new Map(),
    hashes = {};
  const core = "packages/loot-core/src/",
    frontend = "packages/desktop-client/src/";
  function resolve(file) {
    for (const ext of ["", ".ts", ".tsx", "/index.ts"])
      if (existsSync(path.join(base, file + ext))) return file + ext;
    throw Error("Missing pinned module " + file);
  }
  function load(file) {
    file = resolve(file);
    if (modules.has(file)) return modules.get(file).exports;
    const module = { exports: {} };
    modules.set(file, module);
    const source = readFileSync(path.join(base, file), "utf8");
    hashes[file] = createHash("sha256").update(source).digest("hex");
    function require(name) {
      if (name === "@actual-app/core/platform/client/connection")
        return { send: (name, args) => engine.send(name, args) };
      if (name === "#queries/aqlQuery")
        return { aqlQuery: (query) => api.aqlQuery(query) };
      if (name === "#shared/platform")
        return { OS: "linux", env: "web", isPlaywright: false };
      if (name === "i18next")
        return { t: presentation.translate ?? ((value) => value) };
      if (name === "csv-stringify/browser/esm/sync") return dependencies(name);
      if (name === "react")
        return { createElement: () => null, Fragment: () => null };
      if (name === "@actual-app/components/theme")
        return {
          theme: {
            reportsNumberNegative: "#a00",
            reportsNumberPositive: "#060",
            reportsNumberNeutral: "#555",
            ...presentation.theme,
          },
        };
      if (name === "#components/reports/chart-theme")
        return {
          getColorScale: () => presentation.colors ?? ["#060", "#a00", "#555"],
        };
      if (
        name === "@actual-app/components/aligned-text" ||
        name === "#components/FinancialText"
      )
        return { AlignedText: () => null, FinancialText: () => null };
      if (name.startsWith("@actual-app/core/shared/"))
        return load(core + "shared/" + name.split("/").at(-1));
      if (name.startsWith("#shared/"))
        return load(core + "shared/" + name.slice(8));
      if (name.startsWith("#components/reports/"))
        return load(
          frontend +
            "components/reports/" +
            name.slice("#components/reports/".length),
        );
      if (name.startsWith("."))
        return load(path.posix.join(path.posix.dirname(file), name));
      if (
        [
          "date-fns",
          "date-fns/locale",
          "es-toolkit",
          "es-toolkit/compat",
        ].includes(name)
      )
        return dependencies(name);
      throw Error(`Unexpected source dependency ${name} in ${file}`);
    }
    const output = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        jsx: ts.JsxEmit.React,
        esModuleInterop: true,
      },
    }).outputText;
    vm.runInNewContext(
      output,
      {
        exports: module.exports,
        module,
        require,
        Date: globalThis.Date,
        Map,
        Set,
        Intl,
        console,
        global: { IS_TESTING: false },
      },
      { filename: file },
    );
    return module.exports;
  }
  return {
    load: (file) => load(frontend + "components/reports/" + file),
    core: (file) => load(core + file),
    hashes,
  };
}
