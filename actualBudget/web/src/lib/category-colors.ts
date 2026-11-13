// Match the server's stable 64-bit identity hash for unsaved color previews.
function hash(id: string): bigint {
  let value = 0n;
  for (const byte of new TextEncoder().encode(id))
    value = BigInt.asUintN(64, value * 31n + BigInt(byte));
  return value;
}
export function groupColor(id: string): string {
  return [
    "#397C8C",
    "#7764A0",
    "#A66B42",
    "#4D8563",
    "#AC5D74",
    "#6779A6",
    "#9A803C",
    "#587E78",
  ][Number(hash(id) % 8n)];
}
export function categoryShade(base: string, id: string): string {
  const shift = [-0.5, -0.3, -0.1, 0.15, 0.35, 0.55, 0.75][Number(hash(id) % 7n)];
  return (
    "#" +
    [1, 3, 5]
      .map((offset) => {
        const value = parseInt(base.slice(offset, offset + 2), 16);
        return Math.round(
          shift >= 0 ? value + (255 - value) * shift : value * (1 + shift),
        )
          .toString(16)
          .padStart(2, "0");
      })
      .join("")
      .toUpperCase()
  );
}
export type CategoryLaunch = {
  kind: "categories" | "category-groups";
  id?: string;
  groupId?: string;
  income?: boolean;
};
