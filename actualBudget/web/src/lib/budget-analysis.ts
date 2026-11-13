import { request } from "./api";
import { queryReport, localDay, object, type ReportDocument } from "./reports";
import { dateObject, isoDate } from "./dates";
export interface AnalysisQuery {
  version: 1;
  as_of: string;
  options: {
    startDate: string;
    endDate: string;
    conditions?: unknown[];
    conditionsOp?: string;
    showHiddenCategories?: boolean;
  };
}
export interface AnalysisRow {
  date: string;
  budgeted: number;
  spent: number;
  balance: number;
  overspendingAdjustment: number;
}
export interface AnalysisResult {
  intervalData: AnalysisRow[];
  startDate: string;
  endDate: string;
  totalBudgeted: number;
  totalSpent: number;
  totalOverspendingAdjustment: number;
  finalOverspendingAdjustment: number;
}
export interface AnalysisProjection {
  result: AnalysisResult;
  categories: {
    id: string;
    name: string;
    group_id: string | null;
    income_group: boolean;
  }[];
  snapshot: string;
  epoch: string;
  as_of: string;
  recorded: false;
  basis: "budget_allocations_and_recorded_entries";
  method: "tracking";
  assumptions: { code: string; message: string }[];
  cache: { hit: boolean; input_revision: string };
}
export interface AnalysisDetails {
  rows: {
    id: string;
    amount: number;
    date?: string;
    month?: string;
    account_name?: string;
    category_name?: string;
    payee_name?: string | null;
    notes?: string;
  }[];
  total_count: number;
  total_amount: number;
  offset: number;
  limit: number;
  has_more: boolean;
  snapshot: string;
  month: string;
  metric: "spent" | "budgeted";
  basis: "recorded_entries" | "budget_allocations";
}
export async function analysisQuery(
  budget: string,
  document: ReportDocument,
): Promise<AnalysisQuery> {
  const meta = object(document.config.meta),
    today = localDay(),
    start = dateObject(today.slice(0, 7) + "-01");
  start.setUTCMonth(start.getUTCMonth() - 5);
  const range = await queryReport(budget, {
    version: 1,
    kind: "widget-range",
    as_of: today,
    options: {
      timeFrame: meta.timeFrame ?? {
        start: isoDate(start).slice(0, 7),
        end: today.slice(0, 7),
        mode: "sliding-window",
      },
    },
  });
  const [a, z] = range.result as unknown as string[],
    last = dateObject(z.slice(0, 7) + "-01");
  last.setUTCMonth(last.getUTCMonth() + 1, 0);
  return {
    version: 1,
    as_of: today,
    options: {
      startDate: a.slice(0, 7) + "-01",
      endDate: isoDate(last),
      conditions: (meta.conditions ?? []) as unknown[],
      conditionsOp: String(meta.conditionsOp ?? "and"),
    },
  };
}
export async function readAnalysis(budget: string, query: AnalysisQuery) {
  return (
    await request<{ data: AnalysisProjection }>(
      `/budgets/${budget}/reports/budget-analysis`,
      query,
    )
  ).data;
}
export async function inspectAnalysis(
  budget: string,
  query: AnalysisQuery,
  snapshot: string,
  month: string,
  metric: "spent" | "budgeted",
  offset = 0,
) {
  return (
    await request<{ data: AnalysisDetails }>(
      `/budgets/${budget}/reports/budget-analysis/drilldown`,
      { query, snapshot, month, metric, offset, limit: 50 },
    )
  ).data;
}
export function analysisCsv(result: AnalysisResult, headers: string[]): string {
  const string = (value: string) => {
    const safe = /^[=+\-@\t\r]/.test(value) ? "'" + value : value;
    return /[",\r\n]/.test(safe)
      ? '"' + safe.replaceAll('"', '""') + '"'
      : safe;
  };
  // Numeric fields remain numeric CSV cells, as in the unchanged source exporter.
  const decimal = (value: number) => {
    const n = BigInt(value),
      a = n < 0n ? -n : n,
      f = String(a % 100n)
        .padStart(2, "0")
        .replace(/0+$/, "");
    return (n < 0n ? "-" : "") + String(a / 100n) + (f ? "." + f : "");
  };
  const rows = result.intervalData.map((row) =>
    [
      string(row.date),
      decimal(row.budgeted),
      decimal(row.spent),
      decimal(row.overspendingAdjustment),
      decimal(row.balance),
    ].join(","),
  );
  return [headers.map(string).join(","), ...rows].join("\n") + "\n";
}
