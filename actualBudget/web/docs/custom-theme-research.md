# Ticket44 preparation — Actual26.10.0 custom themes

Read-only production research; no runtime implementation in this preparation. Authoritative ticket: **44 — Install and migrate custom themes**; parent story76 and preference migration requirements. Baseline commit2bebdbaae0b701120a8cad9d56aafd64fa04c577.

## Baseline behavior and schema

The pinned catalog contains **33 choices**, each `{name,repo,colors?,mode:'light'|'dark'}`. Catalog installation fetches `actual.css` from the selected repository's `main` branch, embeds relative font files, validates CSS, then stores the downloaded content. Fetch occurs only on explicit installation; installed themes do not require another download at startup. Source: [catalog](../../reference/actual-26.10.0/packages/desktop-client/src/data/customThemeCatalog.json), [installer](../../reference/actual-26.10.0/packages/desktop-client/src/components/settings/ThemeInstaller.tsx), [fetch/embed helpers](../../reference/actual-26.10.0/packages/desktop-client/src/style/customThemes.ts).

Raw `global-store.json` keys are `installed-custom-theme`, `installed-custom-dark-theme`, `custom-css-override`. Parsed API aliases are `installedCustomLightTheme`, `installedCustomDarkTheme`, `customCssOverride`. Each installed field is a JSON **string**, whose object is `{id,name,repo,cssContent,baseTheme?}`; optional baseTheme is light/dark/midnight. Preserve the original string, including unknown fields and malformed JSON, independently of parsed display state. Source: [GlobalPrefs/GlobalPrefsJson](../../reference/actual-26.10.0/packages/loot-core/src/types/prefs.ts), [parseInstalledTheme](../../reference/actual-26.10.0/packages/desktop-client/src/style/customThemes.ts).

Non-system mode uses the light/single installed slot regardless of built-in selection. System mode uses separate light/dark slots according to OS preference, with each slot's baseTheme overriding the contextual built-in fallback. A standalone override applies last in both modes. Changing to a built-in theme removes installed slots in baseline UI. ThemeSettings/ThemeStyle do not gate these capabilities on the retained `customThemes` experimental flag. Source: [Themes](../../reference/actual-26.10.0/packages/desktop-client/src/components/settings/Themes.tsx), [ThemeStyle/CustomThemeStyle](../../reference/actual-26.10.0/packages/desktop-client/src/style/theme.tsx).

Legacy `overrideCss` nested inside an installed object migrates to standalone override: existing nonblank standalone wins; otherwise light wins over dark. Baseline drops the losing override; rewrite requirement is stronger: preserve all raw records and report the collision. Parsing strips unknown fields in baseline, so using only its parsed result would violate source-preservation requirements. Source: [migrateLegacyOverride](../../reference/actual-26.10.0/packages/desktop-client/src/style/customThemes.ts).

## Supported CSS boundary

Executable validation, not older prose docs, is authoritative. It allows one `:root` variable block, optional preceding embedded `@font-face` blocks, comments stripped, arbitrary valid custom-property names and safe literal values/simple `var(--name)` references. No fallback arguments, additional selectors, nested rules, imports, arbitrary functions, or ordinary declarations. Literal named color `red` fails even though browsers accept it. `--font-*` accepts font-family lists; the actually consumed baseline variable is `--font-family`. Embedded fonts allow data URIs, at most2MiB each/10MiB total. Catalog installation can embed relative same-repository woff/woff2/ttf/otf files, with15s per-font timeout; absolute paths, traversal, remote font URLs reject. Source: [validateThemeCss/embedThemeFonts](../../reference/actual-26.10.0/packages/desktop-client/src/style/customThemes.ts), [font usage](../../reference/actual-26.10.0/packages/desktop-client/index.html). [User docs](../../reference/actual-26.10.0/packages/docs/docs/custom-themes.md) omit the newer embedded-font allowance.

Proposed rewrite boundary: retain supported variable/font behavior, parse to structured declarations, apply only translated tokens and permitted embedded fonts. Preserve unsupported original content with explicit warnings and a readable built-in fallback; never inject unsupported selectors. Unknown variables that are dependencies of mapped variables remain usable; unknown unused widget variables receive a per-token warning. Broken references/cycles likewise warn and retain base value. Do not fetch arbitrary source URLs during migration or every page load.

## Token mapping plan

