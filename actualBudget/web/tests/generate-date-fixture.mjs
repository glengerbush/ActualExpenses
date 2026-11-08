// Execute the pinned DateSelect callback and month helpers with its date-fns version.
import { readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import vm from "node:vm";
import ts from "typescript";
const requireOracle = createRequire(
  new URL(
    "../../../actualExpenses/tools/compatibility/package.json",
    import.meta.url,
  ),
);
const dateFns = requireOracle("date-fns");
const version = requireOracle("date-fns/package.json").version;
if (version !== "4.4.0") throw new Error("Expected pinned date-fns4.4.0");
const base = new URL("../../reference/actual-26.10.0/", import.meta.url);
const select = await readFile(
  new URL("packages/desktop-client/src/components/select/DateSelect.tsx", base),
  "utf8",
);
const months = await readFile(
  new URL("packages/loot-core/src/shared/months.ts", base),
  "utf8",
);
const helpers =
  months.slice(
    months.indexOf("export const getDayMonthFormat ="),
    months.indexOf("export const getMonthYearFormat ="),
  ) + months.slice(months.indexOf("export const getShortYearFormat ="));
const callback = select.slice(
  select.indexOf("  const onUpdateEffect = useEffectEvent("),
  select.indexOf("\n  useEffect(() => {\n    onUpdateEffect(value);"),
);
const code = ts.transpileModule(
  helpers + "\n" + callback + "\nexports.parseInput=onUpdateEffect;",
  {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  },
).outputText;
const reference = "2026-11-08";
class FixedDate extends Date {
  constructor(...args) {
    super(...(args.length ? args : [reference + "T12:00:00Z"]));
  }
}
const cases = [];
for (const format of [
  "MM/dd/yyyy",
  "dd/MM/yyyy",
  "yyyy-MM-dd",
  "MM.dd.yyyy",
  "dd.MM.yyyy",
  "dd-MM-yyyy",
]) {
  const separator = format.match(/[^\w]/)[0];
  const fields = format.match(/yyyy|MM|dd/g);
  const assemble = (year, month, day) =>
    fields
      .map((t) => ({ yyyy: year, MM: month, dd: day })[t])
      .filter((v) => v !== null)
      .join(separator);
  const inputs = [
    assemble(null, "4", "5"),
    assemble(null, "04", "05"),
    assemble("19", "4", "5"),
    assemble("75", "4", "5"),
    assemble("76", "4", "5"),
    assemble("00", "4", "5"),
    assemble("2024", "2", "29"),
    assemble("24", "2", "29"),
    assemble(null, "2", "29"),
    assemble("2026", "2", "29"),
    assemble("2026", "13", "1"),
    assemble("2026", "1", "32"),
    assemble("1", "1", "2"),
    assemble("0001", "1", "2"),
    assemble("2026", "4", "5") + " ",
    "4",
    "4" + separator,
    "",
    "garbage",
    "2026-04-05",
  ];
  for (const input of inputs) {
    let expected = null;
    const context = {
      exports: {},
      Date: FixedDate,
      memoizeOne: (fn) => fn,
      useEffectEvent: (fn) => fn,
      dateFormat: format,
      onUpdate: (value) => (expected = value),
      setSelectedValue: () => {},
      ...dateFns,
    };
    vm.runInNewContext(code, context);
    context.exports.parseInput(input);
    cases.push({ format, input, expected });
  }
}
const output =
  JSON.stringify(
    {
      baseline: "Actual 26.10.0",
      date_fns: version,
      reference,
      source_sha256: {
        DateSelect: createHash("sha256").update(select).digest("hex"),
        months: createHash("sha256").update(months).digest("hex"),
      },
      cases,
    },
    null,
    2,
  ) + "\n";
const path = new URL("./fixtures/date-input-26.10.0.json", import.meta.url);
if (process.argv.includes("--check")) {
  if ((await readFile(path, "utf8")) !== output)
    throw new Error("Date oracle changed");
} else await writeFile(path, output);
console.log(`Verified ${cases.length} pinned date-input cases`);
