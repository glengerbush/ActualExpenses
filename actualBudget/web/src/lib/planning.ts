import { validClock } from "./edit-clock";
import type { AmountFormat } from "./money";
import type { PendingMutation } from "./register-api";
export interface PlanCategory {
  id: string;
  name: string;
  is_income: boolean;
  hidden: boolean;
  group_hidden: boolean;
  group_id: string | null;
  group_name: string;
  notes: string;
  budgeted: number;
  actual: number;
  balance: number;
  carryover: boolean;
}
export interface PlanMonth {
  month: string;
  summary: Record<string, number>;
  categories: PlanCategory[];
  notes: string;
}
export interface PlanView {
  method: "tracking" | "envelope";
  months: Record<string, PlanMonth>;
  snapshot: string;
  epoch: string;
  edit_clock: string;
}
export interface PlanningDraft {
  version: 1;
  epoch: string;
  month: string;
  action: string;
  category: string;
  from: string;
  to: string;
  amount: string;
  notes: string;
  method: "tracking" | "envelope";
  enabled: boolean;
  format: AmountFormat;
  pending: PendingMutation | null;
}
export const planningKey = (budget: string) =>
  `actualBudget.planning.v1.${budget}`;
export function readPlanningDraft(budget: string): PlanningDraft | null {
  const raw = localStorage.getItem(planningKey(budget));
  if (raw === null) return null;
  let value: PlanningDraft;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved budget work is unreadable. Keep browser data for recovery.",
    );
  }
  if (
    !value ||
    typeof value !== "object" ||
    value.version !== 1 ||
    !/^[0-9a-f]{32}$/.test(value.epoch) ||
    !/^\d{4}-(0[1-9]|1[0-2])$/.test(value.month) ||
    ![
      "set",
      "carryover",
      "copy_previous",
      "copy_year",
      "zero",
      "hold",
      "reset_hold",
      "move",
      "cover",
      "cover_overbudgeted",
      "notes",
      "category_notes",
      "method",
      "undo",
      "redo",
    ].includes(value.action) ||
    !["tracking", "envelope"].includes(value.method) ||
    !["amount", "notes", "category", "from", "to"].every(
      (k) => typeof value[k as keyof PlanningDraft] === "string",
    ) ||
    typeof value.enabled !== "boolean" ||
    ![0, 2].includes(value.format?.decimal_places) ||
    ![
      "comma-dot",
      "dot-comma",
      "space-comma",
      "apostrophe-dot",
      "comma-dot-in",
    ].includes(value.format?.number_format)
  )
    throw new Error(
      "Saved budget work is unreadable. Keep browser data for recovery.",
    );
  const p = value.pending;
  if (
    p &&
    (p.method !== "POST" ||
      ![
        `/budgets/${budget}/budget`,
        `/budgets/${budget}/history/undo`,
        `/budgets/${budget}/history/redo`,
      ].includes(p.path) ||
      !validClock(p.edit_clock) ||
      p.budgetEpoch !== value.epoch ||
      !p.key ||
      !p.body)
  )
    throw new Error(
      "Saved budget request is unreadable. Keep browser data for recovery.",
    );
  return value;
}
