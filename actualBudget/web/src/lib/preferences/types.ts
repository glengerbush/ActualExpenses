export const numberFormats = [
  "comma-dot",
  "dot-comma",
  "space-comma",
  "apostrophe-dot",
  "comma-dot-in",
] as const;
export type NumberFormat = (typeof numberFormats)[number];
export const dateFormats = [
  "MM/dd/yyyy",
  "dd/MM/yyyy",
  "yyyy-MM-dd",
  "MM.dd.yyyy",
  "dd.MM.yyyy",
  "dd-MM-yyyy",
] as const;
export type DateFormat = (typeof dateFormats)[number];
export const featureNames = [
  "newSidebarUI",
  "goalTemplatesEnabled",
  "goalTemplatesUIEnabled",
  "actionTemplating",
  "formulaMode",
  "currency",
  "balanceForecastReport",
  "customThemes",
  "budgetAnalysisReport",
  "sankeyReport",
  "mobileCalculator",
  "monteCarloReport",
] as const;
export type FeatureName = (typeof featureNames)[number];
export interface SharedPreferences {
  sync_transfer_date?: boolean;
  privacy_mode?: boolean;
  number_format: NumberFormat;
  date_format: DateFormat;
  hide_fraction: boolean;
  first_day_of_week: number;
  currency_code: string;
  currency_symbol_position: "before" | "after";
  currency_space: boolean;
  experimental: Record<FeatureName, boolean>;
  revision: number;
}
export interface DevicePreferences {
  theme: "light" | "dark" | "midnight" | "system";
  preferred_dark_theme: "dark" | "midnight";
  language: string;
  show_release_notices?: boolean;
  seen_release?: string;
}
export const defaultShared = (): SharedPreferences => ({
  number_format: "comma-dot",
  date_format: "MM/dd/yyyy",
  hide_fraction: false,
  privacy_mode: false,
  first_day_of_week: 0,
  currency_code: "",
  currency_symbol_position: "before",
  currency_space: false,
  experimental: Object.fromEntries(
    featureNames.map((name) => [name, false]),
  ) as Record<FeatureName, boolean>,
  revision: 0,
});
export const defaultDevice = (): DevicePreferences => ({
  theme: "system",
  preferred_dark_theme: "dark",
  language: "",
});
