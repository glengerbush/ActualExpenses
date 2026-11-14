import { captureAmountFormat, type AmountFormat } from "../money";
import { validDate } from "../dates";
import { uuid } from "../api";
import { validClock, resetCorrectedClock } from "../edit-clock";
import {
  sameMutation,
  type PendingMutation,
  type Transaction,
} from "../register-api";
import { validPending, type TransactionDraft } from "../register-storage";
import { transaction, result, OfflineStateError } from "./storage";
import { emit } from "./events";
export interface OutboxEntry {
  version: 1;
  origin: string;
  budgetId: string;
  key: string;
  revision: string;
  row: Transaction;
  draft: TransactionDraft | null;
  pending: PendingMutation;
  format: AmountFormat;
  createdAt: string;
  offlineCreated: boolean;
  attempted: boolean;
  dependsOn?: string;
  claim?: { token: string; until: number; owner?: string };
  blocked?: string;
  cancelRequested?: boolean;
}
export interface DeliveryReceipt {
  version: 1;
  budgetId: string;
  key: string;
  at: string;
  message: string;
  row?: Transaction;
  pending?: PendingMutation;
  draft?: TransactionDraft | null;
}
const owner = (() => {
  try {
    const key = "actualBudget.deliveryTab.v1";
    const saved = sessionStorage.getItem(key);
    if (saved) return saved;
    const value = uuid();
    sessionStorage.setItem(key, value);
    return value;
  } catch {
    return uuid();
  }
})();
const canonicalUuid = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
function validate(value: unknown): asserts value is OutboxEntry {
  const e = value as OutboxEntry;
  if (
    !e ||
    e.version !== 1 ||
    e.origin !== location.origin ||
    !canonicalUuid(e.budgetId) ||
    !canonicalUuid(e.key) ||
    !canonicalUuid(e.row?.id) ||
    e.pending?.key !== e.key ||
    !validPending(e.pending, e.budgetId) ||
    (e.pending.edit_clock !== undefined && !validClock(e.pending.edit_clock)) ||
    !Number.isSafeInteger(e.row?.amount) ||
    !validDate(e.row?.date) ||
    typeof e.row?.notes !== "string" ||
    !e.format ||
    ![0, 2].includes(e.format.decimal_places) ||
    ![
      "comma-dot",
      "dot-comma",
      "space-comma",
      "apostrophe-dot",
      "comma-dot-in",
    ].includes(e.format.number_format) ||
    typeof e.attempted !== "boolean" ||
    !canonicalUuid(e.revision)
  )
    throw new OfflineStateError(
      "Saved pending work is invalid. It has been preserved; no request was sent.",
    );
}
async function all(tx: IDBTransaction): Promise<OutboxEntry[]> {
  const rows = await result<unknown[]>(tx.objectStore("operations").getAll());
  rows.forEach(validate);
  // IndexedDB returns primary-key order, which says nothing about local intent.
  // A dependent edit always follows its prerequisite, even after clock correction.
  const pending = rows as OutboxEntry[];
  pending.sort(
    (a, b) =>
      (a.pending.edit_clock ?? a.createdAt).localeCompare(
        b.pending.edit_clock ?? b.createdAt,
      ) || a.key.localeCompare(b.key),
  );
  const byKey = new Map(pending.map((entry) => [entry.key, entry]));
  const ordered: OutboxEntry[] = [];
  const visited = new Set<string>();
  const visiting = new Set<string>();
  function visit(entry: OutboxEntry) {
    if (visited.has(entry.key)) return;
    if (visiting.has(entry.key))
      throw new OfflineStateError(
        "Saved pending work has circular dependencies. It has been preserved; no request was sent.",
      );
    visiting.add(entry.key);
    const parent = entry.dependsOn && byKey.get(entry.dependsOn);
    if (parent) visit(parent);
    visiting.delete(entry.key);
    visited.add(entry.key);
    ordered.push(entry);
  }
  pending.forEach(visit);
  return ordered;
}
export function entries(budget?: string): Promise<OutboxEntry[]> {
  return transaction(["operations"], "readonly", async (tx) =>
    (await all(tx)).filter((e) => !budget || e.budgetId === budget),
  );
}
export async function enqueue(input: {
  budgetId: string;
  pending: PendingMutation;
  row: Transaction;
  draft?: TransactionDraft | null;
  offlineCreated: boolean;
  replace?: {
    key: string;
    revision: string;
    baseline: Transaction | null;
    empty: boolean;
  };
  afterAttempt?: { pending: PendingMutation; original: Transaction };
  dependsOn?: string;
}): Promise<OutboxEntry> {
  const entry: OutboxEntry = {
    version: 1,
    origin: location.origin,
    budgetId: input.budgetId,
    key: input.pending.key,
    revision: uuid(),
    pending: input.pending,
    format: input.draft?.format ?? captureAmountFormat(),
    row: input.row,
    draft: input.draft ?? null,
    offlineCreated: input.offlineCreated,
    attempted: false,
    createdAt: new Date().toISOString(),
    dependsOn: input.dependsOn,
  };
  validate(entry);
  const saved = await transaction(
    ["operations", "receipts"],
    "readwrite",
    async (tx) => {
      const store = tx.objectStore("operations");
      const existing = await result<OutboxEntry | undefined>(
        store.get(entry.key),
      );
      if (existing) {
        validate(existing);
        if (
          !sameMutation(existing.pending, entry.pending) &&
          !sameMutation(existing.pending, input.afterAttempt?.pending)
        )
          throw new OfflineStateError(
            "This operation already has different saved details. Reload pending work.",
          );
        return existing;
      }
      const acknowledged = await result<DeliveryReceipt | undefined>(
        tx.objectStore("receipts").get(entry.key),
      );
      if (acknowledged) {
        if (
          !sameMutation(acknowledged.pending, entry.pending) &&
          !sameMutation(acknowledged.pending, input.afterAttempt?.pending)
        )
          throw new OfflineStateError(
            "This operation has a receipt for different details. The edited draft has been kept.",
          );
        return { ...entry, pending: acknowledged.pending! };
      }
      if (input.replace) {
        const original = await result<OutboxEntry | undefined>(
          store.get(input.replace.key),
        );
        if (original) {
          validate(original);
          entry.offlineCreated = original.offlineCreated;
        }
        if (original && !original.attempted) {
          if (original.revision !== input.replace.revision)
            throw new OfflineStateError(
              "This pending expense changed in another tab. Your edited draft has been kept; review the current pending expense before replacing it.",
            );
          entry.dependsOn = original.dependsOn;
          if (entry.draft)
            entry.draft = {
              ...entry.draft,
              original: input.replace.baseline,
              pending: entry.pending,
            };
          await result(store.delete(original.key));
          if (input.replace.empty) {
            await result(
              tx.objectStore("receipts").put(
                {
                  version: 1,
                  budgetId: entry.budgetId,
                  key: entry.key,
                  at: new Date().toISOString(),
                  message: "Unsent edit removed.",
                  pending: entry.pending,
                  draft: entry.draft,
                } satisfies DeliveryReceipt,
                entry.key,
              ),
            );
            return entry;
          }
        } else {
          const receipt = original
            ? undefined
            : await result<DeliveryReceipt | undefined>(
                tx.objectStore("receipts").get(input.replace.key),
              );
          if (!input.afterAttempt || (!original && !receipt?.row))
            throw new OfflineStateError(
              "The original pending expense was removed or canceled. Your edited draft has been kept.",
            );
          entry.pending = input.afterAttempt.pending;
          entry.dependsOn = original?.key;
          if (entry.draft)
            entry.draft = {
              ...entry.draft,
              original: input.afterAttempt.original,
              pending: entry.pending,
            };
        }
      }
      validate(entry);
      await result(store.put(entry, entry.key));
      return entry;
    },
  );
  emit();
  return saved;
}
export async function removeUnsent(entry: OutboxEntry): Promise<void> {
  await transaction(["operations"], "readwrite", async (tx) => {
    const store = tx.objectStore("operations");
    const current = await result<OutboxEntry | undefined>(store.get(entry.key));
    if (!current) return;
    validate(current);
    if (current.revision !== entry.revision || current.attempted)
      throw new OfflineStateError(
        "This expense started syncing or changed elsewhere. Refresh pending work before deleting it.",
      );
    await result(store.delete(entry.key));
  });
  emit();
}
export async function claimNext(): Promise<OutboxEntry | null> {
  return transaction(["operations"], "readwrite", async (tx) => {
    const rows = await all(tx);
    const now = Date.now();
    const row = rows.find(
      (e) =>
        !e.blocked &&
        (!e.claim || e.claim.until <= now || e.claim.owner === owner) &&
        (!e.dependsOn || !rows.some((parent) => parent.key === e.dependsOn)),
    );
    if (!row) return null;
    const claimed = {
      ...row,
      attempted: true,
      claim: { token: uuid(), until: now + 60_000, owner },
    };
    await result(tx.objectStore("operations").put(claimed, claimed.key));
    return claimed;
  });
}
export async function settle(
  entry: OutboxEntry,
  receipt: DeliveryReceipt,
): Promise<void> {
  const canceled = await transaction(
    ["operations", "receipts"],
    "readwrite",
    async (tx) => {
      const store = tx.objectStore("operations");
      const current = await result<OutboxEntry | undefined>(
        store.get(entry.key),
      );
      if (!current) return;
      validate(current);
      if (!sameMutation(current.pending, entry.pending))
        throw new OfflineStateError(
          "Pending operation changed while syncing. Original result retained on server.",
        );
      if (
        current.cancelRequested &&
        !entry.cancelRequested &&
        receipt.message === "Expense synced."
      )
        receipt.message =
          "This change had already been saved. Its confirmed result was recovered.";
      await result(
        tx
          .objectStore("receipts")
          .put(
            { ...receipt, pending: entry.pending, draft: entry.draft },
            entry.key,
          ),
      );
      await result(store.delete(entry.key));
      return current.cancelRequested;
    },
  );
  if (canceled) {
    try {
      resetCorrectedClock();
    } catch {
      /* A future edit surfaces unavailable clock storage; the receipt is already durable. */
    }
  }
  emit();
}
export async function release(
  entry: OutboxEntry,
  message?: string,
): Promise<void> {
  await transaction(["operations"], "readwrite", async (tx) => {
    const store = tx.objectStore("operations");
    const current = await result<OutboxEntry | undefined>(store.get(entry.key));
    if (!current || current.claim?.token !== entry.claim?.token) return;
    await result(
      store.put({ ...current, claim: undefined, blocked: message }, entry.key),
    );
  });
  emit();
}
export async function retryEntry(
  entry: OutboxEntry,
  cancel = false,
): Promise<void> {
  await transaction(["operations"], "readwrite", async (tx) => {
    const store = tx.objectStore("operations");
    const current = await result<OutboxEntry | undefined>(store.get(entry.key));
    if (!current) return;
    validate(current);
    await result(
      store.put(
        {
          ...current,
          blocked: undefined,
          claim: undefined,
          cancelRequested: cancel || current.cancelRequested,
        },
        entry.key,
      ),
    );
  });
  emit();
}
export function receipts(budget: string): Promise<DeliveryReceipt[]> {
  return transaction(["receipts"], "readonly", async (tx) =>
    (await result<DeliveryReceipt[]>(tx.objectStore("receipts").getAll()))
      .filter((r) => r.budgetId === budget)
      .sort((a, b) => a.at.localeCompare(b.at) || a.key.localeCompare(b.key)),
  );
}

export async function deleteOfflineExpense(
  entry: OutboxEntry,
  deletion: PendingMutation,
): Promise<void> {
  await transaction(["operations"], "readwrite", async (tx) => {
    const store = tx.objectStore("operations");
    const rows = await all(tx);
    const current = rows.find((e) => e.key === entry.key);
    if (!current || current.revision !== entry.revision)
      throw new OfflineStateError(
        "Pending work changed in another tab. Refresh before deleting it.",
      );
    const related = rows.filter(
      (e) => e.budgetId === entry.budgetId && e.row.id === entry.row.id,
    );
    const attempted = related.filter((e) => e.attempted);
    for (const item of related.filter((e) => !e.attempted))
      await result(store.delete(item.key));
    if (attempted.length) {
      const next: OutboxEntry = {
        ...entry,
        key: deletion.key,
        revision: uuid(),
        pending: deletion,
        draft: null,
        attempted: false,
        claim: undefined,
        blocked: undefined,
        cancelRequested: false,
        dependsOn: attempted.at(-1)!.key,
        createdAt: new Date().toISOString(),
      };
      validate(next);
      await result(store.put(next, next.key));
    }
  });
  emit();
}
