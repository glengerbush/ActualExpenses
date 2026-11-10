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
}
export interface Projection {
  result: Record<string, unknown>;
  cells: ReportCell[];
  snapshot: string;
  as_of: string;
  recorded: true;
}
export interface Inspection {
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

  return { columns, rows };
}
