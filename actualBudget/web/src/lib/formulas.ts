import { request, RequestError } from "./api";
import { localDay, object, type ReportDocument } from "./reports";
import { languageState } from "./i18n.svelte";
import { themes } from "./themes/themes.svelte";
import sourceNames from "./themes/source-names.json";
export type FormulaValue = string | number | boolean | null;
export interface FormulaOutput {
  result: FormulaValue;
  error: string | null;
}
export interface FormulaResult {
  output: FormulaOutput;
  color: FormulaOutput | null;
  snapshot: string;
  as_of: string;
  evaluated_at: string;
  locale: string;
}
export function localTimestamp() {
  const now = new Date(),
    offset = -now.getTimezoneOffset(),
    local = new Date(now.getTime() + offset * 60_000)
      .toISOString()
      .slice(0, 23);
  return (
    local +
    (offset < 0 ? "-" : "+") +
    String(Math.floor(Math.abs(offset) / 60)).padStart(2, "0") +
    ":" +
    String(Math.abs(offset) % 60).padStart(2, "0")
  );
}
export function themeExpressions(): Record<string, string> {
  const styles = getComputedStyle(document.documentElement);
  return Object.fromEntries(
    Object.entries(sourceNames).flatMap(([name, value]) => {
      const match = value.match(/^var\((--.*)\)$/);
      const resolved = match ? styles.getPropertyValue(match[1]).trim() : value;
      return resolved ? [["theme_" + name, resolved]] : [];
    }),
  );
}
export async function calculateFormula(
  budget: string,
  document: ReportDocument,
  epoch: string,
  preview = false,
): Promise<FormulaResult> {
  const meta = object(document.config.meta),
    checkpoint = (
      await request<{ data: { snapshot: string } }>(
        `/budgets/${budget}/checkpoint`,
      )
    ).data;
  if (!checkpoint.snapshot.startsWith("c1" + epoch))
    throw new RequestError(
      "The budget version changed. Refresh dashboards before calculating again.",
    );
  const input = {
    formula:
      typeof meta.formula === "string" && (preview || meta.formula)
        ? meta.formula
        : "=SUM(1, 2, 3)",
    queries: meta.queries ?? {},
    snapshot: checkpoint.snapshot,
    as_of: localDay(),
    evaluated_at: localTimestamp(),
    locale: languageState.language || "en-US",
  };
  const result = (
    await request<{ data: Omit<FormulaResult, "color"> }>(
      `/budgets/${budget}/reports/formula`,
      input,
    )
  ).data;
  let color: FormulaOutput | null = null;
  if (typeof meta.colorFormula === "string" && meta.colorFormula) {
    await themes.prepareFormulaColors();
    color = (
      await request<{ data: Omit<FormulaResult, "color"> }>(
        `/budgets/${budget}/reports/formula`,
        {
          ...input,
          formula: meta.colorFormula,
          named_expressions: {
            ...themeExpressions(),
            RESULT:
              typeof result.output.result === "boolean"
                ? String(result.output.result)
                : (result.output.result ?? 0),
          },
        },
      )
    ).data.output;
  }
  return { ...result, color };
}
export function validFormulaResult(
  value: FormulaResult,
  epoch: string,
): boolean {
  const valid = (o: FormulaOutput) =>
    !!o &&
    (o.result === null ||
      typeof o.result === "string" ||
      typeof o.result === "boolean" ||
      (typeof o.result === "number" && Number.isFinite(o.result))) &&
    (o.error === null || typeof o.error === "string");
  return (
    !!value &&
    typeof value.snapshot === "string" &&
    value.snapshot.startsWith("c1" + epoch) &&
    typeof value.as_of === "string" &&
    typeof value.evaluated_at === "string" &&
    valid(value.output) &&
    (value.color === null || valid(value.color))
  );
}
