import { uuid, type Budget, type NewAccount } from "./api";
import { parseAmount, today } from "./money";

export type DraftScope =
  { kind: "budget" } | { kind: "account"; budget_id: string };
interface BaseDraft {
  version: 1;
  origin: string;
  draft_id: string;
}
export interface BudgetFields {
  name: string;
  method: "tracking" | "envelope";
}
export interface AccountFields {
  name: string;
  balance: string;
  date: string;
  off_budget: boolean;
  notes: string;
}
export interface BudgetDraft extends BaseDraft {
  kind: "budget";
  fields: BudgetFields;
  pending: { key: string; body: Budget } | null;
}
export interface AccountDraft extends BaseDraft {
  kind: "account";
  budget_id: string;
  fields: AccountFields;
  pending: { key: string; body: NewAccount } | null;
}
export type CreationDraft = BudgetDraft | AccountDraft;
const prefix = "actualBudget.creation.v1.";
const validUuid = (value: unknown): value is string =>
  typeof value === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const string = (value: unknown, max: number): value is string =>
  typeof value === "string" && value.length <= max;
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
function key(scope: DraftScope): string {
  if (scope.kind === "account" && !validUuid(scope.budget_id))
    throw new Error("This account draft has an invalid budget reference.");
  return (
    prefix + (scope.kind === "budget" ? "budget" : `account.${scope.budget_id}`)
  );
}
export function newBudgetDraft(): BudgetDraft {
  return {
    version: 1,
    origin: location.origin,
    draft_id: uuid(),
    kind: "budget",
    fields: { name: "", method: "tracking" },
    pending: null,
  };
}
export function newAccountDraft(budget_id: string): AccountDraft {
  return {
    version: 1,
    origin: location.origin,
    draft_id: uuid(),
    kind: "account",
    budget_id,
    fields: {
      name: "",
      balance: "0.00",
      date: today(),
      off_budget: false,
      notes: "",
    },
    pending: null,
  };
}
function calendarDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const [, y, m, d] = match.map(Number);
  const leap = y % 4 === 0 && (y % 100 !== 0 || y % 400 === 0);
  return (
    y >= 1 &&
    m >= 1 &&
    m <= 12 &&
    d >= 1 &&
    d <= [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m - 1]
  );
}
export function prepareSubmission(draft: CreationDraft): CreationDraft {
  if (draft.pending) return draft;
  if (!draft.fields.name.trim())
    throw new Error(
      `Enter ${draft.kind === "budget" ? "a budget" : "an account"} name.`,
    );
  if (draft.kind === "budget")
    return {
      ...draft,
      pending: {
        key: draft.draft_id,
        body: {
          id: draft.draft_id,
          name: draft.fields.name.trim(),
          budget_type: draft.fields.method,
        },
      },
    };
  if (!calendarDate(draft.fields.date))
    throw new Error("Enter a valid opening date between years 0001 and 9999.");
  return {
    ...draft,
    pending: {
      key: draft.draft_id,
      body: {
        id: draft.draft_id,
        name: draft.fields.name.trim(),
        off_budget: draft.fields.off_budget,
        opening_balance: parseAmount(draft.fields.balance),
        opening_date: draft.fields.date,
        notes: draft.fields.notes,
      },
    },
  };
}
function validate(value: unknown, scope: DraftScope): value is CreationDraft {
  if (
    !object(value) ||
    value.version !== 1 ||
    value.origin !== location.origin ||
    !validUuid(value.draft_id) ||
    value.kind !== scope.kind ||
    !object(value.fields)
  )
    return false;
  if (scope.kind === "account" && value.budget_id !== scope.budget_id)
    return false;
  const fields = value.fields;
  if (!string(fields.name, 120)) return false;
  if (scope.kind === "budget") {
    if (fields.method !== "tracking" && fields.method !== "envelope")
      return false;
  } else if (
    !string(fields.balance, 128) ||
    !string(fields.date, 10) ||
    typeof fields.off_budget !== "boolean" ||
    !string(fields.notes, 10000)
  )
    return false;
  if (value.pending === null) return true;
  if (
    !object(value.pending) ||
    !validUuid(value.pending.key) ||
    !object(value.pending.body)
  )
    return false;
  const body = value.pending.body;
  if (
    !validUuid(body.id) ||
    body.id !== value.draft_id ||
    value.pending.key !== value.draft_id ||
    !string(body.name, 120) ||
    !body.name.trim() ||
    body.name !== fields.name.trim()
  )
    return false;
  if (scope.kind === "budget") return body.budget_type === fields.method;
  if (
    !Number.isSafeInteger(body.opening_balance) ||
    body.off_budget !== fields.off_budget ||
    body.notes !== fields.notes ||
    body.opening_date !== fields.date ||
    !calendarDate(String(body.opening_date))
  )
    return false;
  try {
    return body.opening_balance === parseAmount(fields.balance as string);
  } catch {
    return false;
  }
}
export function readDraft(scope: DraftScope): CreationDraft | null {
  let raw: string | null;
  try {
    raw = localStorage.getItem(key(scope));
  } catch {
    throw new Error(
      "Cannot read browser storage. Enable storage to recover creation drafts. No creation request will be sent.",
    );
  }
  if (raw === null) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    /* Keep unrecognized data intact. */
  }
  if (!validate(value, scope))
    throw new Error(
      "The saved creation draft could not be validated for this budget and server. It has been kept unchanged; no creation request will be sent.",
    );
  return value;
}
export function saveDraft(
  draft: CreationDraft,
  acknowledgedPending?: CreationDraft["pending"],
): void {
  if (!validate(draft, draft))
    throw new Error(
      "This creation draft is invalid. Correct its fields before continuing.",
    );
  const stored = readDraft(draft);
  if (stored && stored.draft_id !== draft.draft_id)
    throw new Error(
      "Another creation draft is saved here. Reload to recover it before making changes.",
    );
  if (
    stored?.pending &&
    JSON.stringify(stored.pending) !== JSON.stringify(draft.pending) &&
    JSON.stringify(stored.pending) !== JSON.stringify(acknowledgedPending)
  )
    throw new Error(
      "An unconfirmed creation is already saved. Reload and retry it before changing its details.",
    );
  try {
    localStorage.setItem(key(draft), JSON.stringify(draft));
  } catch {
    throw new Error(
      "Cannot save to browser storage. Your entered fields remain here, but are not yet protected from a reload. Enable storage or free space, then retry. No new request has been sent.",
    );
  }
}
export function clearDraft(draft: CreationDraft, acknowledged = false): void {
  const stored = readDraft(draft);
  if (!stored) return;
  if (stored.draft_id !== draft.draft_id)
    throw new Error(
      "Another creation draft is saved here. Reload before continuing.",
    );
  if (
    stored.pending &&
    (!acknowledged ||
      JSON.stringify(stored.pending) !== JSON.stringify(draft.pending))
  )
    throw new Error(
      "Retry the unconfirmed creation before discarding its draft.",
    );
  try {
    localStorage.removeItem(key(draft));
  } catch {
    throw new Error(
      "Cannot clear the saved draft from browser storage. Enable storage, then retry to confirm this creation.",
    );
  }
}
