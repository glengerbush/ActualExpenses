import { currencies } from "./currencies";
import {
  dateFormats,
  featureNames,
  numberFormats,
  type SharedPreferences,
} from "./types";
export function validShared(value: unknown): value is SharedPreferences {
  if (!value || typeof value !== "object") return false;
  const v = value as SharedPreferences;
  return (
    numberFormats.includes(v.number_format) &&
    dateFormats.includes(v.date_format) &&
    (v.privacy_mode === undefined || typeof v.privacy_mode === "boolean") &&
    typeof v.hide_fraction === "boolean" &&
    Number.isInteger(v.first_day_of_week) &&
    v.first_day_of_week >= 0 &&
    v.first_day_of_week <= 6 &&
    currencies.some((c) => c.code === v.currency_code) &&
    ["before", "after"].includes(v.currency_symbol_position) &&
    typeof v.currency_space === "boolean" &&
    Number.isSafeInteger(v.revision) &&
    v.revision >= 0 &&
    !!v.experimental &&
    featureNames.every((name) => typeof v.experimental[name] === "boolean")
  );
}
