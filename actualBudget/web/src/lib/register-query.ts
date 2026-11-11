export type Condition = {
  field: string;
  op: string;
  value:
    | string
    | number
    | boolean
    | null
    | (string | number | boolean | null)[]
    | Record<string, unknown>;
  options?: {
    inflow?: boolean;
    outflow?: boolean;
    month?: boolean;
    year?: boolean;
  };
};
export type RegisterQuery = {
  search: string;
  account_scope?: "all" | "onbudget" | "offbudget" | "closed" | "uncategorized";
  sort: string;
  direction: "asc" | "desc";
  hide_cleared: boolean;
  hide_reconciled: boolean;
  filter: { op: "and" | "or"; conditions: Condition[] };
};
export const columnLabels: Record<string, string> = {
  date: "Date",
  account: "Account",
  payee: "Payee",
  notes: "Notes",
  group: "Category group",
  category: "Category",
  amount: "Amount",
  payment: "Payment",
  deposit: "Deposit",
  status: "Status",
  balance: "Running balance",
};
export const defaultColumns = [
  "date",
  "payee",
  "notes",
  "category",
  "amount",
  "status",
  "balance",
];
export function defaultQuery(): RegisterQuery {
  return {
    search: "",
    account_scope: "all",
    sort: "date",
    direction: "desc",
    hide_cleared: false,
    hide_reconciled: false,
    filter: { op: "and", conditions: [] },
  };
}
export function isDefaultQuery(query: RegisterQuery) {
  return (
    !query.search &&
    (query.account_scope ?? "all") === "all" &&
    query.sort === "date" &&
    query.direction === "desc" &&
    !query.hide_cleared &&
    !query.hide_reconciled &&
    query.filter.conditions.length === 0
  );
}
export type RegisterView = {
  query: RegisterQuery;
  columns: string[];
  as_of: string;
  show_extra_balances?: boolean;
};
export type RegisterSettings = {
  views: Record<string, RegisterView>;
  saved_filters: { id: string; name: string; query: RegisterQuery }[];
};
export type SettingsResponse = {
  settings: RegisterSettings;
  revision: number;
  epoch: string;
};
