import type { Condition } from "./register-query";
import type { RuleAction } from "./rules";
import type { PendingMutation, Transaction } from "./register-api";
import type { AmountFormat } from "./money";
import { validClock } from "./edit-clock";
export type Schedule = {
  id: string;
  rule_id: string;
  name: string | null;
  posts_transaction: boolean;
  completed: boolean;
  next_date: string | null;
  custom_upcoming_length: string | null;
  conditions: Condition[];
  actions: RuleAction[];
  status: string;
  account_id: string | null;
  payee_id: string | null;
  amount: Condition["value"];
};
export type SchedulePage = {
  schedules: Schedule[];
  previews: {
    id: string;
    schedule_id: string;
    date: string;
    amount: number;
    status: string;
  }[];
  preview_continuations: { schedule_id: string; from: string }[];
  next_offset: number | null;
  upcoming_length: string;
  today: string;
  snapshot: string;
  epoch: string;
};
export type ScheduleDraft = {
  version: 1;
  epoch: string;
  snapshot: string;
  body: Record<string, unknown>;
  original: Schedule | null;
  format: AmountFormat;
  pending: PendingMutation | null;
  transactions?: Transaction[];
};
export const schedulesKey = (budget: string) =>
  `actualBudget.schedules.v1.${budget}`;
export function readScheduleDraft(budget: string): ScheduleDraft | null {
  const raw = localStorage.getItem(schedulesKey(budget));
  if (raw === null) return null;
  let d: ScheduleDraft;
  try {
    d = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved schedule work is unreadable. Keep browser data for recovery.",
    );
  }
  if (
    !d ||
    d.version !== 1 ||
    !d.body ||
    typeof d.body !== "object" ||
    ![
      "create",
      "update",
      "delete",
      "post",
      "skip",
      "catch-up",
      "settings",
      "link",
      "unlink",
      "convert",
    ].includes(String(d.body.action)) ||
    !/^[0-9a-f]{32}$/.test(d.epoch) ||
    typeof d.snapshot !== "string" ||
    !d.format
  )
    throw new Error(
      "Saved schedule work is unreadable. Keep browser data for recovery.",
    );
  const p = d.pending;
  if (
    p &&
    (p.path !== `/budgets/${budget}/schedules/commands` ||
      p.method !== "POST" ||
      !validClock(p.edit_clock) ||
      p.budgetEpoch !== d.epoch ||
      !p.key ||
      !p.body)
  )
    throw new Error(
      "Saved schedule request is unreadable. Keep browser data for recovery.",
    );
  return d;
}

export type ScheduleMatches = {
  transactions: Transaction[];
  total_count: number;
  next_offset: number | null;
  snapshot: string;
  epoch: string;
};
export type ScheduleSuggestion = {
  id: string;
  account_id: string;
  payee_id: string | null;
  date: Record<string, unknown>;
  amount: number;
  conditions: Condition[];
};
export type ScheduleDiscovery = {
  suggestions: ScheduleSuggestion[];
  total_count: number;
  next_offset: number | null;
  snapshot: string;
  epoch: string;
  as_of: string;
};
export type ConversionPreview = {
  transaction: Transaction | Record<string, unknown>;
  conditions: Condition[];
  actions: RuleAction[];
  removed_transaction_ids: string[];
  affected_transactions: Transaction[];
  protected_count: number;
  is_future: boolean;
  is_beyond_window: boolean;
  days_until_transaction: number;
  upcoming_days: number;
  as_of: string;
  snapshot: string;
  epoch: string;
};
