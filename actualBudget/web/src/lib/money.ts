import { preferences } from "./preferences.svelte";
import { getCurrency } from "./preferences/currencies";
import type { NumberFormat } from "./preferences/types";
export interface AmountFormat {
  number_format: NumberFormat;
  decimal_places: 0 | 2;
}
const canonical: AmountFormat = {
  number_format: "comma-dot",
  decimal_places: 2,
};
const locales: Record<NumberFormat, string> = {
  "comma-dot": "en-US",
  "dot-comma": "de-DE",
  "space-comma": "fr-FR",
  "apostrophe-dot": "de-CH",
  "comma-dot-in": "en-IN",
};
export function captureAmountFormat(): AmountFormat {
  return {
    number_format: preferences.shared.number_format,
    decimal_places: getCurrency(preferences.shared.currency_code)
      .decimalPlaces as 0 | 2,
  };
}
export function amountDecimalPlaces(
  format: AmountFormat = captureAmountFormat(),
): 0 | 2 {
  return format.decimal_places;
}
function separator(format: AmountFormat): string {
  return ["dot-comma", "space-comma"].includes(format.number_format)
    ? ","
    : ".";
}
/** Normalize notation only. The server owns arithmetic and valid-prefix parsing. */
export function normalizeAmountExpression(
  input: string,
  format: AmountFormat = captureAmountFormat(),
): string {
  let result = input
    .trim()
    .replace(/\p{Sc}/gu, "")
    .replace(/\u2212/g, "-")
    .replace(/[\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, "");
  switch (format.number_format) {
    case "comma-dot":
    case "comma-dot-in":
      result = result.replace(/,/g, "");
      break;
    case "dot-comma":
      result = result.replace(/\./g, "").replace(/,/g, ".");
      break;
    case "space-comma":
      result = result.replace(/[ \u00a0\u202f]/g, "").replace(/,/g, ".");
      break;
    case "apostrophe-dot":
      result = result.replace(/['\u2019]/g, "");
      break;
  }
  return result;
}
/** Defaults stay canonical for version1 creation drafts without a captured format. */
export function parseAmount(
  input: string,
  format: AmountFormat = canonical,
): number {
  const normalized = normalizeAmountExpression(input, format);
  const pattern =
    format.decimal_places === 0 ? /^(-?)(\d+)$/ : /^(-?)(\d+)(?:\.(\d{1,2}))?$/;
  const match = pattern.exec(normalized);
  if (!match)
    throw new Error(
      format.decimal_places === 0
        ? "Enter a whole amount for this currency."
        : "Enter an amount with up to two decimal places.",
    );
  const divisor = 10n ** BigInt(format.decimal_places);
  const amount =
    BigInt(match[2]) * divisor +
    BigInt(format.decimal_places === 0 ? 0 : (match[3] ?? "").padEnd(2, "0"));
  if (amount > BigInt(Number.MAX_SAFE_INTEGER))
    throw new Error("This amount is too large. Enter a smaller balance.");
  return Number(match[1] ? -amount : amount);
}
export function formatAmountInput(
  amount: number,
  format: AmountFormat = captureAmountFormat(),
): string {
  const integer = BigInt(amount);
  const absolute = integer < 0n ? -integer : integer;
  const divisor = 10n ** BigInt(format.decimal_places);
  return `${integer < 0n ? "-" : ""}${absolute / divisor}${format.decimal_places ? separator(format) + (absolute % divisor).toString().padStart(2, "0") : ""}`;
}
export function formatAmount(amount: number): string {
  const settings = preferences.shared;
  const format = captureAmountFormat();
  const integer = BigInt(amount);
  const absolute = integer < 0n ? -integer : integer;
  const divisor = 10n ** BigInt(format.decimal_places);
  const hidden = settings.hide_fraction;
  const whole = hidden
    ? (absolute + divisor / 2n) / divisor
    : absolute / divisor;
  let formatted = new Intl.NumberFormat(locales[format.number_format], {
    maximumFractionDigits: 0,
  })
    .format(whole)
    .replace(/'/g, "\u2019");
  if (!hidden && format.decimal_places)
    formatted +=
      separator(format) + (absolute % divisor).toString().padStart(2, "0");
  const currency = getCurrency(settings.currency_code);
  if (currency.symbol) {
    const space = settings.currency_space ? "\u202f" : "";
    formatted =
      settings.currency_symbol_position === "after"
        ? `${formatted}${space}${currency.symbol}`
        : `\u202a${currency.symbol}\u202c${space}${formatted}`;
  }
  return `${integer < 0n && (!hidden || whole > 0n) ? "-" : ""}${formatted}`;
}
export function formatDate(iso: string): string {
  const [year, month, day] = iso.split("-");
  return preferences.shared.date_format
    .replace("yyyy", year)
    .replace("MM", month)
    .replace("dd", day);
}
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
