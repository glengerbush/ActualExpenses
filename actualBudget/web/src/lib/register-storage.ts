import type { AmountFormat } from "./money";
import { dateFormats, type DateFormat } from "./preferences/types";
import {
  draftKey,
  actionKey,
  type PendingMutation,
  type Transaction,
} from "./register-api";
export interface TransactionFields {
  account_id: string;
  date: string;
  amount: string;
  direction: string;
  payee: string;
  category: string;
  notes: string;
  cleared: boolean;
  newPayee: string;
  newCategory: string;
  categoryIncome: boolean;
}
export interface TransactionDraft {
  version: 1;
  origin: string;
  budgetId: string;
  draftId: string;
  id: string;
  referenceIds: { payee: string; category: string };
  original: Transaction | null;
  fields: TransactionFields;
  format: AmountFormat;
  dateFormat: DateFormat;
  amountDirty: boolean;
  pending: PendingMutation | null;
}
export type SavedAction = PendingMutation & { description: string };
const isObject = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
const uuid = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
const text = (value: unknown, max: number): value is string =>
  typeof value === "string" && value.length <= max;
const equal = (a: unknown, b: unknown) =>
  JSON.stringify(a) === JSON.stringify(b);
function read(key: string): unknown {
  let raw: string | null;
  try {
    raw = localStorage.getItem(key);
  } catch {
    throw new Error(
      "Cannot read saved work on this device. Enable storage and retry recovery before making changes.",
    );
  }
  if (raw === null) return null;
  try {
    return JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved work could not be read. It has been kept unchanged; retry recovery before making changes.",
    );
  }
}
function validPending(
  value: unknown,
  budget: string,
): value is PendingMutation {
  if (!isObject(value) || !uuid(value.key) || typeof value.path !== "string")
    return false;
  const base = `/budgets/${budget}/transactions`;
  if (value.method === "POST" && value.path === base)
    return isObject(value.body) && uuid(value.body.id);
  const match = new RegExp(`^${base}/([0-9a-f-]{36})(/duplicate)?$`).exec(
    value.path,
  );
  if (!match || !uuid(match[1])) return false;
  if (value.method === "DELETE")
    return (
      !match[2] &&
      (value.body === undefined ||
        (isObject(value.body) && Object.keys(value.body).length === 0))
    );
  if (value.method === "PATCH") return !match[2] && isObject(value.body);
  return (
    value.method === "POST" &&
    !!match[2] &&
    isObject(value.body) &&
    uuid(value.body.id) &&
    Object.keys(value.body).length === 1
  );
}
function validDraft(value: unknown, budget: string): value is TransactionDraft {
  if (
    !isObject(value) ||
    value.version !== 1 ||
    value.origin !== location.origin ||
    value.budgetId !== budget ||
    !uuid(value.draftId) ||
    !uuid(value.id) ||
    !isObject(value.fields) ||
    !isObject(value.format) ||
    typeof value.amountDirty !== "boolean"
  )
    return false;
  const f = value.fields;
  if (
    !uuid(f.account_id) ||
    !text(f.date, 10) ||
    !text(f.amount, 256) ||
    !["expense", "income"].includes(String(f.direction)) ||
    !text(f.notes, 10000) ||
    typeof f.cleared !== "boolean" ||
    !text(f.newPayee, 120) ||
    !text(f.newCategory, 120) ||
    typeof f.categoryIncome !== "boolean"
  )
    return false;
  if (![f.payee, f.category].every((v) => v === "" || v === "new" || uuid(v)))
    return false;
  if (
    ![
      "comma-dot",
      "dot-comma",
      "space-comma",
      "apostrophe-dot",
      "comma-dot-in",
    ].includes(String(value.format.number_format)) ||
    ![0, 2].includes(Number(value.format.decimal_places))
  )
    return false;
  if (
    !(dateFormats as readonly string[]).includes(String(value.dateFormat)) ||
    !isObject(value.referenceIds) ||
    !uuid(value.referenceIds.payee) ||
    !uuid(value.referenceIds.category)
  )
    return false;
  if (
    value.original !== null &&
    (!isObject(value.original) ||
      value.original.id !== value.id ||
      !Number.isSafeInteger(value.original.amount))
  )
    return false;
  if (value.pending === null) return true;
  if (
    !validPending(value.pending, budget) ||
    value.pending.key !== value.draftId
  )
    return false;
  const expected = `/budgets/${budget}/transactions${value.original ? `/${value.id}` : ""}`;
  return (
    value.pending.path === expected &&
    value.pending.method === (value.original ? "PATCH" : "POST") &&
    (value.original !== null ||
      (isObject(value.pending.body) && value.pending.body.id === value.id))
  );
}
export function readTransactionDraft(budget: string): TransactionDraft | null {
  const stored = read(draftKey(budget));
  if (stored === null) return null;
  if (!validDraft(stored, budget))
    throw new Error(
      "The saved transaction draft could not be validated for this budget and server. It has been kept unchanged; no mutation will be sent.",
    );
  return stored;
}
export function saveTransactionDraft(
  draft: TransactionDraft,
  acknowledged?: PendingMutation,
): void {
  if (!validDraft(draft, draft.budgetId))
    throw new Error(
      "This transaction draft is invalid. Correct its fields before saving.",
    );
  const stored = readTransactionDraft(draft.budgetId);
  if (stored && stored.draftId !== draft.draftId)
    throw new Error(
      "Another transaction draft is saved. Reload to recover it before changing this form.",
    );
  if (
    stored?.pending &&
    !equal(stored.pending, draft.pending) &&
    !equal(stored.pending, acknowledged)
  )
    throw new Error(
      "An unconfirmed transaction is already saved. Reload and retry it before changing its details.",
    );
  localStorage.setItem(draftKey(draft.budgetId), JSON.stringify(draft));
}
export function clearTransactionDraft(
  draft: TransactionDraft,
  acknowledged = false,
): void {
  const stored = readTransactionDraft(draft.budgetId);
  if (!stored) return;
  if (stored.draftId !== draft.draftId)
    throw new Error(
      "Another transaction draft is saved. Reload before continuing.",
    );
  if (
    stored.pending &&
    (!acknowledged || !equal(stored.pending, draft.pending))
  )
    throw new Error("Retry the saved transaction before discarding its draft.");
  localStorage.removeItem(draftKey(draft.budgetId));
}
export function readRegisterAction(budget: string): SavedAction | null {
  const stored = read(actionKey(budget));
  if (stored === null) return null;
  if (
    !isObject(stored) ||
    stored.origin !== location.origin ||
    stored.budgetId !== budget ||
    !validPending(stored.pending, budget) ||
    !isObject(stored.pending) ||
    !text(stored.pending.description, 1000) ||
    stored.pending.method === "PATCH" ||
    stored.pending.path === `/budgets/${budget}/transactions`
  )
    throw new Error(
      "The saved register action could not be validated. It has been kept unchanged; retry recovery before making changes.",
    );
  return stored.pending as unknown as SavedAction;
}
export function saveRegisterAction(budget: string, pending: SavedAction): void {
  const stored = readRegisterAction(budget);
  if (stored && !equal(stored, pending))
    throw new Error(
      "Another unconfirmed register action is saved. Reload and retry it first.",
    );
  localStorage.setItem(
    actionKey(budget),
    JSON.stringify({ origin: location.origin, budgetId: budget, pending }),
  );
}
export function clearRegisterAction(
  budget: string,
  pending: SavedAction,
): void {
  const stored = readRegisterAction(budget);
  if (!stored) return;
  if (!equal(stored, pending))
    throw new Error(
      "Another register action is saved. Reload before continuing.",
    );
  localStorage.removeItem(actionKey(budget));
}
