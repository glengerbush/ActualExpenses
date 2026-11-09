import { type Account, type AccountGroup } from "./api";
export const accountDraftKey = (budget: string) =>
  `actualBudget.accounts.v1.${budget}`;
export interface AccountIntent {
  path: string;
  body: Record<string, unknown>;
  key: string;
  method: "POST" | "PATCH" | "DELETE";
  clock: string;
  epoch: string;
  message: string;
}
export interface AccountDraft {
  version: 1;
  budgetId: string;
  epoch: string;
  accountId: string;
  name: string;
  notes: string;
  groupId: string;
  beforeId: string;
  groupSelection: string;
  groupName: string;
  groupBefore: string;
  confirmName: string;
  initial: Account | null;
  pending: AccountIntent | null;
}
export function readAccountDraft(budget: string): AccountDraft | null {
  const raw = localStorage.getItem(accountDraftKey(budget));
  if (raw === null) return null;
  const d = JSON.parse(raw) as AccountDraft;
  if (
    d?.version !== 1 ||
    d.budgetId !== budget ||
    typeof d.epoch !== "string" ||
    ![
      "accountId",
      "name",
      "notes",
      "groupId",
      "beforeId",
      "groupSelection",
      "groupName",
      "groupBefore",
      "confirmName",
    ].every((key) => typeof d[key as keyof AccountDraft] === "string") ||
    (d.pending &&
      (!d.pending.path.startsWith(`/budgets/${budget}/`) ||
        !["POST", "PATCH", "DELETE"].includes(d.pending.method) ||
        typeof d.pending.key !== "string" ||
        typeof d.pending.clock !== "string" ||
        typeof d.pending.epoch !== "string"))
  )
    throw new Error(
      "Saved account changes are unreadable. Keep this browser data for recovery.",
    );
  return d;
}
function requirePendingOwner(budgetId: string, pending: AccountIntent | null) {
  const existing = readAccountDraft(budgetId);
  if (
    existing?.pending &&
    JSON.stringify(existing.pending) !== JSON.stringify(pending)
  )
    throw new Error(
      "Another tab has a pending account change. Reload before continuing.",
    );
}
export function persistAccountDraft(
  draft: AccountDraft,
  previous?: AccountDraft | null,
) {
  requirePendingOwner(draft.budgetId, previous?.pending ?? draft.pending);
  localStorage.setItem(accountDraftKey(draft.budgetId), JSON.stringify(draft));
}
export function clearAccountDraft(budgetId: string) {
  // Closing an editor can only discard an unsent draft, never a saved request.
  requirePendingOwner(budgetId, null);
  localStorage.removeItem(accountDraftKey(budgetId));
}
export function newAccountDraft(
  budgetId: string,
  epoch: string,
  account: Account | null,
): AccountDraft {
  return {
    version: 1,
    budgetId,
    epoch,
    accountId: account?.id ?? "",
    name: account?.name ?? "",
    notes: account?.notes ?? "",
    groupId: account?.group_id ?? "",
    beforeId: "",
    groupSelection: "",
    groupName: "",
    groupBefore: "",
    confirmName: "",
    initial: account,
    pending: null,
  };
}
export function sortedAccounts(accounts: Account[]) {
  return [...accounts].sort(
    (a, b) =>
      Number(a.off_budget) - Number(b.off_budget) ||
      (a.sort_order ?? 0) - (b.sort_order ?? 0) ||
      a.id.localeCompare(b.id),
  );
}
export function sortedGroups(groups: AccountGroup[]) {
  return [...groups].sort(
    (a, b) => a.sort_order - b.sort_order || a.id.localeCompare(b.id),
  );
}
