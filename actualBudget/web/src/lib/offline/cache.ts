import { request, RequestError, type Account, type Budget } from "../api";
import { changes, type Reference, type RegisterPage } from "../register-api";
import type { SharedPreferences } from "../preferences/types";
import { validShared } from "../preferences/validation";
import {
  readValue,
  writeValue,
  OfflineStateError,
  transaction,
  result,
} from "./storage";
import { setReachable, offlineStatus, emit } from "./events";
export interface CatalogCache {
  version: 1;
  origin: string;
  budgets: Budget[];
  savedAt: string;
}
export interface BudgetCache {
  version: 1;
  origin: string;
  budgetId: string;
  accounts: Account[];
  payees: Reference[];
  categories: Reference[];
  preferences: SharedPreferences;
  epoch: string;
  generation: number;
  snapshot: string;
  savedAt: string;
}
export interface ViewCache {
  version: 1;
  origin: string;
  budgetId: string;
  accountId: string | null;
  asOf: string;
  page: RegisterPage;
  savedAt: string;
}
const stamp = () => new Date().toISOString();
export async function readCatalog(): Promise<CatalogCache | null> {
  const value = await readValue<CatalogCache>("catalog");
  if (!value) return null;
  if (
    value.version !== 1 ||
    value.origin !== location.origin ||
    !Array.isArray(value.budgets)
  )
    throw new OfflineStateError(
      "Saved budget list is unreadable. Reconnect to refresh it.",
    );
  return value;
}
export async function saveCatalog(budgets: Budget[]) {
  const existing = await readCatalog().catch(() => null);
  if (existing && JSON.stringify(existing.budgets) === JSON.stringify(budgets))
    return;
  await writeValue("catalog", {
    version: 1,
    origin: location.origin,
    budgets,
    savedAt: stamp(),
  } satisfies CatalogCache);
}
export async function readBudget(id: string): Promise<BudgetCache | null> {
  const value = await readValue<BudgetCache>(`budget:${id}`);
  if (!value) return null;
  if (
    value.version !== 1 ||
    value.origin !== location.origin ||
    value.budgetId !== id ||
    !validShared(value.preferences) ||
    !Array.isArray(value.accounts) ||
    !Array.isArray(value.payees) ||
    !Array.isArray(value.categories) ||
    typeof value.epoch !== "string" ||
    !/^[a-f0-9]{32}$/.test(value.epoch) ||
    !Number.isSafeInteger(value.generation) ||
    value.generation < 0 ||
    typeof value.snapshot !== "string" ||
    !/^c1[a-f0-9]{48}$/.test(value.snapshot)
  )
    throw new OfflineStateError(
      "Saved budget references are unreadable. Reconnect before recording expenses.",
    );
  return value;
}
async function referencePages<T>(
  budget: string,
  kind: string,
  snapshot: string,
): Promise<T[]> {
  const items: T[] = [];
  let offset: number | null = 0,
    cursor: string | null = null;
  do {
    const query: URLSearchParams = new URLSearchParams({
      snapshot,
      limit: "1000",
      offset: String(offset ?? 0),
    });
    if (cursor) query.set("page_cursor", cursor);
    const page: {
      data: T[];
      next_offset?: number | null;
      next_page_cursor?: string | null;
    } = await request(`/budgets/${budget}/${kind}?${query}`);
    items.push(...page.data);
    offset = page.next_offset ?? null;
    cursor = page.next_page_cursor ?? null;
  } while (offset !== null || cursor);
  return items;
}
const refreshing = new Map<string, Promise<BudgetCache>>();
export function refreshBudget(id: string): Promise<BudgetCache> {
  const active = refreshing.get(id);
  if (active) return active;
  const task = refreshBudgetData(id).finally(() => refreshing.delete(id));
  refreshing.set(id, task);
  return task;
}
async function saveBudget(value: BudgetCache): Promise<BudgetCache> {
  try {
    return await transaction(["cache"], "readwrite", async (tx) => {
      const store = tx.objectStore("cache");
      const previous = await result<BudgetCache | undefined>(
        store.get(`budget:${value.budgetId}`),
      );
      if (
        previous?.version === 1 &&
        (previous.generation > value.generation ||
          (previous.epoch === value.epoch &&
            previous.snapshot > value.snapshot))
      )
        return previous;
      await result(store.put(value, `budget:${value.budgetId}`));
      return value;
    });
  } catch (error) {
    offlineStatus.error =
      error instanceof Error
        ? error.message
        : "Offline references could not be saved.";
    emit(false);
    return { ...value, savedAt: "" };
  }
}
async function refreshBudgetData(id: string): Promise<BudgetCache> {
  for (let attempt = 0; attempt < 3; attempt++) {
    const { data: checkpoint } = await request<{
      data: { snapshot: string; epoch: string; generation: number };
    }>(`/budgets/${id}/checkpoint`);
    const cached = await readBudget(id).catch(() => null);
    if (cached?.snapshot === checkpoint.snapshot) {
      setReachable(true);
      return cached;
    }
    if (cached && cached.epoch === checkpoint.epoch) {
      try {
        const next = structuredClone(cached);
        let complete = false;
        for (let pages = 0; pages < 100; pages++) {
          const page = await changes(id, next.snapshot);
          for (const change of page.data) {
            if (change.entity === "preferences") {
              if (!validShared(change.data))
                throw new Error("Invalid changed preferences");
              next.preferences = change.data;
            } else if (
              ["accounts", "payees", "categories"].includes(change.entity)
            ) {
              const key = change.entity as "accounts" | "payees" | "categories";
              const value = change.data as Account & { deleted?: boolean };
              if (
                !value ||
                value.id !== change.id ||
                (!value.deleted && typeof value.name !== "string")
              )
                throw new Error("Unsupported reference change");
              const rows = next[key] as (Account | Reference)[];
              const at = rows.findIndex((item) => item.id === change.id);
              if (value.deleted) {
                if (at >= 0) rows.splice(at, 1);
              } else if (at >= 0) rows[at] = value;
              else rows.push(value);
            } else if (change.entity !== "transactions")
              throw new Error(
                "Unsupported change entity; reload references before advancing checkpoint.",
              );
          }
          next.snapshot = page.next_cursor;
          if (!page.has_more) {
            complete = true;
            break;
          }
        }
        if (complete) {
          next.savedAt = stamp();
          setReachable(true);
          return await saveBudget(next);
        }
      } catch (error) {
        if (
          error instanceof RequestError &&
          !["cursor_invalid"].includes(error.code ?? "")
        )
          throw error;
      }
    }
    try {
      const [accounts, payees, categories, prefs] = await Promise.all([
        request<{ data: Account[] }>(
          `/budgets/${id}/accounts?${new URLSearchParams({ snapshot: checkpoint.snapshot })}`,
        ).then((response) => response.data),
        referencePages<Reference>(id, "payees", checkpoint.snapshot),
        referencePages<Reference>(id, "categories", checkpoint.snapshot),
        request<{ data: SharedPreferences }>(
          `/budgets/${id}/preferences?${new URLSearchParams({ snapshot: checkpoint.snapshot })}`,
        ),
      ]);
      if (!validShared(prefs.data))
        throw new Error("The server returned invalid display preferences.");
      setReachable(true);
      return await saveBudget({
        version: 1,
        origin: location.origin,
        budgetId: id,
        accounts,
        payees,
        categories,
        preferences: prefs.data,
        epoch: checkpoint.epoch,
        generation: checkpoint.generation,
        snapshot: checkpoint.snapshot,
        savedAt: stamp(),
      });
    } catch (error) {
      if (
        !(error instanceof RequestError) ||
        error.code !== "snapshot_changed" ||
        attempt === 2
      )
        throw error;
    }
  }
  throw new Error(
    "The budget changed while loading. Retry to get a consistent view.",
  );
}
const viewKey = (budget: string, account: string | null, asOf: string) =>
  `view:${budget}:${account ?? "all"}:${asOf}`;
