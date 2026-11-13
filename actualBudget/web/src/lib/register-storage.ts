import { validClock } from "./edit-clock";
import type { AmountFormat } from "./money";
import { dateFormats, type DateFormat } from "./preferences/types";
import {
  draftKey,
  actionKey,
  cancellationRequested,
  clearCancellation,
  type PendingMutation,
  type Transaction,
} from "./register-api";
export interface TransactionFields {
  recurrence?: import("./recurrence").Recurrence;
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
  categoryManual?: boolean;
  manualCategory?: string;
}
export interface TransactionDraft {
  payeeCreation?: PendingMutation;
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
  editClock?: string;
  clockRejected?: boolean;
  attemptId?: string;
  budgetEpoch?: string;
  beganOnline?: boolean;
  automaticRules?: boolean;
  ruleSnapshot?: string;
  ruleChildren?: Transaction[];
  ruleDeleted?: boolean;
  replaces?: {
    key: string;
    revision: string;
    row: Transaction;
    pending: PendingMutation;
    baseline?: Transaction | null;
  };
}
export type SavedAction = PendingMutation & {
  description: string;
  clockRejected?: boolean;
  attemptId?: string;
};
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
export function validPending(
  value: unknown,
  budget: string,
): value is PendingMutation {
  if (!isObject(value) || !uuid(value.key) || typeof value.path !== "string")
    return false;
  if (
    value.cancelRequested !== undefined &&
    typeof value.cancelRequested !== "boolean"
  )
    return false;
  if (value.historyOwner !== undefined && !uuid(value.historyOwner))
    return false;
  if (value.attemptId !== undefined && !uuid(value.attemptId)) return false;
  if (value.edit_clock !== undefined && !validClock(value.edit_clock))
    return false;
  if (
    value.clockRejected !== undefined &&
    typeof value.clockRejected !== "boolean"
  )
    return false;
  if (
    value.budgetEpoch !== undefined &&
    (typeof value.budgetEpoch !== "string" ||
      !/^[0-9a-f]{32}$/.test(value.budgetEpoch))
  )
    return false;
  const base = `/budgets/${budget}/transactions`;
  if (
    value.method === "POST" &&
    (value.path === base ||
      value.path === `/budgets/${budget}/rules/transactions`)
  )
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
    f.recurrence !== undefined &&
    (!isObject(f.recurrence) ||
      typeof f.recurrence.enabled !== "boolean" ||
      !isObject(f.recurrence.config) ||
      !text(f.recurrence.config.start, 10) ||
      typeof f.recurrence.config.frequency !== "string")
  )
    return false;
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
  if (
    value.replaces !== undefined &&
    (!isObject(value.replaces) ||
      !uuid(value.replaces.key) ||
      !uuid(value.replaces.revision) ||
      !isObject(value.replaces.row) ||
      value.replaces.row.id !== value.id ||
      !Number.isSafeInteger(value.replaces.row.amount) ||
      (value.replaces.baseline !== undefined &&
        value.replaces.baseline !== null &&
        (!isObject(value.replaces.baseline) ||
          value.replaces.baseline.id !== value.id ||
          !Number.isSafeInteger(value.replaces.baseline.amount))) ||
      !validPending(value.replaces.pending, budget) ||
      value.replaces.pending.key !== value.replaces.key)
  )
    return false;
  if (value.historyOwner !== undefined && !uuid(value.historyOwner))
    return false;
  if (value.attemptId !== undefined && !uuid(value.attemptId)) return false;
  if (value.editClock !== undefined && !validClock(value.editClock))
    return false;
  if (
    value.clockRejected !== undefined &&
    typeof value.clockRejected !== "boolean"
  )
    return false;
  const payee = value.payeeCreation;
  if (
    payee !== undefined &&
    (!isObject(payee) ||
      payee.path !== `/budgets/${budget}/payees` ||
      payee.method !== "POST" ||
      !uuid(payee.key) ||
      !validClock(payee.edit_clock) ||
      payee.budgetEpoch !== value.budgetEpoch ||
      !isObject(payee.body) ||
      !uuid(payee.body.id) ||
      !text(payee.body.name, 120))
  )
    return false;
  if (value.pending === null) return !value.clockRejected;
  if (
    !validPending(value.pending, budget) ||
    value.pending.key !== value.draftId
  )
    return false;
  const expected = `/budgets/${budget}/transactions${value.original ? `/${value.id}` : ""}`;
  return (
    (value.pending.path === expected ||
      (!value.original &&
        value.pending.path === `/budgets/${budget}/rules/transactions`)) &&
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
  if (stored.pending && cancellationRequested(stored.pending))
    stored.pending.cancelRequested = true;
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
    !equalPending(stored.pending, draft.pending) &&
    !equalPending(stored.pending, acknowledged)
  )
    throw new Error(
      "An unconfirmed transaction is already saved. Reload and retry it before changing its details.",
    );
  if (
    stored?.payeeCreation &&
    !equalPending(stored.payeeCreation, draft.payeeCreation) &&
    !equalPending(stored.payeeCreation, acknowledged)
  )
    throw new Error(
      "Resolve the saved payee creation before changing this draft.",
    );
  if (draft.clockRejected && stored?.attemptId !== draft.attemptId)
    throw new Error(
      "Another retry may have reached the server. Reload and retry the saved transaction.",
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
    (!acknowledged || !equalPending(stored.pending, draft.pending))
  )
    throw new Error("Retry the saved transaction before discarding its draft.");
  if (stored.payeeCreation)
    throw new Error(
      "Resolve the saved payee creation before discarding this draft.",
    );
  localStorage.removeItem(draftKey(draft.budgetId));
  if (acknowledged && draft.pending) clearCancellation(draft.pending);
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
  const pending = stored.pending as unknown as SavedAction;
  if (cancellationRequested(pending)) pending.cancelRequested = true;
  return pending;
}
export function saveRegisterAction(budget: string, pending: SavedAction): void {
  const stored = readRegisterAction(budget);
  if (stored && !equalAction(stored, pending))
    throw new Error(
      "Another unconfirmed register action is saved. Reload and retry it first.",
    );
  if (pending.clockRejected && stored?.attemptId !== pending.attemptId)
    throw new Error(
      "Another retry may have reached the server. Reload and retry the saved action.",
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
  if (!equalAction(stored, pending))
    throw new Error(
      "Another register action is saved. Reload before continuing.",
    );
  localStorage.removeItem(actionKey(budget));
  clearCancellation(pending);
}

function equalAction(a: SavedAction, b: SavedAction) {
  return equal(
    {
      ...a,
      clockRejected: undefined,
      attemptId: undefined,
      cancelRequested: undefined,
    },
    {
      ...b,
      clockRejected: undefined,
      attemptId: undefined,
      cancelRequested: undefined,
    },
  );
}

function equalPending(
  a: PendingMutation | null | undefined,
  b: PendingMutation | null | undefined,
) {
  return equal(
    a ? { ...a, cancelRequested: undefined } : a,
    b ? { ...b, cancelRequested: undefined } : b,
  );
}
