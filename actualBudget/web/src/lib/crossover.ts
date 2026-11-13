import { list, request, type Account } from "./api";
import { queryReport, localDay, object, type ReportDocument } from "./reports";
import { references } from "./register-api";
import { validDate } from "./dates";
import { addMonths } from "./report-dates";
export interface CrossoverQuery {
  version: 1;
  as_of: string;
  options: {
    start: string;
    end: string;
    expenseCategoryIds: string[];
    incomeAccountIds: string[];
    safeWithdrawalRate: number;
    estimatedReturn: number | null;
    expectedContribution: number | null;
    projectionType: string;
    expenseAdjustmentFactor: number;
  };
}
export interface CrossoverPoint {
  x: string;
  investmentIncome: number;
  expenses: number;
  nestEgg: number;
  adjustedExpenses?: number;
  isProjection?: boolean;
}
export interface CrossoverResult {
  graphData: {
    data: CrossoverPoint[];
    start: string;
    end: string;
    crossoverXLabel: string | null;
  };
  lastKnownBalance: number;
  lastKnownMonthlyIncome: number;
  lastKnownMonthlyExpenses: number;
  historicalReturn: number | null;
  yearsToRetire: number | null;
  targetMonthlyIncome: number | null;
  targetNestEgg: number | null;
}
export interface CrossoverProjection {
  result: CrossoverResult;
  months: string[];
  epoch: string;
  snapshot: string;
  as_of: string;
  recorded: false;
  basis: string;
  cache: { hit: boolean; input_revision: string };
  assumptions: { code: string; message: string }[];
}
function selection(value: unknown, defaults: string[]): string[] {
  if (value === undefined) return defaults;
  if (!Array.isArray(value) || value.some((id) => typeof id !== "string"))
    throw Error(
      "The saved selection is unreadable. Edit the report before calculating.",
    );
  return value;
}
export async function crossoverQuery(
  budget: string,
  document: ReportDocument,
): Promise<CrossoverQuery> {
  const meta = object(document.config.meta),
    today = localDay(),
    current = today.slice(0, 7),
    previous = addMonths(current, -1),
    frame = {
      start: addMonths(current, -120),
      end: previous,
      mode: "full",
      ...object(meta.timeFrame),
    };
  const [resolved, history, categories, accounts] = await Promise.all([
    queryReport(budget, {
      version: 1,
      kind: "widget-range",
      as_of: today,
      options: { timeFrame: frame },
    }),
    queryReport(budget, {
      version: 1,
      kind: "range",
      as_of: today,
      options: { range: "All time" },
    }),
    references(budget, "categories"),
    list<Account>(`/budgets/${budget}/accounts`),
  ]);
  const earliest = (history.result as unknown as string[])[0].slice(0, 7);
  if (earliest > previous)
    throw Error(
      "No completed historical month is available. Record earlier activity or choose another budget.",
    );
  let [start, end] = resolved.result as unknown as string[];
  if (frame.mode === "full") {
    start = earliest;
    end = previous;
  } else if (frame.mode === "sliding-window") {
    start = addMonths(start.slice(0, 7), -1);
    end = addMonths(end.slice(0, 7), -1);
  }
  const clamp = (month: string) =>
    month.slice(0, 7) < earliest
      ? earliest
      : month.slice(0, 7) > previous
        ? previous
        : month.slice(0, 7);
  start = clamp(start);
  end = clamp(end);
  if (end < start) end = start;
  const selected = selection(
    meta.expenseCategoryIds,
    categories.filter((c) => !c.is_income).map((c) => c.id),
  );
  const opts = {
    start,
    end,
    expenseCategoryIds: categories
      .filter((c) => selected.includes(c.id))
      .map((c) => c.id),
    incomeAccountIds: selection(
      meta.incomeAccountIds ?? undefined,
      accounts.map((a) => a.id),
    ),
    safeWithdrawalRate: meta.safeWithdrawalRate ?? 0.04,
    estimatedReturn: meta.estimatedReturn ?? null,
    expectedContribution: meta.expectedContribution ?? null,
    projectionType: meta.projectionType ?? "hampel",
    expenseAdjustmentFactor: meta.expenseAdjustmentFactor ?? 1,
  };
  for (const key of [
    "safeWithdrawalRate",
    "estimatedReturn",
    "expectedContribution",
    "expenseAdjustmentFactor",
  ] as const)
    if (
      opts[key] !== null &&
      (typeof opts[key] !== "number" || !Number.isFinite(opts[key]))
    )
      throw Error(
        "A saved projection assumption is unreadable. Edit the report before calculating.",
      );
  return {
    version: 1,
    as_of: today,
    options: opts as CrossoverQuery["options"],
  };
}
export async function readCrossover(budget: string, query: CrossoverQuery) {
  return (
    await request<{ data: CrossoverProjection }>(
      `/budgets/${budget}/reports/crossover`,
      query,
    )
  ).data;
}
export function validCrossover(
  value: CrossoverProjection,
  epoch: string,
): boolean {
  try {
    const r = value.result;
    return (
      value.epoch === epoch &&
      typeof value.snapshot === "string" &&
      typeof value.as_of === "string" &&
      Array.isArray(r.graphData.data) &&
      r.graphData.data.length <= 5600 &&
      Array.isArray(value.months) &&
      value.months.length === r.graphData.data.length &&
      value.months.every((m) => validDate(m + "-01")) &&
      r.graphData.data.every(
        (p) =>
          typeof p.x === "string" &&
          [p.investmentIncome, p.expenses, p.nestEgg].every(
            Number.isSafeInteger,
          ) &&
          (p.adjustedExpenses === undefined ||
            Number.isSafeInteger(p.adjustedExpenses)) &&
          (p.isProjection === undefined || typeof p.isProjection === "boolean"),
      ) &&
      [
        r.lastKnownBalance,
        r.lastKnownMonthlyExpenses,
        r.lastKnownMonthlyIncome,
      ].every(Number.isSafeInteger) &&
      [r.targetMonthlyIncome, r.targetNestEgg].every(
        (v) => v === null || Number.isSafeInteger(v),
      ) &&
      [r.historicalReturn, r.yearsToRetire].every(
        (v) => v === null || Number.isFinite(v),
      ) &&
      (r.graphData.crossoverXLabel === null ||
        typeof r.graphData.crossoverXLabel === "string") &&
      Array.isArray(value.assumptions) &&
      value.assumptions.every(
        (a) => typeof a.code === "string" && typeof a.message === "string",
      )
    );
  } catch {
    return false;
  }
}