Baseline inventories:93 palette tokens;236 light,234 dark,237 midnight tokens;27 sidebar-redesign tokens. Preserve baseline palette/base-variable reference resolution; it is insufficient to replace names without resolving `var()` chains. The full source catalogs remain immutable. Candidate semantic mappings (existing Svelte token on right):

| Baseline variable | Svelte token |
|---|---|
| `--color-pageBackground` | `--page` |
| `--color-pageText` | `--text` |
| `--color-pageTextSubdued` | `--muted` |
| `--color-tableBackground` | `--surface` (split table/control surface if distinct values conflict) |
| `--color-tableBorder` | `--line` |
| `--color-tableRowBackgroundHover` | `--hover` (split table/button hover where necessary) |
| `--color-buttonPrimaryBackground` | `--accent` (split link/accent if needed) |
| `--color-buttonPrimaryText` | `--primary-text` |
| `--color-buttonPrimaryBackgroundHover` | `--primary-hover` |
| `--color-buttonNormalText` | `--button-text` |
| `--color-buttonNormalBorder` | `--button-border` |
| `--color-formInputBorder` | `--control-border` |
| `--color-formInputBorderSelected` | `--focus` |
| `--color-formInputBackgroundSelection` | `--selection` |
| `--color-formInputTextSelection` | `--selection-text` |
| `--color-sidebarBackground` / redesign equivalent | `--sidebar` |
| `--color-sidebarItemBackgroundSelected` / redesign equivalent | `--selected` |
| `--color-sidebarItemTextSelected` / redesign equivalent | `--selected-text` |
| `--color-errorText` | `--error` |
| `--color-errorBackground` | `--error-bg` |
| `--font-family` | root font stack |

Current Svelte semantic tokens merge concerns that Actual separates. Add narrowly scoped semantic tokens for sidebar text/backgrounds, table text/surface and form text/surface where needed, rather than choose an arbitrary winner and claim complete fidelity. Chart/report tokens are recognized/preserved for later report modules; until actually consumed, report them as unapplied. Native Svelte token overrides may be accepted with explicit recognized-token allowlist. Source: [baseline theme files](../../reference/actual-26.10.0/packages/component-library/src/themes/light.css), [redesign aliases](../../reference/actual-26.10.0/packages/component-library/src/themes/sidebar-redesign-light.css), [current CSS](../src/styles.css).

## Proposed implementation contract

Keep themes device-scoped. Store a versioned device theme record containing both slots, standalone override, exact original sources, compiler version, warnings, and compiled token/font data. IndexedDB is preferable to localStorage because supported embedded fonts can exceed common localStorage quotas. Save before applying; failed quota/storage writes leave the previous active theme usable and retain the editor text. Validated compiled data can be rederived after upgrades from original source. A separate reset/recovery control must remain readable even if user colors have poor contrast.

Extend the existing public preference conversion response with a device-theme conversion result, preserving original source and structured warnings. A stateless public theme-compile endpoint can serve browser installation/import and publish a versioned token catalog; it must not persist device appearance on the server or fetch remote URLs. The browser fetches a catalog choice only on explicit user action; pasted input requires no internet. Exact endpoint/schema remain an implementation decision after43 acceptance.

04 owner confirmed ordinary Actual ZIP contains metadata.json+SQLite, not device globals. Only separately supplied globals may include theme records; retain those originals and call the43/44 converters. Never fabricate device values from archive contents or put them into budget-shared settings.

## Independent reference fixtures

`node tests/generate-theme-fixture.mjs --check` executes immutable baseline TypeScript helpers in an isolated VM without network access. Fixture `tests/fixtures/custom-themes-26.10.0.json` records source hashes,33 catalog entries and token inventories,15 CSS validation cases,6 installed-record parse cases and5 override migration cases. Cases include valid variable reference/font, unsupported selector/remote font/fallback variable, malformed record, unknown base, existing override precedence and light/dark collision.

Future public acceptance should compare these outcomes and verify catalog fetch failure/abort, pasted CSS warnings, exact raw export/reload, malformed stored recovery, light/dark/system selection, unsupported-token visibility, explicit reset, keyboard/mobile/large text, and no startup network fetch after installation. No production files changed by this preparation.

Implementation and final acceptance evidence: [custom-theme contract](custom-theme-contract.md). The separate theme conversion endpoint keeps device-only data outside shared budget preferences.
