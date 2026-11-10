import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { uuid } from "./api";
import { captureAmountFormat, today, type AmountFormat } from "./money";
import { validClock } from "./edit-clock";
export interface BankConnection {
  id: string;
  provider: "simplefin";
  name: string;
  state:
    | "claim_pending"
    | "claim_inflight"
    | "connected"
    | "claim_failed"
    | "credentials_missing";
  generation: string;
  revision: number;
  error_code: string | null;
  message: string | null;
  last_success: string | null;
  deleted: boolean;
}
export interface BankDiagnostic {
  code: string;
  message: string;
  count?: number;
}
export interface BankRemote {
  id: string;
  connection_id: string;
  generation: string;
  remote_connection_id: string;
  remote_id: string;
  institution: string;
  name: string;
  currency: string;
  balance: string;
  available_balance: string | null;
  balance_date: number;
  diagnostics: BankDiagnostic[];
  deleted: boolean;
}
export interface BankLink {
  id: string;
  connection_id: string;
  remote_account_id: string;
  connection_generation: string;
  revision: number;
  initial_sync: boolean;
  opening_mode: "auto" | "none" | "custom";
  opening_amount: number | null;
  opening_date: string | null;
  last_sync_end: string | null;
  deleted: boolean;
}
export interface BankState {
  connections: BankConnection[];
  accounts: BankRemote[];
  links: BankLink[];
  epoch: string;
  snapshot: string;
  clock: string;
}
export interface BankMapping {
  date: string;
  payee: string;
  notes: string;
}
export interface BankPreferences {
  import_transactions: boolean;
  import_pending: boolean;
  import_notes: boolean;
  update_dates: boolean;
  mappings: { payment: BankMapping; deposit: BankMapping };
}
export interface BankAccountView {
  link: BankLink | null;
  preferences: {
    settings: BankPreferences | null;
    raw_preferences: Record<string, unknown>;
    field_versions: Record<string, string>;
    error: string | null;
  };
  snapshot: string;
  epoch: string;
}
export const defaultBankPreferences = (): BankPreferences => ({
  import_transactions: true,
  import_pending: true,
  import_notes: true,
  update_dates: false,
  mappings: {
    payment: { date: "date", payee: "payeeName", notes: "notes" },
    deposit: { date: "date", payee: "payeeName", notes: "notes" },
  },
});
export interface BankFields {
  name: string;
  connectionId: string;
  accountId: string;
  remoteId: string;
  openingMode: "auto" | "none" | "custom";
  openingAmount: string;
  openingDate: string;
  startDate: string;
  endDate: string;
  format: AmountFormat;
  settings: BankPreferences;
  settingsAccount: string;
}
export interface BankPending {
  path: string;
  body: Record<string, unknown>;
  key: string;
  epoch: string;
  editClock?: string;
  owner?: string;
  credential: boolean;
  cancel: boolean;
  label: string;
}
export interface BankDraft {
  version: 1;
  budgetId: string;
  revision: string;
  epoch: string | null;
  fields: BankFields;
  pending: BankPending | null;
}
export function secretDigest(token: string): string {
  // Pure bundled SHA256: crypto.subtle is unavailable on ordinary LAN HTTP.
  return bytesToHex(sha256(new TextEncoder().encode(token)));
}
export function newBankFields(): BankFields {
  const end = today(),
    date = new Date(end + "T00:00:00Z");
  date.setUTCDate(date.getUTCDate() - 5);
  const next = new Date(end + "T00:00:00Z");
  next.setUTCDate(next.getUTCDate() + 1);
  return {
    name: "SimpleFIN",
    connectionId: "",
    accountId: "",
    remoteId: "",
    openingMode: "auto",
    openingAmount: "",
    openingDate: end,
    startDate: date.toISOString().slice(0, 10),
    endDate: next.toISOString().slice(0, 10),
    format: captureAmountFormat(),
    settings: defaultBankPreferences(),
    settingsAccount: "",
  };
}
const validId = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i.test(value);
function noSecrets(value: unknown): boolean {
  if (!value || typeof value !== "object") return true;
  return Object.entries(value).every(
    ([key, item]) =>
      !["setup_token", "access_url", "authorization"].includes(key) &&
      noSecrets(item),
  );
}
function validate(
  value: BankDraft | undefined,
  budgetId: string,
): BankDraft | null {
  if (!value) return null;
  const f = value.fields,
    p = value.pending;
  const mapping = (m: BankMapping | undefined) =>
    !!m &&
    [m.date, m.payee, m.notes].every(
      (v) => typeof v === "string" && v.length <= 120,
    );
  if (
    value.version !== 1 ||
    !validId(budgetId) ||
    value.budgetId !== budgetId ||
    !validId(value.revision) ||
    (value.epoch !== null && !/^[0-9a-f]{32}$/.test(value.epoch)) ||
    !f ||
    !noSecrets(value) ||
    ![
      f.name,
      f.connectionId,
      f.accountId,
      f.remoteId,
      f.openingAmount,
      f.openingDate,
      f.startDate,
      f.endDate,
      f.settingsAccount,
    ].every((v) => typeof v === "string" && v.length <= 8192) ||
    !["auto", "none", "custom"].includes(f.openingMode) ||
    !f.format ||
    ![0, 2].includes(f.format.decimal_places) ||
    !f.settings ||
    ![
      f.settings.import_transactions,
      f.settings.import_pending,
      f.settings.import_notes,
      f.settings.update_dates,
    ].every((v) => typeof v === "boolean") ||
    !mapping(f.settings.mappings?.payment) ||
    !mapping(f.settings.mappings?.deposit) ||
    (p &&
      (!validId(p.key) ||
        !/^[0-9a-f]{32}$/.test(p.epoch) ||
        typeof p.credential !== "boolean" ||
        typeof p.cancel !== "boolean" ||
        typeof p.label !== "string" ||
        !p.body ||
        typeof p.body !== "object" ||
        !(
          p.path === `/budgets/${budgetId}/banking/connections` ||
          new RegExp(
            `^/budgets/${budgetId}/(?:banking/connections|accounts)/[0-9a-f-]{36}/(?:banking/)?commands$`,
            "i",
          ).test(p.path)
        ) ||
        (p.editClock !== undefined && !validClock(p.editClock)) ||
        (p.owner !== undefined && !validId(p.owner)) ||
        p.credential !== Object.hasOwn(p.body, "setup_token_sha256") ||
        (p.credential &&
          !/^[0-9a-f]{64}$/.test(String(p.body.setup_token_sha256)))))
  )
    throw new Error(
      "Saved bank work is unreadable. Keep browser data for recovery; no request was sent.",
    );
  return value;
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open("actualBudget.banking.v1", 1);
    open.onupgradeneeded = () =>
      open.result.createObjectStore("drafts", { keyPath: "budgetId" });
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
    open.onblocked = () =>
      reject(Error("Close other tabs and reopen bank connections."));
  });
}
export async function readBankDraft(
  budgetId: string,
): Promise<BankDraft | null> {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const r = db.transaction("drafts").objectStore("drafts").get(budgetId);
      r.onsuccess = () => {
        try {
          resolve(validate(r.result, budgetId));
        } catch (e) {
          reject(e);
        }
      };
      r.onerror = () => reject(r.error);
    });
  } finally {
    db.close();
  }
}
export async function writeBankDraft(
  budgetId: string,
  previous: BankDraft | null,
  next: BankDraft | null,
): Promise<BankDraft | null> {
  const db = await database(),
    saved = next ? { ...next, revision: uuid() } : null;
  if (saved) validate(saved, budgetId);
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("drafts", "readwrite"),
        store = tx.objectStore("drafts"),
        r = store.get(budgetId);
      let error: unknown;
      r.onsuccess = () => {
        try {
          const old = validate(r.result, budgetId);
          if ((old?.revision ?? null) !== (previous?.revision ?? null))
            throw Error(
              "Another tab changed saved bank work. Reload to review its original request.",
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
