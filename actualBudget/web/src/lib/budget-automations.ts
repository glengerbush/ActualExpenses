import type { AmountFormat } from "./money";
import type { PendingMutation } from "./register-api";
import { numberFormats } from "./preferences/types";
import { validClock } from "./edit-clock";
export type TemplateDefinition = {
  id: string;
  name: string;
  notes: string;
  source: "notes" | "ui";
  templates: Record<string, unknown>[];
  cleanup: Record<string, unknown>[];
  parse_errors: { line: string; error: string }[];
  is_income: boolean;
  hidden: boolean;
  group_hidden: boolean;
  group_name: string;
  field_versions: Record<string, string>;
};
export type DefinitionPage = {
  month: string;
  method: "tracking" | "envelope";
  categories: TemplateDefinition[];
  pools: { id: string; name: string }[];
  snapshot: string;
  epoch: string;
  experimental: Record<string, boolean>;
};
export type TemplatePlan = {
  month: string;
  method: "tracking" | "envelope";
  categories: {
    id: string;
    name: string;
    before_budgeted: number;
    budgeted: number;
    goal: number | null;
    long_goal: number | null;
    applies: boolean;
    clear_goal: boolean;
    per_template: number[];
  }[];
  errors: { id: string; name: string; error: string }[];
  snapshot: string;
  epoch: string;
};
export type AutomationDraft = {
  version: 1;
  mode: "notes" | "apply" | "overwrite";
  month: string;
  method: "tracking" | "envelope";
  epoch: string;
  snapshot: string;
  body: Record<string, unknown>;
  originalNotes?: string;
  categoryName?: string;
  goalInput?: string;
  format: AmountFormat;
  preview: TemplatePlan | null;
  pending: PendingMutation | null;
};
export const automationKey = (budget: string) =>
  `actualBudget.templates.v1.${budget}`;
export function readAutomationDraft(budget: string): AutomationDraft | null {
  const raw = localStorage.getItem(automationKey(budget));
  if (raw === null) return null;
  let d: AutomationDraft;
  try {
    d = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved template work is unreadable. Keep browser data for recovery.",
    );
  }
  if (
    d?.version !== 1 ||
    !["notes", "apply", "overwrite"].includes(d.mode) ||
    !["tracking", "envelope"].includes(d.method) ||
    !/^\d{4}-\d{2}$/.test(d.month) ||
    !/^[0-9a-f]{32}$/.test(d.epoch) ||
    typeof d.snapshot !== "string" ||
    !d.body ||
    typeof d.body !== "object" ||
    !d.format ||
    !numberFormats.includes(d.format.number_format) ||
    ![0, 2].includes(d.format.decimal_places) ||
    (d.goalInput !== undefined && typeof d.goalInput !== "string") ||
    (d.preview &&
      (!Array.isArray(d.preview.categories) ||
        !Array.isArray(d.preview.errors)))
  )
    throw new Error(
      "Saved template work is unreadable. Keep browser data for recovery.",
    );
  if (
    d.mode === "notes" &&
    (typeof d.body.notes !== "string" || typeof d.originalNotes !== "string")
  )
    throw new Error(
      "Saved template notes are unreadable. Keep browser data for recovery.",
    );
  const p = d.pending;
  if (
    p &&
    (p.path !==
      `/budgets/${budget}/budget${d.mode === "notes" ? "" : "/templates/commands"}` ||
      p.method !== "POST" ||
      p.budgetEpoch !== d.epoch ||
      !validClock(p.edit_clock) ||
      typeof p.key !== "string")
  )
    throw new Error(
      "Saved template request is unreadable. Keep browser data for recovery.",
    );
  return d;
}
