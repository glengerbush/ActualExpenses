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
export interface SharedPreferences {
  sync_transfer_date?: boolean;
  number_format: NumberFormat;
  date_format: DateFormat;
  hide_fraction: boolean;
  first_day_of_week: number;
  revision: number;
}
export interface DevicePreferences {
  theme: "light" | "dark" | "system";
  language: string;
  show_release_notices?: boolean;
  seen_release?: string;
}
export const defaultShared = (): SharedPreferences => ({
  number_format: "comma-dot",
  date_format: "MM/dd/yyyy",
  hide_fraction: false,
  first_day_of_week: 0,
  revision: 0,
});
export const defaultDevice = (): DevicePreferences => ({
  theme: "system",
  language: "",
});
