export type BaseTheme = "light" | "dark" | "midnight";
export interface ThemeWarning {
  key: string;
  reason: string;
}
export interface CompiledTheme {
  version: 1 | 2;
  source_hash?: string;
  id: string;
  name: string;
  repo: string;
  base_theme: BaseTheme;
  source: string;
  valid: boolean;
  variables: Record<string, string>;
  variables_by_base: Record<BaseTheme, Record<string, string>>;
  fonts: string[];
  warnings: ThemeWarning[];
}
export interface ThemeRecord {
  version: 1;
  enabled: boolean;
  light: CompiledTheme | null;
  dark: CompiledTheme | null;
  override: CompiledTheme | null;
  original: Record<string, unknown>;
  warnings: ThemeWarning[];
}
export interface ThemeDraft {
  version: 1;
  slot: "light" | "dark" | "override";
  name: string;
  base_theme: BaseTheme;
  css: string;
}
export interface CatalogTheme {
  name: string;
  repo: string;
  colors?: string[];
  mode: "light" | "dark";
}
export const emptyRecord = (): ThemeRecord => ({
  version: 1,
  enabled: false,
  light: null,
  dark: null,
  override: null,
  original: {},
  warnings: [],
});
