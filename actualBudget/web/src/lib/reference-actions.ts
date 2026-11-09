import { uuid } from "./api";
export interface ReferenceIntent {
  historyOwner?: string;
  path: string;
  method: "POST" | "PATCH";
  body: Record<string, unknown>;
  key: string;
  clock: string;
  epoch: string;
}
export interface ReferenceDraft {
  version: 1;
  budget: string;
  revision: string;
  epoch: string;
  kind: "payees" | "tags";
  selected: string[];
  editing: string;
  name: string;
  color: string;
  description: string;
  target: string;
  pending: ReferenceIntent | null;
}
export const referenceDraftKey = (budget: string) =>
  `actualBudget.references.v1.${budget}`;
export function readReferenceDraft(budget: string): ReferenceDraft | null {
  const raw = localStorage.getItem(referenceDraftKey(budget));
  if (!raw) return null;
  const value = JSON.parse(raw) as ReferenceDraft;
  if (
    value?.version !== 1 ||
    value.budget !== budget ||
    typeof value.revision !== "string" ||
    typeof value.epoch !== "string" ||
    !["payees", "tags"].includes(value.kind) ||
    !Array.isArray(value.selected) ||
    !value.selected.every((id) => typeof id === "string") ||
    !["editing", "name", "color", "description", "target"].every(
      (k) => typeof value[k as keyof ReferenceDraft] === "string",
    ) ||
    (value.pending &&
      (!value.pending.path.startsWith(`/budgets/${budget}/`) ||
        typeof value.pending.key !== "string" ||
        typeof value.pending.clock !== "string" ||
        typeof value.pending.epoch !== "string" ||
        !["POST", "PATCH"].includes(value.pending.method)))
  )
    throw new Error(
      "Saved payee or tag changes are unreadable. Keep this browser data for recovery.",
    );
  return value;
}
/** Compare the complete revision, even when this tab believes no request is pending. */
export function saveReferenceDraft(
  next: ReferenceDraft,
  previous: ReferenceDraft | null,
): ReferenceDraft {
  const existing = readReferenceDraft(next.budget);
  if ((existing?.revision ?? null) !== (previous?.revision ?? null))
    throw new Error(
      "Another tab changed this form. Reload to recover its saved work.",
    );
  const saved = { ...next, revision: uuid() };
  localStorage.setItem(referenceDraftKey(next.budget), JSON.stringify(saved));
  return saved;
}
export function clearReferenceDraft(draft: ReferenceDraft) {
  const existing = readReferenceDraft(draft.budget);
  if (existing?.revision !== draft.revision)
    throw new Error("Another tab has saved changes. Reload before leaving.");
  if (existing.pending)
    throw new Error("Resolve the pending change before leaving.");
  localStorage.removeItem(referenceDraftKey(draft.budget));
}
export const referenceCancelKey = (intent: ReferenceIntent) =>
  `actualBudget.references.cancel.${intent.key}`;
