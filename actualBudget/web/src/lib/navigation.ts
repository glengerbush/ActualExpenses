/** Delivered features register a real command here through App's command list. */
export interface NavigationCommand {
  id: string;
  label: string;
  keywords?: string;
  disabled?: boolean;
  run: () => void | Promise<void>;
}
export const rememberedViews = [
  "planning",
  "rules",
  "schedules",
  "reports",
  "settings",
] as const;
export type RememberedView = (typeof rememberedViews)[number];
const key = (budget: string) => `actualBudget.navigation.v1.${budget}`;
export function readView(budget: string): RememberedView | null {
  try {
    const value = JSON.parse(localStorage.getItem(key(budget)) ?? "null");
    return value?.version === 1 && rememberedViews.includes(value.view)
      ? value.view
      : null;
  } catch {
    return null;
  }
}
export function rememberView(budget: string, view: string | null): void {
  try {
    if (rememberedViews.includes(view as RememberedView))
      localStorage.setItem(key(budget), JSON.stringify({ version: 1, view }));
    else localStorage.removeItem(key(budget));
  } catch {
    /* Navigation remains usable when browser storage is unavailable. */
  }
}
