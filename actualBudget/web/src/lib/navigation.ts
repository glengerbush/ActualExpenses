/** Delivered features register a real command here through App's command list. */
export interface NavigationCommand {
  id: string;
  label: string;
  keywords?: string;
  disabled?: boolean;
  run: () => void | Promise<void>;
}
export const rememberedViews = [
  "references",
  "planning",
  "transaction-files",
  "rules",
  "schedules",
  "reports",
  "settings",
  "backups",
] as const;
export type RememberedView = (typeof rememberedViews)[number];
const key = (budget: string) => `actualBudget.navigation.v1.${budget}`;
export function readView(budget: string): RememberedView | null {
  try {
    const value = JSON.parse(localStorage.getItem(key(budget)) ?? "null");
    return value?.version === 1 && rememberedViews.includes(value.view)
      ? value.view
      : null;
  } catch {
    return null;
  }
}
export function rememberView(budget: string, view: string | null): void {
  try {
    if (rememberedViews.includes(view as RememberedView))
      localStorage.setItem(key(budget), JSON.stringify({ version: 1, view }));
    else localStorage.removeItem(key(budget));
  } catch {
    /* Navigation remains usable when browser storage is unavailable. */
  }
}

const screens = [
  "references",
  "organize",
  "account",
  "planning",
  "transaction-files",
  "rules",
  "schedules",
  "reports",
  "settings",
  "import",
  "backups",
  "restore",
  "recovery",
  "pending",
] as const;
export type Screen = (typeof screens)[number] | null;
export const reportTabs = ["dashboards", "custom", "crossover"];
export interface BrowserRoute {
  budgetId: string;
  screen: Screen;
  report: string | null;
  source: "actual" | "ynab4";
  transactions?: { category: string; month: string } | null;
}
export function readBrowserRoute(): BrowserRoute | null {
  const query = new URL(location.href).searchParams;
  if (!query.has("view")) return null;
  const view = query.get("view");
  if (
    view !== "transactions" &&
    view !== "budgets" &&
    !screens.includes(view as Exclude<Screen, null>)
  )
    return null;
  const budgetId = query.get("budget") ?? "";
  if (budgetId && !/^[0-9a-f-]{36}$/i.test(budgetId)) return null;
  const report = query.get("report");
  return {
    budgetId,
    transactions:
      view === "transactions" &&
      /^[0-9a-f-]{36}$/i.test(query.get("category") ?? "") &&
      /^\d{4}-(0[1-9]|1[0-2])$/.test(query.get("month") ?? "")
        ? { category: query.get("category")!, month: query.get("month")! }
        : null,
    screen: screens.includes(view as Exclude<Screen, null>)
      ? (view as Screen)
      : null,
    report: report && reportTabs.includes(report) ? report : null,
    source: query.get("source") === "ynab4" ? "ynab4" : "actual",
  };
}
export function writeBrowserRoute(route: BrowserRoute, replace = false) {
  const url = new URL(location.href);
  url.hash = "";
  url.searchParams.set("view", route.screen ?? "transactions");
  if (route.budgetId) url.searchParams.set("budget", route.budgetId);
  else url.searchParams.delete("budget");
  if (route.screen === "reports" && route.report)
    url.searchParams.set("report", route.report);
  else url.searchParams.delete("report");
  if (route.screen === "import") url.searchParams.set("source", route.source);
  else url.searchParams.delete("source");
  if (!route.screen && route.budgetId && route.transactions) {
    url.searchParams.set("category", route.transactions.category);
    url.searchParams.set("month", route.transactions.month);
  } else {
    url.searchParams.delete("category");
    url.searchParams.delete("month");
  }
  if (!replace && url.href === location.href) return;
  history[replace ? "replaceState" : "pushState"](null, "", url);
}
