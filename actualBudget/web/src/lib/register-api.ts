import { request } from "./api";
export interface Transaction {
  id: string;
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
  category_id: string | null;
  category_name: string | null;
  category_is_income: boolean | null;
  running_balance?: number | null;
}
export interface Reference {
  id: string;
  name: string;
  is_income?: boolean;
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
  body?: unknown;
}
export function registerPage(
  budgetId: string,
  accountId: string | null,
  offset = 0,
  asOf = "",
  snapshot?: string,
) {
  const params = new URLSearchParams({ limit: "100", offset: String(offset) });
  if (accountId) params.set("account_id", accountId);
  if (asOf) params.set("as_of", asOf);
  if (snapshot) params.set("snapshot", snapshot);
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
export function mutate<T>(pending: PendingMutation) {
  return request<{ data: T }>(
    pending.path,
    pending.body,
    pending.key,
    pending.method,
  );
}
export function draftKey(budgetId: string) {
  return `actualBudget.transaction.v1.${budgetId}`;
}
export function actionKey(budgetId: string) {
  return `actualBudget.register-action.v1.${budgetId}`;
}
