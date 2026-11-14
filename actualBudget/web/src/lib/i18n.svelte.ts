import englishPlurals from "../locales/english-plurals.json";
// Vite paths are relative to this module, so keep catalogs outside the main bundle.
const catalogUrls = import.meta.glob<string>(
  [
    "../locales/*.json",
    "!../locales/en.json",
    "!../locales/provenance.json",
    "!../locales/english-plurals.json",
  ],
  { query: "?url", import: "default", eager: true },
);
// Fetch (rather than module import) allows retry after a transient network failure.
const loadedCatalogs: Record<string, Record<string, string> | undefined> = {};
export const languages = [
  "ca",
  "de",
  "en-GB",
  "en",
  "es",
  "fr",
  "it",
  "nb-NO",
  "nl",
  "pl",
  "pt-BR",
  "ru",
  "uk",
  "zh-Hans",
  "zh-Hant",
] as const;
class LanguageState {
  language = $state("en");
  messages = $state.raw<Record<string, string>>({});
}
export const languageState = new LanguageState();
let generation = 0;
export function resolveLanguage(requested: string): string {
  const candidates = requested
    ? [requested]
    : navigator.languages?.length
      ? navigator.languages
      : [navigator.language || "en"];
  for (const value of candidates) {
    const exact = languages.find(
      (language) => language.toLowerCase() === value.toLowerCase(),
    );
    if (exact) return exact;
    const base = languages.find(
      (language) =>
        language.toLowerCase() === value.split("-")[0].toLowerCase(),
    );
    if (base) return base;
  }
  return "en";
}
// Shipped i18next catalogs contain dotted message keys represented as nested objects.
// Normalize only string leaves; application callers still use the original message key.
function flattenCatalog(catalog: unknown): Record<string, string> {
  const entries: Record<string, string> = Object.create(null);
  function visit(value: unknown, prefix: string[]) {
    if (
      !value ||
      typeof value !== "object" ||
      Array.isArray(value) ||
      prefix.length > 32
    )
      throw Error("Language catalog is invalid.");
    for (const [key, child] of Object.entries(value)) {
      const parts = [...prefix, key],
        path = parts.join(".");
      if (typeof child === "string") {
        if (Object.hasOwn(entries, path) && entries[path] !== child)
          throw Error("Language catalog contains conflicting message keys.");
        entries[path] = child;
      } else visit(child, parts);
    }
  }
  visit(catalog, []);
  return entries;
}
export async function setLanguage(requested: string): Promise<void> {
  const current = ++generation;
  const language = resolveLanguage(requested);
  let messages: Record<string, string> = {};
  if (language !== "en") {
    const url = catalogUrls[`../locales/${language}.json`];
    if (!url) throw new Error("Language catalog is unavailable.");
    const cached = loadedCatalogs[language];
    if (cached) messages = cached;
    else {
      const response = await fetch(url);
      if (!response.ok) throw new Error("Language catalog could not load.");
      const catalog: unknown = await response.json();
      messages = flattenCatalog(catalog);
      loadedCatalogs[language] = messages;
    }
  }
  if (current !== generation) return;
  languageState.messages = messages;
  languageState.language = language;
  document.documentElement.lang = language;
  document.documentElement.dir = "ltr";
}
// Semantically equivalent shipped labels; default English copy stays unchanged.
export const translationAliases: Record<string, string> = {
  "Your budgets": "Budget files",
  "Opening balance": "Starting balance",
  "Current balance": "Balance",
  "All-date balance": "Balance",
  "Create account": "Create new account",
  Retry: "Try again",
  "Retry creation": "Try again",
  "Retry saving": "Try again",
  "Save shared preferences": "Save changes",
  Dates: "Date format",
  Dark: "Dark theme",
};
export function t(
  key: string,
  params: Record<string, string | number> = {},
): string {
  const count = params.count;
  const plural =
    typeof count === "number"
      ? new Intl.PluralRules(languageState.language).select(count)
      : undefined;
  const englishPlural =
    typeof count === "number"
      ? new Intl.PluralRules("en").select(count)
      : undefined;
  const english = englishPlurals as Record<string, string>;
  const message =
    (plural ? languageState.messages[`${key}_${plural}`] : undefined) ??
    languageState.messages[key] ??
    languageState.messages[translationAliases[key]] ??
    (englishPlural ? english[`${key}_${englishPlural}`] : undefined) ??
    key;
  return message.replace(
    /\{\{\s*-?\s*([\w]+)\s*\}\}/g,
    (placeholder, name: string) =>
      Object.hasOwn(params, name) ? String(params[name]) : placeholder,
  );
}
