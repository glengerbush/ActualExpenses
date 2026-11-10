import { request } from "./api";
import { dateObject } from "./dates";
import { localDay, object, queryReport, type ReportDocument } from "./reports";
export interface ForecastRequest {
  version: 1;
  as_of: string;
  options: Record<string, unknown>;
}
export interface ForecastProjection {
  epoch: string;
  snapshot: string;
  as_of: string;
  recorded: false;
  chart: { date: string; balance: number }[];
  lowest_displayed: { date: string; balance: number } | null;
  scheduled_occurrence_count: number;
  assumptions: { code: string; message: string }[];
  result: {
    dataPoints: {
      date: string;
      balance: number;
      accountId: string;
      accountName: string;
      transactions: {
        amount: number;
        payee: string;
        scheduleId: string;
        scheduleName: string;
      }[];
    }[];
    forecastStartDate: string;
    forecastEndDate: string;
  };
}
export async function forecastRequest(
  budget: string,
  document: ReportDocument,
): Promise<ForecastRequest> {
  const meta = object(document.config.meta),
    today = localDay();
  const frame = Object.keys(object(meta.timeFrame)).length
    ? object(meta.timeFrame)
    : {
        mode: "static",
        start: meta.startDate || today.slice(0, 7),
        end: meta.endDate || addMonths(today.slice(0, 7), 11),
      };
  const dates = await queryReport(budget, {
    version: 1,
    kind: "widget-range",
    as_of: today,
    options: { timeFrame: frame },
  });
  const [start, end] = dates.result as unknown as string[];
  const lastDay = dateObject(end.slice(0, 7) + "-01");
  lastDay.setUTCMonth(lastDay.getUTCMonth() + 1, 0);
  const last = lastDay.getUTCDate();
  return {
    version: 1,
    as_of: today,
    options: {
      startDate: start.slice(0, 7) + "-01",
      endDate: end.slice(0, 7) + "-" + last,
      viewStart: start,
      viewEnd: end,
      source: meta.source ?? "schedules",
      granularity: meta.granularity ?? "Monthly",
      ...(Array.isArray(meta.accounts) ? { accountIds: meta.accounts } : {}),
      conditions: meta.conditions ?? [],
      conditionsOp: meta.conditionsOp ?? "and",
      includeAccountlessSchedules: meta.accounts === undefined,
    },
  };
}
export function addMonths(month: string, n: number) {
  const d = dateObject(month + "-01");
  d.setUTCMonth(d.getUTCMonth() + n);
  return d.toISOString().slice(0, 7);
}
export async function queryForecast(budget: string, query: ForecastRequest) {
  return (
    await request<{ data: ForecastProjection }>(
      `/budgets/${budget}/reports/forecast`,
      query,
    )
  ).data;
}
