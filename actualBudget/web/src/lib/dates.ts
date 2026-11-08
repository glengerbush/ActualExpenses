import type { DateFormat } from "./preferences/types";
export function dateObject(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  const value = new Date(0);
  value.setUTCHours(0, 0, 0, 0);
  value.setUTCFullYear(y, m - 1, d);
  return value;
}
export function isoDate(value: Date): string {
  return `${String(value.getUTCFullYear()).padStart(4, "0")}-${String(value.getUTCMonth() + 1).padStart(2, "0")}-${String(value.getUTCDate()).padStart(2, "0")}`;
}
export function validDate(iso: string): boolean {
  return (
    /^\d{4}-\d{2}-\d{2}$/.test(iso) &&
    iso >= "0001-01-01" &&
    iso <= "9999-12-31" &&
    isoDate(dateObject(iso)) === iso
  );
}
export function displayDate(iso: string, format: DateFormat): string {
  if (!validDate(iso)) return iso;
  const [year, month, day] = iso.split("-");
  return format.replace("yyyy", year).replace("MM", month).replace("dd", day);
}
// DateSelect accepts month/day in the current year and two-digit years in a
// rolling 100-year window ending 50 years after the reference year. Keep this
// parser separate from input events: normalizing a prefix would interrupt typing.
export function parseDateInput(
  input: string,
  format: DateFormat,
  referenceYear = new Date().getFullYear(),
): string {
  if (validDate(input)) return input;
  const separator = format.match(/[^\w]/)![0];
  const pieces = input.trimEnd().split(separator);
  const tokens = format.match(/yyyy|MM|dd/g)!;
  const hasYear = pieces.length === 3;
  if (!hasYear && (pieces.length !== 2 || input !== input.trimEnd()))
    return input;
  const activeTokens = hasYear
    ? tokens
    : tokens.filter((token) => token !== "yyyy");
  const parts = Object.fromEntries(
    activeTokens.map((token, index) => [token, pieces[index]]),
  );
  if (!/^\d{1,2}$/.test(parts.MM ?? "") || !/^\d{1,2}$/.test(parts.dd ?? ""))
    return input;
  let year = referenceYear;
  if (hasYear) {
    if (!/^\d{1,4}$/.test(parts.yyyy ?? "")) return input;
    year = Number(parts.yyyy);
    // The pinned shortcut regex is anchored, unlike full-date parsing which
    // permits trailing whitespace. Preserve that distinction for short years.
    if (parts.yyyy.length === 2 && input === input.trimEnd()) {
      const end = referenceYear + 50;
      year += Math.floor(end / 100) * 100;
      if (year >= end) year -= 100;
    }
  }
  const result = `${String(year).padStart(4, "0")}-${parts.MM.padStart(2, "0")}-${parts.dd.padStart(2, "0")}`;
  return validDate(result) ? result : input;
}
export function moveDay(iso: string, days: number): string {
  const date = dateObject(iso);
  date.setUTCDate(date.getUTCDate() + days);
  return date.getUTCFullYear() < 1
    ? "0001-01-01"
    : date.getUTCFullYear() > 9999
      ? "9999-12-31"
      : isoDate(date);
}
export function moveMonth(iso: string, months: number): string {
  const date = dateObject(iso);
  const day = date.getUTCDate();
  date.setUTCDate(1);
  date.setUTCMonth(date.getUTCMonth() + months);
  if (date.getUTCFullYear() < 1) return "0001-01-01";
  if (date.getUTCFullYear() > 9999) return "9999-12-31";
  const last = new Date(date);
  last.setUTCMonth(last.getUTCMonth() + 1);
  last.setUTCDate(0);
  date.setUTCDate(Math.min(day, last.getUTCDate()));
  return isoDate(date);
}
export function calendarWeeks(
  month: string,
  firstDay: number,
): (string | null)[][] {
  const first = dateObject(`${month}-01`);
  const offset = (first.getUTCDay() - firstDay + 7) % 7;
  const days = Array.from({ length: 42 }, (_, index) => {
    const date = new Date(first);
    date.setUTCDate(date.getUTCDate() - offset + index);
    return date.getUTCFullYear() < 1 || date.getUTCFullYear() > 9999
      ? null
      : isoDate(date);
  });
  return Array.from({ length: 6 }, (_, week) =>
    days.slice(week * 7, week * 7 + 7),
  );
}
