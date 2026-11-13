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
  group: "Category group",
  category: "Category",
  amount: "Amount",
  payment: "Payment",
  deposit: "Deposit",
  balance: "Running balance",
};
export const defaultColumns = [
  "date",
  "payee",
  "category",
  "amount",
  "balance",
];
// Older saved/imported views may have separate inflow and outflow columns.
export function transactionColumns(columns: string[]): string[] {
  const normalized = [
    ...new Set(
      columns
        .filter((column) => !["account", "status", "notes"].includes(column))
        .map((column) =>
          column === "payment" || column === "deposit" ? "amount" : column,
        ),
    ),
  ];
  if (!normalized.includes("amount")) normalized.push("amount");
  return normalized;
}
export function manualQuery(query: RegisterQuery): RegisterQuery {
  return {
    ...query,
    hide_cleared: false,
    hide_reconciled: false,
    sort: ["cleared", "reconciled", "notes"].includes(query.sort)
      ? "date"
      : query.sort,
    filter: {
      ...query.filter,
      conditions: query.filter.conditions.filter(
        (condition) =>
          !["cleared", "reconciled", "notes"].includes(condition.field) &&
          !["hasTags", "hasAnyTag"].includes(condition.op),
      ),
    },
  };
}
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

export function categoryMonthQuery(
  category: string,
  month: string,
): RegisterQuery {
  const [year, number] = month.split("-").map(Number);
  const lastDay = new Date(Date.UTC(year, number, 0)).getUTCDate();
  return {
    ...defaultQuery(),
    filter: {
      op: "and",
      conditions: [
        { field: "category", op: "eq", value: category },
        { field: "date", op: "gte", value: month + "-01" },
        { field: "date", op: "lte", value: month + "-" + lastDay },
      ],
    },
  };
}
