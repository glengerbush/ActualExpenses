import type { PendingMutation } from "./register-api";
import type { ConversionPreview } from "./schedules";
import type { AmountFormat } from "./money";
import { validClock } from "./edit-clock";
import { dateFormats, type DateFormat } from "./preferences/types";
export type ConversionSource = {
  transaction_id?: string;
  transaction?: Record<string, unknown>;
  transfer_account_id?: string;
};
export type ConversionContext = {
  source: ConversionSource;
  epoch: string;
  snapshot: string | null;
  format: AmountFormat;
  dateFormat: DateFormat;
  clock?: string;
  sourceDraftRaw?: string;
  sourceDraftKey?: string;
};
export type ConversionIntent = ConversionContext & {
  version: 1;
  scheduleId: string;
  preview: ConversionPreview | null;
  pending: PendingMutation | null;
  confirmed: boolean;
};
export const conversionKey = (budget: string) =>
  `actualBudget.future-conversion.v1.${budget}`;
const uuid = (v: unknown) =>
  typeof v === "string" &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
export function readConversion(budget: string): ConversionIntent | null {
  const raw = localStorage.getItem(conversionKey(budget));
  if (raw === null) return null;
  let d: ConversionIntent;
  try {
    d = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved future conversion is unreadable. Keep browser data for recovery.",
    );
  }
  const source = d?.source;
  if (
    d?.version !== 1 ||
    !uuid(d.scheduleId) ||
    !source ||
    Boolean(source.transaction_id) === Boolean(source.transaction) ||
    (source.transaction_id && !uuid(source.transaction_id)) ||
    (source.transaction && !uuid(source.transaction.id)) ||
    (source.transfer_account_id && !uuid(source.transfer_account_id)) ||
    !/^[0-9a-f]{32}$/.test(d.epoch) ||
    !(d.snapshot === null || typeof d.snapshot === "string") ||
    typeof d.confirmed !== "boolean" ||
    !d.format ||
    !dateFormats.includes(d.dateFormat)
  )
    throw new Error(
      "Saved future conversion is unreadable. Keep browser data for recovery.",
    );
  if (
    d.sourceDraftKey &&
    ![
      `actualBudget.transaction.v1.${budget}`,
      `actualBudget.transfers.v1.${budget}`,
    ].includes(d.sourceDraftKey)
  )
    throw new Error("Unknown saved source draft.");
  const p = d.pending;
  if (
    p &&
    (p.path !== `/budgets/${budget}/schedules/commands` ||
      p.method !== "POST" ||
      p.budgetEpoch !== d.epoch ||
      !validClock(p.edit_clock) ||
      !uuid(p.key) ||
      !p.body ||
      typeof p.body !== "object" ||
      (p.body as Record<string, unknown>).action !== "convert" ||
      (p.body as Record<string, unknown>).id !== d.scheduleId)
  )
    throw new Error(
      "Saved future conversion request is unreadable. Keep browser data for recovery.",
    );
  return d;
}
