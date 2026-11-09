import { request, errorMessage } from "../api";
import {
  emptyRecord,
  type BaseTheme,
  type CompiledTheme,
  type ThemeRecord,
} from "./types";
import { readThemeValue, writeThemeValue } from "./storage";
const targets = [
  "--page",
  "--text",
  "--muted",
  "--link",
  "--table-surface",
  "--table-text",
  "--line",
  "--table-hover",
  "--surface",
  "--accent",
  "--primary-text",
  "--primary-hover",
  "--button-text",
  "--button-surface",
  "--hover",
  "--button-border",
  "--control-border",
  "--focus",
  "--input-surface",
  "--input-text",
  "--selection",
  "--selection-text",
  "--sidebar",
  "--sidebar-text",
  "--sidebar-title",
  "--selected",
  "--selected-text",
  "--error",
  "--error-bg",
  "--font-family",
];
function validTheme(value: unknown): value is CompiledTheme {
  if (!value || typeof value !== "object") return false;
  const v = value as CompiledTheme;
  return (
    v.version === 1 &&
    typeof v.source === "string" &&
    v.source.length <= 16 * 1024 * 1024 &&
    typeof v.name === "string" &&
    typeof v.id === "string" &&
    typeof v.repo === "string" &&
    ["light", "dark", "midnight"].includes(v.base_theme) &&
    typeof v.valid === "boolean" &&
    !!v.variables &&
    typeof v.variables === "object" &&
    Object.entries(v.variables).every(
      ([key, value]) =>
        targets.includes(key) &&
        typeof value === "string" &&
        value.length <= 65536 &&
        !/[<>;{}\\]|url\s*\(|expression\s*\(/i.test(value),
    ) &&
    !!v.variables_by_base &&
    ["light", "dark", "midnight"].every((base) => {
      const vars = v.variables_by_base[base as BaseTheme];
      return (
        !!vars &&
        Object.entries(vars).every(
          ([key, value]) =>
            targets.includes(key) &&
            typeof value === "string" &&
            value.length <= 65536 &&
            !/[<>;{}\\]|url\s*\(|expression\s*\(/i.test(value),
        )
      );
    }) &&
    Array.isArray(v.fonts) &&
    v.fonts.reduce(
      (size, font) => size + (typeof font === "string" ? font.length : 0),
      0,
    ) <=
      15 * 1024 * 1024 &&
    v.fonts.every(
      (font) =>
        typeof font === "string" &&
        font.length <= 3 * 1024 * 1024 &&
        /^@font-face\s*\{[^{}<>\\]*\}$/s.test(font) &&
        !/@(?!font-face)/i.test(font) &&
        [...font.matchAll(/url\(\s*(['"]?)(.*?)\1\s*\)/g)].every((match) =>
          match[2].startsWith("data:"),
        ),
    ) &&
    Array.isArray(v.warnings) &&
    v.warnings.every(
      (w) => typeof w.key === "string" && typeof w.reason === "string",
    )
  );
}
function validRecord(value: unknown): value is ThemeRecord {
  if (!value || typeof value !== "object") return false;
  const v = value as ThemeRecord;
  return (
    v.version === 1 &&
    typeof v.enabled === "boolean" &&
    [v.light, v.dark, v.override].every(
      (theme) => theme === null || validTheme(theme),
    ) &&
    !!v.original &&
    typeof v.original === "object" &&
    Array.isArray(v.warnings) &&
    v.warnings.every(
      (w) => typeof w.key === "string" && typeof w.reason === "string",
    )
  );
}
class Themes {
  record = $state.raw<ThemeRecord>(emptyRecord());
  error = $state("");
  loading = $state(false);
  ready = $state(false);
  private base: BaseTheme = "light";
  private system = false;
  private systemDark = false;
  private fontStyle: HTMLStyleElement | null = null;
  private raw: unknown;
  async start() {
    this.loading = true;
    try {
      const value = await readThemeValue<unknown>("active");
      this.raw = value;
      if (value !== undefined && !validRecord(value))
        throw new Error(
          "Saved custom themes are malformed. Built-in appearance is active; export the original data before replacing it.",
        );
      this.record = (value as ThemeRecord) ?? emptyRecord();
      this.ready = true;
      this.error = "";
      this.apply();
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.loading = false;
    }
  }
  setBase(base: BaseTheme, system: boolean, systemDark: boolean) {
    this.base = base;
    this.system = system;
    this.systemDark = systemDark;
    this.apply();
  }
  private apply() {
    const selected =
      this.system && this.systemDark ? this.record.dark : this.record.light;
    const enabled = this.ready && this.record.enabled;
    document.documentElement.dataset.theme =
      enabled && selected?.valid ? selected.base_theme : this.base;
    document.documentElement.style.colorScheme =
      document.documentElement.dataset.theme === "light" ? "light" : "dark";
    for (const key of targets)
      document.documentElement.style.removeProperty(key);
    this.fontStyle?.remove();
    this.fontStyle = null;
    if (!enabled) return;
    const fonts: string[] = [];
    for (const theme of [selected, this.record.override]) {
      if (!theme?.valid) continue;
      for (const [key, value] of Object.entries(
        theme === this.record.override
          ? theme.variables_by_base[
              document.documentElement.dataset.theme as BaseTheme
            ]
          : theme.variables,
      ))
        document.documentElement.style.setProperty(key, value);
      fonts.push(...theme.fonts);
    }
    if (fonts.length) {
      const style = document.createElement("style");
      style.dataset.actualbudgetTheme = "fonts";
      style.textContent = fonts.join("\n");
      document.head.append(style);
      this.fontStyle = style;
    }
  }
  async save(record: ThemeRecord) {
    if (!this.ready && this.raw === undefined) {
      await this.start();
      if (!this.ready && this.raw === undefined)
        throw new Error(
          "Recover readable theme storage before replacing saved themes.",
        );
      // The caller built its intended change before this read completed. Never
      // replace recovered sources using that stale fallback; an explicit retry
      // rebuilds reset/install from the now-loaded record.
      if (this.raw !== undefined)
        throw new Error(
          "Saved theme data recovered. Retry your action to use it; no saved source was replaced.",
        );
    }
    if (!this.ready && this.raw !== undefined)
      record = {
        ...record,
        original: { ...record.original, unreadable_saved_record: this.raw },
      };
    if (!validRecord(record))
      throw new Error("Unsupported theme response. Existing theme kept.");
    await writeThemeValue("active", record);
    this.record = record;
    this.raw = record;
    this.ready = true;
    this.error = "";
    this.apply();
  }
  async install(
    slot: "light" | "dark" | "override",
    theme: CompiledTheme | null,
    originalSource?: string,
  ) {
    const original = { ...this.record.original };
    if (theme && originalSource !== undefined) {
      const old = original.catalog_sources;
      original.catalog_sources = {
        ...(old && typeof old === "object" ? old : {}),
        [theme.repo]: { source: originalSource, repo: theme.repo },
      };
    }
    await this.save({ ...this.record, enabled: true, [slot]: theme, original });
  }
  async clearInstalledSlots(darkOnly = false) {
    const { light, dark } = this.record;
    if ((!light || darkOnly) && !dark) return;
    await this.save({
      ...this.record,
      light: darkOnly ? light : null,
      dark: null,
      enabled: !!this.record.override || (darkOnly && !!light),
      original: {
        ...this.record.original,
        replaced_installed_slots: { light: darkOnly ? null : light, dark },
      },
    });
  }
  async reset() {
    try {
      const record = { ...this.record, enabled: false };
      if (!this.ready && this.raw !== undefined)
        record.original = {
          ...record.original,
          unreadable_saved_record: this.raw,
        };
      await this.save(record);
    } catch (error) {
      this.error = errorMessage(error);
      this.record = { ...this.record, enabled: false };
      this.apply();
    }
  }
  export() {
    return this.raw ?? this.record;
  }
  async compile(
    css: string,
    base_theme: BaseTheme,
    name = "Custom theme",
    repo = "",
  ): Promise<CompiledTheme> {
    const { data } = await request<{ data: CompiledTheme }>("/themes/compile", {
      css,
      base_theme,
      name,
      repo,
      id: repo || "pasted",
    });
    if (!validTheme(data))
      throw new Error("Unsupported theme compiler response.");
    return data;
  }
  async restore(value: unknown) {
    if (!validRecord(value))
      throw new Error(
        "Unsupported saved theme data. Original input remains in the editor.",
      );
    const record = { ...value };
    for (const slot of ["light", "dark", "override"] as const) {
      const theme = value[slot];
      if (theme)
        record[slot] = await this.compile(
          theme.source,
          theme.base_theme,
          theme.name,
          theme.repo,
        );
    }
    await this.save(record);
  }
  async convert(global: unknown): Promise<ThemeRecord> {
    const { data } = await request<{ data: Omit<ThemeRecord, "enabled"> }>(
      "/themes/convert-actual",
      { global },
    );
    const record = { ...data, enabled: true };
    if (!validRecord(record))
      throw new Error("Unsupported theme conversion response.");
    return record;
  }
}
export const themes = new Themes();
export function exportJson(value: unknown, name: string) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(value, null, 2) + "\n"], {
      type: "application/json",
    }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
