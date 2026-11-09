export interface Budget {
  id: string;
  name: string;
  budget_type: "tracking" | "envelope";
}
export interface Account {
  id: string;
  name: string;
  off_budget: boolean;
  closed: boolean;
  balance: number;
  notes?: string;
}
export interface NewAccount {
  id: string;
  name: string;
  off_budget: boolean;
  opening_balance: number;
  opening_date: string;
  notes: string;
}
export class RequestError extends Error {
  constructor(
    message: string,
    readonly uncertain = false,
    readonly code?: string,
  ) {
    super(message);
  }
}
export async function request<T>(
  path: string,
  body?: unknown,
  key?: string,
  method: "GET" | "POST" | "PATCH" | "DELETE" = body === undefined
    ? "GET"
    : "POST",
  editClock?: string,
): Promise<T> {
  const mutation = method !== "GET";
  let response: Response;
  try {
    response = await fetch(`/api/v1${path}`, {
      method,
      headers: {
        ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        ...(key === undefined ? {} : { "Idempotency-Key": key }),
        ...(editClock === undefined
          ? {}
          : { "X-Actual-Edit-Clock": editClock }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(15000),
    });
  } catch {
    throw new RequestError(
      "Cannot reach the server. Check your connection and retry.",
      mutation,
    );
  }
  const result = await response.json().catch(() => null);
  if (!response.ok || !result) {
    throw new RequestError(
      result?.error?.message ??
        "The server could not complete the request. Retry in a moment.",
      mutation && (response.status >= 500 || !result),
      typeof result?.error?.code === "string" ? result.error.code : undefined,
    );
  }
  return result;
}
export async function list<T>(path: string): Promise<T[]> {
  const rows: T[] = [];
  let cursor: string | null = null;
  do {
    const result: { data: T[]; next_page_cursor: string | null } =
      await request(
        `${path}${cursor ? `?page_cursor=${encodeURIComponent(cursor)}` : ""}`,
      );
    rows.push(...result.data);
    cursor = result.next_page_cursor;
  } while (cursor);
  return rows;
}
// getRandomValues works on plain HTTP; randomUUID requires a secure context.
export function uuid(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
    "",
  );
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
export function errorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "Request failed. Please retry.";
}
export function saveSelection(budgetId: string, accountId = ""): void {
  try {
    localStorage.setItem(
      "actualBudget.selection",
      JSON.stringify({ budgetId, accountId }),
    );
  } catch {
    /* Storage may be disabled; server data is unaffected. */
  }
}
export function loadSelection(): { budgetId?: string; accountId?: string } {
  try {
    const saved = JSON.parse(
      localStorage.getItem("actualBudget.selection") ?? "{}",
    );
    return saved && typeof saved === "object"
      ? {
          budgetId:
            typeof saved.budgetId === "string" ? saved.budgetId : undefined,
          accountId:
            typeof saved.accountId === "string" ? saved.accountId : undefined,
        }
      : {};
  } catch {
    return {};
  }
}
