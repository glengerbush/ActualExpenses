import { request } from "./api";
import {
  normalizeAmountExpression,
  amountDecimalPlaces,
  captureAmountFormat,
} from "./money";
export async function calculateAmount(
  expression: string,
  format = captureAmountFormat(),
): Promise<number> {
  const result = await request<{ data: { amount: number } }>(
    "/amounts/evaluate",
    {
      expression: normalizeAmountExpression(expression, format),
      decimal_places: amountDecimalPlaces(format),
    },
  );
  return result.data.amount;
}