export async function readView(
  budget: string,
  account: string | null,
  asOf: string,
): Promise<ViewCache | null> {
  const value = await readValue<ViewCache>(viewKey(budget, account, asOf));
  if (!value) return null;
  if (
    value.version !== 1 ||
    value.origin !== location.origin ||
    value.budgetId !== budget ||
    value.accountId !== account ||
    value.asOf !== asOf ||
    !Array.isArray(value.page?.data) ||
    typeof value.page?.snapshot !== "string" ||
    !/^c1[a-f0-9]{48}$/.test(value.page.snapshot) ||
    !value.page.data.every(
      (row) =>
        Number.isSafeInteger(row.amount) &&
        typeof row.date === "string" &&
        typeof row.id === "string",
    ) ||
    ![
      value.page.balances?.total,
      value.page.balances?.cleared,
      value.page.balances?.uncleared,
    ].every(Number.isSafeInteger)
  )
    throw new OfflineStateError(
      "Saved transaction view is unreadable. Reconnect to refresh it.",
    );
  return value;
}
export async function saveView(
  budget: string,
  account: string | null,
  page: RegisterPage,
): Promise<string> {
  const existing = await readView(budget, account, page.as_of ?? "").catch(
    () => null,
  );
  if (existing && JSON.stringify(existing.page) === JSON.stringify(page))
    return existing.savedAt;
  const savedAt = stamp();
  await writeValue(viewKey(budget, account, page.as_of ?? ""), {
    version: 1,
    origin: location.origin,
    budgetId: budget,
    accountId: account,
    asOf: page.as_of ?? "",
    page,
    savedAt,
  } satisfies ViewCache);
  return savedAt;
}
