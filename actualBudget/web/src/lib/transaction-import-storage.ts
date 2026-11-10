import { uuid } from "./api";
import { validClock } from "./edit-clock";
import type { PendingMutation } from "./register-api";
export interface FileOptions {
  has_header_row: boolean;
  delimiter: string;
  encoding: string;
  skip_start_lines: number;
  skip_end_lines: number;
  date_format: string;
  split_mode: boolean;
  in_out_mode: boolean;
  out_value: string;
  flip_amount: boolean;
  multiplier: string;
  import_notes: boolean;
  clear_on_import: boolean;
  reconcile: boolean;
  reimport_deleted: boolean;
  fallback_missing_payee_to_memo: boolean;
  swap_payee_and_memo: boolean;
}
export interface FileChoice {
  row: number;
  action: "auto" | "add" | "skip" | "merge";
  match_id?: string;
}
export interface ImportDraft {
  budgetId: string;
  revision: string;
  accountId: string;
  epoch: string;
  defaultsLoaded?: boolean;
  filename: string;
  original: Blob;
  uploadKey: string;
  stageId?: string;
  options: FileOptions;
  mapping: Record<string, string | null>;
  remember: boolean;
  numberDrafts?: Record<string, string>;
  firstRow?: number;
  lastRow?: number;
  choices: FileChoice[];
  pending?: PendingMutation;
}
function validate(
  value: ImportDraft | undefined,
  budgetId: string,
): ImportDraft | null {
  if (!value) return null;
  if (
    value.budgetId !== budgetId ||
    typeof value.revision !== "string" ||
    typeof value.accountId !== "string" ||
    typeof value.epoch !== "string" ||
    typeof value.filename !== "string" ||
    !(value.original instanceof Blob) ||
    typeof value.uploadKey !== "string" ||
    !value.options ||
    !value.mapping ||
    !Array.isArray(value.choices) ||
    (value.pending &&
      (value.pending.path !==
        `/budgets/${budgetId}/transaction-imports/${value.stageId}/commit` ||
        value.pending.method !== "POST" ||
        !validClock(value.pending.edit_clock) ||
        !value.pending.historyOwner ||
        value.pending.budgetEpoch !== value.epoch))
  )
    throw new Error(
      "Saved file import is unreadable. Keep browser data for recovery.",
    );
  return value;
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("actualBudget.transaction-import.v1", 1);
    open.onupgradeneeded = () =>
      open.result.createObjectStore("drafts", { keyPath: "budgetId" });
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}
export async function readImport(
  budgetId: string,
): Promise<ImportDraft | null> {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const read = db.transaction("drafts").objectStore("drafts").get(budgetId);
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
export async function writeImport(
  budgetId: string,
  previous: ImportDraft | null,
  next: ImportDraft | null,
): Promise<ImportDraft | null> {
  const db = await database(),
    saved = next ? { ...next, revision: uuid() } : null;
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("drafts", "readwrite"),
        store = tx.objectStore("drafts"),
        read = store.get(budgetId);
      let error: unknown;
      read.onsuccess = () => {
        try {
          const existing = validate(read.result, budgetId);
          if ((existing?.revision ?? null) !== (previous?.revision ?? null))
            throw new Error(
              "Another tab changed this import. Reload to recover its saved request.",
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
