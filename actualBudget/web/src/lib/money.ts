export function parseAmount(input: string): number {
  const match = /^(-?)(\d+)(?:\.(\d{1,2}))?$/.exec(input.trim());
  if (!match)
    throw new Error(
      "Enter an amount with up to two decimal places, such as 1234.56 or -25.00.",
    );
  const amount =
    BigInt(match[2]) * 100n + BigInt((match[3] ?? "").padEnd(2, "0"));
  if (amount > BigInt(Number.MAX_SAFE_INTEGER))
    throw new Error("This amount is too large. Enter a smaller balance.");
  return Number(match[1] ? -amount : amount);
}
export function formatAmount(amount: number): string {
  const integer = BigInt(amount);
  const absolute = integer < 0n ? -integer : integer;
  return `${integer < 0n ? "-" : ""}${(absolute / 100n).toLocaleString("en-US")}.${(absolute % 100n).toString().padStart(2, "0")}`;
}
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
