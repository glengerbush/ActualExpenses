import { dateObject } from "./dates";
export function addMonths(month: string, n: number) {
  const d = dateObject(month + "-01");
  d.setUTCMonth(d.getUTCMonth() + n);
  return d.toISOString().slice(0, 7);
}
