import { request, uuid, RequestError } from "./api";
import {
  transaction,
  result,
  readValue,
  writeValue,
  OfflineStateError,
} from "./offline/storage";
export interface ReportDocument {
  id: string;
  kind: "page" | "widget" | "custom_report";
  revision: number;
  config: Record<string, unknown>;
  tombstone: boolean;
}
export interface Reporting {
  documents: ReportDocument[];
  epoch: string;
  snapshot: string;
}
export interface ReportQuery {
  version: 1;
  kind: string;
  as_of: string;
  options: Record<string, unknown>;
}
export interface ReportCell {
  id: string;
  label: string;
  start: string;
  end: string;
  metric: string;
  account: string | null;
  group?: string;
}
export interface Projection {
  result: Record<string, unknown>;
  cells: ReportCell[];
  snapshot: string;
  as_of: string;
  recorded: boolean;
  basis?: "recorded_transactions" | "budget_allocations" | "date_resolution";
}
export interface Inspection {
  record_kind?: "transactions" | "budget_allocations";
  rows: {
    id: string;
    date: string;
    amount: number;
    account: string;
    category: string;
    payee: string;
    notes: string;
  }[];
  total_count: number;
  total_amount: number;
  offset: number;
  limit: number;
  has_more: boolean;
  snapshot: string;
  cell: ReportCell;
}
export interface ReportOperation {
  version: 1;
  origin: string;
  budget: string;
  epoch: string;
  key: string;
  body: { writes: ReportDocument[] };
  cancelRequested?: boolean;
}
const operationKey = (b: string) => `reporting-operation:${b}`;
export const reportingCache = (b: string) => `view:reporting:${b}`;
export const draftKey = (b: string) => `reporting-draft:${b}`;
function validate(v: unknown, b: string): asserts v is ReportOperation {
  const o = v as ReportOperation;
  if (
    !o ||
    o.version !== 1 ||
    o.origin !== location.origin ||
    o.budget !== b ||
    !/^[a-f0-9]{32}$/.test(o.epoch) ||
    !/^[a-f0-9-]{36}$/.test(o.key) ||
    !Array.isArray(o.body?.writes) ||
    !o.body.writes.length ||
    (o.cancelRequested !== undefined && typeof o.cancelRequested !== "boolean")
  )
    throw new OfflineStateError(
      "Saved reporting change is unreadable. Keep browser data; no request was sent.",
    );
}
export async function readReportOperation(
  b: string,
): Promise<ReportOperation | null> {
  const o = await readValue<unknown>(operationKey(b));
  if (!o) return null;
  validate(o, b);
  return o;
}
function identity(o: ReportOperation) {
  const { cancelRequested: _, ...immutable } = o;
  return JSON.stringify(immutable);
}
export async function prepareReportOperation(
  budget: string,
  epoch: string,
  writes: ReportDocument[],
): Promise<ReportOperation> {
  const operation: ReportOperation = {
    version: 1,
    origin: location.origin,
    budget,
    epoch,
    key: uuid(),
    body: { writes: structuredClone(writes) },
  };
  validate(operation, budget);
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache");
    if (await result(store.get(operationKey(budget))))
      throw new OfflineStateError("Recover the saved reporting change first.");
    await result(store.put(operation, operationKey(budget)));
  });
  return operation;
}
export async function cancelReportOperation(
  o: ReportOperation,
): Promise<ReportOperation> {
  return transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      current = await result(store.get(operationKey(o.budget)));
    validate(current, o.budget);
    if (identity(current) !== identity(o))
      throw new OfflineStateError(
        "Saved reporting change changed in another tab. Reload first.",
      );
    const next = { ...current, cancelRequested: true };
    await result(store.put(next, operationKey(o.budget)));
    return next;
  });
}
export async function completeReportOperation(o: ReportOperation) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      current = await result(store.get(operationKey(o.budget)));
    if (current && JSON.stringify(current) !== JSON.stringify(o))
      throw new OfflineStateError(
        "Saved reporting change changed in another tab. Reload first.",
      );
    await result(store.delete(operationKey(o.budget)));
  });
}
export async function sendReportOperation(
  o: ReportOperation,
): Promise<Reporting> {
  const current = await readReportOperation(o.budget);
  if (!current || identity(current) !== identity(o))
    throw new OfflineStateError(
      "Saved reporting request changed. Reload before retry.",
    );
  const response = await request<{ data: Reporting }>(
    `/budgets/${o.budget}/reporting/commands`,
    o.body,
    o.key,
    "POST",
    undefined,
    !!current.cancelRequested,
    o.epoch,
  );
  return response.data;
}
export async function loadReporting(
  b: string,
): Promise<{ data: Reporting; cached: boolean }> {
  try {
    const { data } = await request<{ data: Reporting }>(
      `/budgets/${b}/reporting`,
    );
    await writeValue(reportingCache(b), data);
    return { data, cached: false };
  } catch (e) {
    if (e instanceof RequestError && !e.code) {
      const data = await readValue<Reporting>(reportingCache(b));
      if (data) return { data, cached: true };
    }
    throw e;
  }
}
export async function queryReport(
  b: string,
  query: ReportQuery,
): Promise<Projection> {
  const { data } = await request<{ data: Projection }>(
    `/budgets/${b}/reports/query`,
    query,
  );
  return data;
}
export async function inspectReport(
  b: string,
  query: ReportQuery,
  snapshot: string,
  cell_id: string,
  offset = 0,
): Promise<Inspection> {
  const { data } = await request<{ data: Inspection }>(
    `/budgets/${b}/reports/drilldown`,
    { query, snapshot, cell_id, offset, limit: 50 },
  );
  return data;
}
export function localDay() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
export function object(v: unknown): Record<string, unknown> {
  return v && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : {};
}
export function array(v: unknown): Record<string, unknown>[] {
  return Array.isArray(v) ? v.map(object) : [];
}
export interface DisplayRow {
  id: string;
  label: string;
  values: (number | null)[];
}
export function displayProjection(
  q: ReportQuery,
  p: Projection,
): { columns: string[]; rows: DisplayRow[] } {
  const r = p.result,
    g = object(r.graphData);
  let columns: string[] = [],
    rows: DisplayRow[] = [];
  if (q.kind === "cash-flow") {
    columns = ["Income", "Expenses", "Transfers", "Balance"];
    const arrays = ["income", "expenses", "transfers", "balances"].map((k) =>
      array(g[k]),
    );
    rows = arrays[0].map((row, i) => ({
      id: String(i),
      label: String(row.x).slice(0, 10),
      values: arrays.map((a) => Number(a[i]?.y ?? 0)),
    }));
  }
  if (q.kind === "net-worth") {
    const accounts = array(r.accounts);
    columns = ["Net worth", ...accounts.map((a) => String(a.name))];
    rows = array(g.data).map((row, i) => ({
      id: String(i),
      label: String(row.date),
      values: [
        Number(row.y),
        ...accounts.map((a) => Number(row[String(a.id)] ?? 0)),
      ],
    }));
  }
  if (q.kind === "summary") {
    columns = [
      object(q.options.summary).type === "percentage" ? "Percentage" : "Amount",
    ];
    rows = [
      {
        id: "summary",
        label: `${r.fromRange} – ${r.toRange}`,
        values: [r.total === null ? null : Number(r.total)],
      },
    ];
  }
  if (q.kind === "calendar") {
    columns = ["Income", "Expenses"];
    rows = array(r.calendarData).flatMap((month, mi) =>
      array(month.data).map((row, i) => ({
        id: `${mi}:${i}`,
        label: String(row.date).slice(0, 10),
        values: [Number(row.incomeValue), Number(row.expenseValue)],
      })),
    );
  }
  if (q.kind === "spending") {
    const mode = String(q.options.mode || "single-month"),
      compare =
        mode === "budget"
          ? "budget"
          : mode === "average"
            ? "average"
            : "compareTo";
    columns = [
      "Selected month",
      mode === "budget"
        ? "Budget"
        : mode === "average"
          ? "Average"
          : "Comparison month",
    ];
    rows = array(r.intervalData).map((row) => ({
      id: String(row.day),
      label: `Day ${row.day}`,
      values: ["compare", compare].map((k) =>
        row[k] === null ? null : Number(row[k]),
      ),
    }));
  }

  if (q.kind === "custom") {
    const metric = String(q.options.balanceTypeOp || "totalDebts");
    const primary = array(r.groupedData).length
      ? array(r.groupedData)
      : array(r.data);
    const groups: Record<string, unknown>[] = primary.flatMap((group) => [
      group,
      ...array(group.categories).map((child) => ({
        ...child,
        name: `${String(group.name)} / ${String(child.name)}`,
      })),
    ]);
    if (q.options.mode === "time" && q.options.groupBy !== "Interval") {
      columns = groups.map((group) => String(group.name));
      rows = array(r.intervalData).map((period, index) => ({
        id: String(index),
        label: String(period.date),
        values: groups.map((group) =>
          customMetric(
            array(group.intervalData).find(
              (value) => value.intervalStartDate === period.intervalStartDate,
            ),
            metric,
          ),
        ),
      }));
    } else {
      columns = [
        balanceModes.find((mode) => mode[1] === metric)?.[0] || "Amount",
      ];
      rows = (
        q.options.groupBy === "Interval" ? array(r.intervalData) : groups
      ).map((group, index) => ({
        id: String(index),
        label: String(group.name || group.date),
        values: [customMetric(group, metric)],
      }));
    }
  }
  return { columns, rows };
}
export const balanceModes = [
  ["Payment", "totalDebts"],
  ["Deposit", "totalAssets"],
  ["Net", "totalTotals"],
  ["Net Payment", "netDebts"],
  ["Net Deposit", "netAssets"],
  ["Budgeted", "totalBudgeted"],
] as const;
export const graphTypes = [
  ["TableGraph", "Table"],
  ["BarGraph", "Bar"],
  ["LineGraph", "Line"],
  ["AreaGraph", "Area"],
  ["DonutGraph", "Donut"],
] as const;
export const liveDateRanges = [
  "This week",
  "Last week",
  "This month",
  "Last month",
  "Current quarter",
  "Previous quarter",
  "Last 30 days",
  "Last 3 months",
  "Last 6 months",
  "Last 12 months",
  "Year to date",
  "Last year",
  "Prior year to date",
  "All time",
];
export const customDraftKey = (budget: string) =>
  `custom-report-draft:${budget}`;
