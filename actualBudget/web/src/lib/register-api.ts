import { request } from "./api";
export interface Transaction {
  is_parent?: boolean;
  parent_id?: string | null;
  children?: Transaction[];
  id: string;
  field_versions?: Record<string, string>;
  deleted?: boolean;
  account_id: string;
  account_name: string;
  amount: number;
  date: string;
  cleared: boolean;
  reconciled: boolean;
  starting_balance: boolean;
  notes: string;
  payee_id: string | null;
  payee_name: string | null;
  imported_payee?: string | null;
  retained_payee_id?: string | null;
  category_id: string | null;
  category_name: string | null;
  category_is_income: boolean | null;
  running_balance?: number | null;
}
export interface Reference {
  id: string;
  name: string;
  is_income?: boolean;
  hidden?: boolean;
  group_hidden?: boolean;
  deleted?: boolean;
}
export interface RegisterPage {
  data: Transaction[];
  balances: { total: number; cleared: number; uncleared: number };
  as_of: string | null;
  next_offset: number | null;
  snapshot: string;
}
export interface PendingMutation {
  path: string;
  method: "POST" | "PATCH" | "DELETE";
  key: string;
  edit_clock?: string;
  cancelRequested?: boolean;
  budgetEpoch?: string;
  body?: unknown;
}
/** Compare the immutable HTTP operation; cancellation and UI bookkeeping are not payload. */
export function sameMutation(
  a: PendingMutation | null | undefined,
  b: PendingMutation | null | undefined,
): boolean {
  if (!a || !b) return a === b;
  const identity = (value: PendingMutation) => [
    value.method,
    value.path,
    value.key,
    value.edit_clock ?? null,
    value.budgetEpoch ?? null,
    value.body ?? null,
  ];
  return JSON.stringify(identity(a)) === JSON.stringify(identity(b));
}
export function registerPage(
  budgetId: string,
  accountId: string | null,
  offset = 0,
  asOf = "",
  snapshot?: string,
  payeeId = "",
  tag = "",
) {
  const params = new URLSearchParams({ limit: "100", offset: String(offset) });
  if (accountId) params.set("account_id", accountId);
  if (asOf) params.set("as_of", asOf);
  if (snapshot) params.set("snapshot", snapshot);
  if (payeeId) params.set("payee_id", payeeId);
  if (tag) params.set("tag", tag);
  return request<RegisterPage>(`/budgets/${budgetId}/transactions?${params}`);
}
export async function references(
  budgetId: string,
  kind: "payees" | "categories",
): Promise<Reference[]> {
  const items: Reference[] = [];
  let offset: number | null = 0;
  do {
    const page: { data: Reference[]; next_offset: number | null } =
      await request(`/budgets/${budgetId}/${kind}?limit=1000&offset=${offset}`);
    items.push(...page.data);
    offset = page.next_offset;
  } while (offset !== null);
  return items;
}
function cancellationKey(pending: PendingMutation) {
  return `actualBudget.cancel.v1.${JSON.stringify([pending.method, pending.path, pending.key])}`;
}
export function cancellationRequested(pending: PendingMutation): boolean {
  return (
    pending.cancelRequested === true ||
    localStorage.getItem(cancellationKey(pending)) === "true"
  );
}
export function requestCancellation(pending: PendingMutation): void {
  // Separate monotonic intent survives stale primary-draft writes in another tab.
  localStorage.setItem(cancellationKey(pending), "true");
  pending.cancelRequested = true;
}
export function clearCancellation(pending: PendingMutation): void {
  localStorage.removeItem(cancellationKey(pending));
}
export async function mutate<T>(
  pending: PendingMutation,
  cancelIfUncommitted = false,
) {
  cancelIfUncommitted ||= cancellationRequested(pending);
  if (cancelIfUncommitted) {
    requestCancellation(pending);
    const { data } = await request<{ data: { capabilities?: string[] } }>(
      "/info",
    );
    if (!data.capabilities?.includes("transaction_cancellation"))
      throw new Error(
        "This server does not support safe cancellation. The saved request has been kept for retry.",
      );
  }
  return request<{
    data: T;
    conflicts?: {
      field: string;
      submitted_clock: string;
      winning_clock: string;
    }[];
    edit_clock?: string;
    cancelRequested?: boolean;
    cursor?: string;
  }>(
    pending.path,
    pending.body,
    pending.key,
    pending.method,
    pending.edit_clock,
    cancelIfUncommitted,
    pending.budgetEpoch,
  );
}
export function draftKey(budgetId: string) {
  return `actualBudget.transaction.v1.${budgetId}`;
}
export function actionKey(budgetId: string) {
  return `actualBudget.register-action.v1.${budgetId}`;
}

export function changes(budgetId: string, cursor: string) {
  return request<{
    data: { entity: string; id: string; data: unknown }[];
    next_cursor: string;
    has_more: boolean;
  }>(
    `/budgets/${budgetId}/changes?${new URLSearchParams({ cursor, limit: "100" })}`,
  );
}
