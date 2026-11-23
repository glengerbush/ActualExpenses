import { type Account } from "./api";
export function sortedAccounts(accounts: Account[]) {
  return [...accounts].sort(
    (a, b) =>
      Number(a.off_budget) - Number(b.off_budget) ||
      (a.sort_order ?? 0) - (b.sort_order ?? 0) ||
      a.id.localeCompare(b.id),
  );
}