export function flag(value: unknown, fallback = false) {
  return value === undefined || value === null
    ? fallback
    : value === true || value === 1;
}
export interface CustomDraft {
  version: 1;
  origin: string;
  budget: string;
  epoch: string;
  document: ReportDocument;
}
export async function readCustomDraft(
  budget: string,
): Promise<CustomDraft | null> {
  const draft = await readValue<CustomDraft>(customDraftKey(budget));
  if (!draft) return null;
  if (
    draft.version !== 1 ||
    draft.origin !== location.origin ||
    draft.budget !== budget ||
    !/^[a-f0-9]{32}$/.test(draft.epoch) ||
    draft.document?.kind !== "custom_report" ||
    !/^[a-f0-9-]{36}$/.test(draft.document.id) ||
    !Number.isSafeInteger(draft.document.revision) ||
    draft.document.revision < 0 ||
    !draft.document.config ||
    typeof draft.document.config !== "object"
  )
    throw new OfflineStateError(
      "Saved custom report draft is unreadable. Keep browser data; no change was sent.",
    );
  return draft;
}
export function newCustomReport(): ReportDocument {
  const today = localDay();
  return {
    id: uuid(),
    kind: "custom_report",
    revision: 0,
    tombstone: false,
    config: {
      name: "New custom report",
      start_date: today,
      end_date: today,
      date_static: 0,
      date_range: "Last 6 months",
      mode: "total",
      group_by: "Category",
      interval: "Monthly",
      balance_type: "Payment",
      sort_by: "desc",
      show_empty: 0,
      show_offbudget: 0,
      show_hidden: 1,
      show_uncategorized: 1,
      trim_intervals: 0,
      show_trend_lines: 0,
      include_current: 1,
      graph_type: "BarGraph",
      conditions: [],
      conditions_op: "and",
      metadata: null,
      color_scheme: null,
    },
  };
}
export async function customReportQuery(
  budget: string,
  config: Record<string, unknown>,
  as_of = localDay(),
  weekday = 0,
): Promise<ReportQuery> {
  let startDate = String(config.start_date || as_of),
    endDate = String(config.end_date || as_of);
  if (!flag(config.date_static)) {
    const dates = await queryReport(budget, {
      version: 1,
      kind: "range",
      as_of,
      options: {
        range: String(config.date_range || "Last 6 months"),
        includeCurrent: flag(config.include_current, true),
        first_day_of_week: String(weekday),
      },
    });
    [startDate, endDate] = dates.result as unknown as string[];
  }
  const metric = balanceModes.find(
    ([name]) => name === config.balance_type,
  )?.[1];
  if (!metric)
    throw Error(
      "This saved balance mode is preserved but unavailable. Choose a supported mode to run the report.",
    );
  const groupBy = String(config.group_by || "Category");
  return {
    version: 1,
    kind: "custom",
    as_of,
    options: {
      startDate,
      endDate,
      interval: String(config.interval || "Monthly"),
      mode: String(config.mode || "total"),
      groupBy,
      balanceTypeOp: metric,
      sortByOp: String(config.sort_by || "desc"),
      showEmpty: flag(config.show_empty),
      showOffBudget: flag(config.show_offbudget),
      showHiddenCategories: flag(config.show_hidden, true),
      showUncategorized: flag(config.show_uncategorized, true),
      trimIntervals: flag(config.trim_intervals),
      showTrendLines: flag(config.show_trend_lines),
      graphType: String(config.graph_type || "BarGraph"),
      conditions: Array.isArray(config.conditions) ? config.conditions : [],
      conditionsOp: String(config.conditions_op || "and"),
      firstDayOfWeekIdx: String(weekday),
      nested: groupBy === "CategoryGroup",
    },
  };
}
export function customMetric(value: unknown, metric: string): number {
  const n = Number(object(value)[metric] || 0);
  return ["totalDebts", "netDebts"].includes(metric) ? Math.abs(n) : n;
}
