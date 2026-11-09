import { uuid } from "./api";
import { validClock } from "./edit-clock";
import type { AmountFormat } from "./money";
import type { PendingMutation, Transaction } from "./register-api";
export class SplitOwnershipError extends Error {
  name = "SplitOwnershipError";
}
export interface SplitDraft {
  budgetId: string;
  revision: string;
  parent: Transaction;
  snapshot: string;
  epoch: string;
  format: AmountFormat;
  children: {
    id: string;
    amount: string;
    payee_id: string;
    category_id: string;
    notes: string;
  }[];
  fields: {
    account_id: string;
    amount: string;
    date: string;
    payee_id: string;
    notes: string;
    cleared: boolean;
  };
  pending: PendingMutation | null;
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("actualBudget.split-drafts.v1", 1);
    open.onupgradeneeded = () =>
      open.result.createObjectStore("drafts", { keyPath: "budgetId" });
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}
const identifier = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
function validate(value: SplitDraft | undefined, budgetId: string) {
  if (
    value &&
    (value.budgetId !== budgetId ||
      !identifier(value.revision) ||
      !identifier(value.parent?.id) ||
      !Number.isSafeInteger(value.parent?.amount) ||
      !value.format ||
      ![0, 2].includes(value.format.decimal_places) ||
      ![
        "comma-dot",
        "dot-comma",
        "space-comma",
        "apostrophe-dot",
        "comma-dot-in",
      ].includes(value.format.number_format) ||
      !value.fields ||
      !identifier(value.fields.account_id) ||
      typeof value.fields.amount !== "string" ||
      typeof value.fields.date !== "string" ||
      typeof value.fields.notes !== "string" ||
      typeof value.fields.cleared !== "boolean" ||
      !/^[0-9a-f]{32}$/.test(value.epoch) ||
      !Array.isArray(value.children) ||
      value.children.some(
        (c) =>
          !c ||
          !identifier(c.id) ||
          typeof c.amount !== "string" ||
          typeof c.notes !== "string" ||
          ![c.payee_id, c.category_id].every(
            (id) => id === "" || identifier(id),
          ),
      ) ||
      typeof value.snapshot !== "string" ||
      value.snapshot.length !== 50 ||
      (value.pending &&
        (value.pending.path !==
          `/budgets/${budgetId}/transactions/${value.parent.id}/splits` ||
          value.pending.method !== "POST" ||
          !identifier(value.pending.key) ||
          !value.pending.edit_clock ||
          !validClock(value.pending.edit_clock) ||
          value.pending.budgetEpoch !== value.epoch ||
          !value.pending.body)))
  )
    throw new Error(
      "Saved split work is unreadable. Keep browser data and retry recovery.",
    );
  return value ?? null;
}
export async function readSplit(budgetId: string): Promise<SplitDraft | null> {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const request = db
        .transaction("drafts")
        .objectStore("drafts")
        .get(budgetId);
      request.onsuccess = () => {
        try {
          resolve(validate(request.result, budgetId));
        } catch (e) {
          reject(e);
        }
      };
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}
/** IndexedDB serializes compare-and-write across tabs, including plain HTTP. */
export async function writeSplit(
  budgetId: string,
  previous: SplitDraft | null,
  next: SplitDraft | null,
): Promise<SplitDraft | null> {
  const db = await database();
  const result = next ? { ...next, revision: uuid() } : null;
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("drafts", "readwrite"),
        store = tx.objectStore("drafts"),
        read = store.get(budgetId);
      let problem: unknown;
      read.onsuccess = () => {
        try {
          const existing = validate(read.result, budgetId);
          if ((existing?.revision ?? null) !== (previous?.revision ?? null))
            throw new SplitOwnershipError(
              "Another tab changed this split draft. Reload to recover its saved work.",
            );
          if (result) store.put(result);
          else store.delete(budgetId);
        } catch (e) {
          problem = e;
          tx.abort();
        }
      };
      tx.oncomplete = () => resolve();
      tx.onabort = () => reject(problem ?? tx.error);
      tx.onerror = () => reject(tx.error);
    });
    return result;
  } finally {
    db.close();
  }
}
