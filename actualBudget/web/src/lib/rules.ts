import type { Condition } from "./register-query";
import type { PendingMutation, Transaction } from "./register-api";
import type { AmountFormat } from "./money";
import { validClock } from "./edit-clock";
export type RuleAction = {
  op: string;
  field?: string;
  value: Condition["value"];
  options?: {
    splitIndex?: number;
    method?: string;
    template?: string;
    formula?: string;
  };
};
export type Rule = {
  id: string;
  stage: "pre" | "post" | null;
  conditionsOp: "and" | "or";
  conditions: Condition[];
  actions: RuleAction[];
};
export type RuleProjection = Omit<Transaction, "amount" | "children"> & {
  amount: number | null;
  children?: RuleProjection[];
  rule_errors?: string[];
  error?: {
    type: "SplitTransactionError";
    version: 1;
    difference: number | null;
  } | null;
};
export type RulePreview = {
  transactions: {
    before: Transaction;
    after: RuleProjection;
    matched_rule_ids: string[];
  }[];
  candidate_count: number;
  total_count: number;
  next_offset: number | null;
  snapshot: string;
  epoch: string;
  protected_count: number;
};
export type RulesView = {
  data: Rule[];
  snapshot: string;
  epoch: string;
  edit_clock: string;
  learning_enabled: boolean;
};
export type RuleDraft = {
  version: 1;
  epoch: string;
  rule: Rule;
  existing: boolean;
  original: Rule | null;
  format: AmountFormat;
  pending: PendingMutation | null;
  intent: string;
};
export const rulesKey = (budget: string) => `actualBudget.rules.v1.${budget}`;
export function readRulesDraft(budget: string): RuleDraft | null {
  const raw = localStorage.getItem(rulesKey(budget));
  if (raw === null) return null;
  let d: RuleDraft;
  try {
    d = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved rule work is unreadable. Keep browser data for recovery.",
    );
  }
  if (
    !d ||
    d.version !== 1 ||
    !/^[0-9a-f]{32}$/.test(d.epoch) ||
    !d.rule ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      d.rule.id,
    ) ||
    ![null, "pre", "post"].includes(d.rule.stage) ||
    !["and", "or"].includes(d.rule.conditionsOp) ||
    !Array.isArray(d.rule.conditions) ||
    !Array.isArray(d.rule.actions) ||
    typeof d.existing !== "boolean" ||
    !d.format ||
    ![0, 2].includes(d.format.decimal_places) ||
    ![
      "comma-dot",
      "dot-comma",
      "space-comma",
      "apostrophe-dot",
      "comma-dot-in",
    ].includes(d.format.number_format) ||
    (d.existing && (!d.original || d.original.id !== d.rule.id)) ||
    typeof d.intent !== "string"
  )
    throw new Error(
      "Saved rule work is unreadable. Keep browser data for recovery.",
    );
  const p = d.pending;
  const base = `/budgets/${budget}/rules`;
  const route = p
    ? p.path === base ||
      [base + "/apply", base + "/learning", base + "/rename-payee"].includes(
        p.path,
      )
      ? p.method === "POST"
      : new RegExp(
          `^${base}/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`,
        ).test(p.path) && ["PATCH", "DELETE"].includes(p.method)
    : true;
  if (
    p &&
    (!["POST", "PATCH", "DELETE"].includes(p.method) ||
      !route ||
      !p.key ||
      !validClock(p.edit_clock ?? "") ||
      p.budgetEpoch !== d.epoch ||
      !p.body)
  )
    throw new Error(
      "Saved rule request is unreadable. Keep browser data for recovery.",
    );
  return d;
}
export const conditionFields: Record<string, string> = {
  payee: "Payee",
  imported_payee: "Imported payee",
  payee_name: "Payee name",
  category: "Category",
  category_group: "Category group",
  account: "Account",
  amount: "Amount",
  date: "Date",
  transfer: "Transfer",
  parent: "Split parent",
};
export const opLabels: Record<string, string> = {
  is: "is",
  isNot: "is not",
  oneOf: "is one of",
  notOneOf: "is not one of",
  contains: "contains",
  doesNotContain: "does not contain",
  matches: "matches regular expression",
  isapprox: "is approximately",
  isbetween: "is between",
  gt: "is greater than",
  gte: "is at least",
  lt: "is less than",
  lte: "is at most",
  onBudget: "is on budget",
  offBudget: "is off budget",
};
export function operators(field: string): string[] {
  if (["cleared", "reconciled", "transfer", "parent"].includes(field))
    return ["is"];
  if (field === "amount")
    return ["is", "isapprox", "isbetween", "gt", "gte", "lt", "lte"];
  if (field === "date") return ["is", "isapprox", "gt", "gte", "lt", "lte"];
  const ops = [
    "is",
    "isNot",
    "oneOf",
    "notOneOf",
    "contains",
    "doesNotContain",
    "matches",
  ];
  if (field === "account") return ops.concat(["onBudget", "offBudget"]);
  return ops;
}
export function newCondition(field = "payee", op = "is"): Condition {
  return {
    field,
    op,
    value:
      op === "isbetween"
        ? { num1: 0, num2: 0 }
        : ["oneOf", "notOneOf"].includes(op)
          ? []
          : field === "amount"
            ? 0
            : field === "date"
              ? "2026-01-01"
              : ["cleared", "reconciled", "transfer", "parent"].includes(field)
                ? false
                : ["payee", "account", "category", "category_group"].includes(
                      field,
                    )
                  ? null
                  : "",
  };
}
