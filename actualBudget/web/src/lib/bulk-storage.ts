import { uuid } from "./api";
import { validClock } from "./edit-clock";
import type { PendingMutation } from "./register-api";
import type { AmountFormat } from "./money";
import type { DateFormat } from "./preferences/types";
export interface BulkIntent {
  budgetId: string;
  revision: string;
  pending: PendingMutation;
  description: string;
  format: AmountFormat;
  dateFormat: DateFormat;
}
function validate(
  value: BulkIntent | undefined,
  budgetId: string,
): BulkIntent | null {
  if (
    value &&
    (value.budgetId !== budgetId ||
      typeof value.revision !== "string" ||
      typeof value.description !== "string" ||
      !value.format ||
      !value.dateFormat ||
      !value.pending ||
      value.pending.method !== "POST" ||
      !value.pending.historyOwner ||
      !validClock(value.pending.edit_clock) ||
      ![
        `/budgets/${budgetId}/transactions/bulk`,
        `/budgets/${budgetId}/history/undo`,
        `/budgets/${budgetId}/history/redo`,
      ].includes(value.pending.path))
  )
    throw new Error(
      "Saved bulk action is unreadable. Keep browser data for recovery.",
    );
  return value ?? null;
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("actualBudget.bulk.v1", 1);
    open.onupgradeneeded = () =>
      open.result.createObjectStore("intents", { keyPath: "budgetId" });
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}
export async function readBulk(budgetId: string): Promise<BulkIntent | null> {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const read = db
        .transaction("intents")
        .objectStore("intents")
        .get(budgetId);
      read.onsuccess = () => {
        try {
          resolve(validate(read.result, budgetId));
        } catch (e) {
          reject(e);
        }
      };
      read.onerror = () => reject(read.error);
    });
  } finally {
    db.close();
  }
}
/** Serialized compare-and-write across tabs. Update UI checkpoints only after commit. */
export async function writeBulk(
  budgetId: string,
  previous: BulkIntent | null,
  next: BulkIntent | null,
): Promise<BulkIntent | null> {
  const db = await database();
  const saved = next ? { ...next, revision: uuid() } : null;
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("intents", "readwrite"),
        store = tx.objectStore("intents"),
        read = store.get(budgetId);
      let error: unknown;
      read.onsuccess = () => {
        try {
          const existing = validate(read.result, budgetId);
          if ((existing?.revision ?? null) !== (previous?.revision ?? null))
            throw new Error(
              "Another tab changed the saved bulk action. Reload to recover its result.",
            );
          if (saved) store.put(saved);
          else store.delete(budgetId);
        } catch (e) {
          error = e;
          tx.abort();
        }
      };
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(error ?? tx.error);
      tx.onabort = () => reject(error ?? tx.error);
    });
    return saved;
  } finally {
    db.close();
  }
}
