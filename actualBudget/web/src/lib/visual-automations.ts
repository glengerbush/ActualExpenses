import { normalizeAmountExpression, type AmountFormat } from "./money";
const numeric = new Set([
  "priority",
  "monthly",
  "amount",
  "percent",
  "adjustment",
  "weight",
  "numMonths",
  "lookBack",
  "repeat",
]);
/** Keep unfinished localized text in the draft. Only a prepared request gets
 * typed values; an attempted request is never rebuilt from these fields. */
export function prepareVisualBody(
  body: Record<string, unknown>,
  format: AmountFormat,
): Record<string, unknown> {
  const walk = (value: unknown, key: string): unknown => {
    if (Array.isArray(value)) return value.map((v) => walk(v, ""));
    if (value && typeof value === "object")
      return Object.fromEntries(
        Object.entries(value).map(([k, v]) => [k, walk(v, k)]),
      );
    if (numeric.has(key) && typeof value === "string") {
      if (!value.trim()) return null;
      const raw = normalizeAmountExpression(value, {
        ...format,
        decimal_places: 2,
      });
      if (
        !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(raw) ||
        !Number.isFinite(Number(raw))
      )
        throw new Error(
          `Complete the ${key} value before saving. Your entered text remains retained.`,
        );
      return Number(raw);
    }
    return value;
  };
  return walk(structuredClone(body), "") as Record<string, unknown>;
}
export function visualNumber(value: unknown, format: AmountFormat): string {
  if (typeof value !== "number") return String(value ?? "");
  return ["dot-comma", "space-comma"].includes(format.number_format)
    ? String(value).replace(".", ",")
    : String(value);
}
