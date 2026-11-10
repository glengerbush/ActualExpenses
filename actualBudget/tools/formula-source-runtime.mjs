// Unchanged production formula hook/plugin and real pinned engine transport.
// Only presentation (React elements, palette, localization) and browser transport
// are adapted; no financial query, reducer or date algorithm is replaced.
import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";
export function formulaSourceRuntime(api, engine) {
  let states = [],
    stateIndex = 0,
    settled,
    selectedLocale = "en-US";
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
      if (name === "i18next") return { t: (value) => value };
      if (name === "csv-stringify/browser/esm/sync") return dependencies(name);
      if (name === "react")
        return {
          useRef: (value) => ({ current: value }),
          useState: (initial) => {
            const i = stateIndex++;
            states[i] = initial;
            return [
              initial,
              (value) => {
                states[i] = value;
                if (i === 1 && value === false) settled();
              },
            ];
          },
          useEffect: (effect) => {
            effect();
          },
          createElement: () => null,
          Fragment: () => null,
        };
      if (name === "./useLocale") return { useLocale: () => selectedLocale };
      if (name === "./useGlobalPref") return { useGlobalPref: () => [selectedLocale] };
      if (name === "hyperformula" || name.startsWith("hyperformula/"))
        return dependencies(name);
      if (name.startsWith("#util/"))
        return load(frontend + "util/" + name.slice(6));
      if (name.startsWith("#components/formula/"))
        return load(frontend + "components/formula/" + name.slice(20));
      if (name === "@actual-app/components/theme")
        return {
          theme: {
            reportsNumberNegative: "#a00",
            reportsNumberPositive: "#060",
            reportsNumberNeutral: "#555",
          },
        };
      if (name === "#components/reports/chart-theme")
        return { getColorScale: () => ["#060", "#a00", "#555"] };
      if (
        name === "@actual-app/components/aligned-text" ||
        name === "#components/FinancialText"
      )
        return { AlignedText: () => null, FinancialText: () => null };
      if (name.startsWith("@actual-app/core/shared/"))
        return load(
          core + "shared/" + name.slice("@actual-app/core/shared/".length),
        );
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
    // HyperFormula checks cell arrays with instanceof Array. Execute trusted
    // pinned modules in the dependency's realm, as the production bundle does.
    vm.runInThisContext(
      "(function(exports,module,require){" + output + "\n})",
      { filename: file },
    )(module.exports, module, require);
    return module.exports;
  }
  return {
    async evaluate(formula, queries = {}, namedExpressions, accounts = [], locale = "en-US") {
      selectedLocale = locale;
      states = [];
      stateIndex = 0;
      const done = new Promise((resolve) => {
        settled = resolve;
      });
      load(frontend + "hooks/useFormulaExecution").useFormulaExecution(
        formula,
        queries,
        1,
        namedExpressions,
        accounts,
      );
      await done;
      return { result: states[0], error: states[2] };
    },
    core: (file) => load(core + file),
    hashes,
  };
}
