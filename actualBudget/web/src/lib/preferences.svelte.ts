import { readBudget } from "./offline/cache";
import { errorMessage, request } from "./api";
import { setLanguage, languages } from "./i18n.svelte";
import {
  defaultDevice,
  defaultShared,
  type DevicePreferences,
  type SharedPreferences,
} from "./preferences/types";
import { validShared } from "./preferences/validation";
const deviceKey = "actualBudget.devicePreferences.v1";
const cacheKey = (budget: string) =>
  `actualBudget.preferencesCache.v1.${budget}`;
function validDevice(value: unknown): value is DevicePreferences {
  if (!value || typeof value !== "object") return false;
  const v = value as DevicePreferences;
  return (
    (v.show_release_notices === undefined ||
      typeof v.show_release_notices === "boolean") &&
    (v.seen_release === undefined ||
      (typeof v.seen_release === "string" && v.seen_release.length <= 128)) &&
    ["light", "dark", "midnight", "system"].includes(v.theme) &&
    ["dark", "midnight"].includes(v.preferred_dark_theme) &&
    (v.language === "" || (languages as readonly string[]).includes(v.language))
  );
}
class Preferences {
  shared = $state.raw<SharedPreferences>(defaultShared());
  device = $state.raw<DevicePreferences>(defaultDevice());
  budgetId = $state("");
  error = $state("");
  deviceError = $state("");
  loading = $state(false);
  hasUsableShared = $state(false);
  cachedAt = $state<string | null>(null);
  private generation = 0;
  private languageGeneration = 0;
  private media: MediaQueryList | null = null;
  startDevice(): () => void {
    try {
      const stored = localStorage.getItem(deviceKey);
      if (stored) {
        const record = JSON.parse(stored);
        if (record.version !== 1 || !validDevice(record.values))
          throw new Error(
            "Saved device preferences are invalid; defaults are in use.",
          );
        this.device = record.values;
      }
    } catch (error) {
      this.deviceError = errorMessage(error);
    }
    this.media = matchMedia("(prefers-color-scheme: dark)");
    this.applyTheme();
    const changed = () => this.applyTheme();
    this.media.addEventListener("change", changed);
    void this.retryLanguage();
    return () => this.media?.removeEventListener("change", changed);
  }
  private applyTheme() {
    const theme =
      this.device.theme === "system"
        ? this.media?.matches
          ? this.device.preferred_dark_theme
          : "light"
        : this.device.theme;
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme =
      theme === "light" ? "light" : "dark";
  }
  async retryLanguage() {
    const generation = ++this.languageGeneration;
    try {
      await setLanguage(this.device.language);
      if (generation !== this.languageGeneration) return;
      if (this.deviceError.startsWith("Language preference is saved"))
        this.deviceError = "";
    } catch {
      if (generation !== this.languageGeneration) return;
      this.deviceError =
        "Language preference is saved, but translations could not load. Check your connection and retry.";
    }
  }
  async updateDevice(changes: Partial<DevicePreferences>): Promise<void> {
    const next = { ...this.device, ...changes };
    if (!validDevice(next)) {
      this.deviceError = "Choose a supported device preference.";
      return;
    }
    try {
      localStorage.setItem(
        deviceKey,
        JSON.stringify({ version: 1, values: next }),
      );
    } catch {
      this.deviceError =
        "Cannot save device preferences to browser storage. Enable storage or free space, then retry.";
      this.device = { ...this.device };
      return;
    }
    this.device = next;
    this.deviceError = "";
    this.applyTheme();
    await this.retryLanguage();
  }
  budgetEpoch = $state<string | undefined>(undefined);
  useCachedBudget(
    budgetId: string,
    values: SharedPreferences,
    savedAt: string,
    epoch?: string,
  ) {
    if (!validShared(values))
      throw new Error("Saved display preferences are invalid.");
    // Registers open from saved snapshots before HTTP refresh. An older snapshot
    // in this epoch must not undo confirmed formatting preferences.
    if (
      this.hasUsableShared &&
      this.budgetId === budgetId &&
      this.budgetEpoch === epoch &&
      this.shared.revision > values.revision
    )
      return;
    this.generation++;
    this.budgetId = budgetId;
    this.shared = values;
    this.cachedAt = savedAt;
    this.budgetEpoch = epoch;
    this.hasUsableShared = true;
    this.loading = false;
    this.error = "";
  }
  async loadBudget(budgetId: string): Promise<void> {
    const generation = ++this.generation;
    const keepCurrent = this.budgetId === budgetId && this.hasUsableShared;
    this.budgetId = budgetId;
    this.loading = true;
    this.error = "";
    // A same-budget refresh keeps confirmed values until the HTTP read completes.
    // Resetting to defaults or an older cache can briefly show incorrect formatting.
    if (!keepCurrent) {
      this.hasUsableShared = false;
      this.cachedAt = null;
      this.shared = defaultShared();
    }
    try {
      const offline = await readBudget(budgetId);
      if (generation !== this.generation) return;
      if (offline && !keepCurrent) {
        this.shared = offline.preferences;
        this.hasUsableShared = true;
        this.cachedAt = offline.savedAt;
        this.budgetEpoch = offline.epoch;
      }
      const cache = JSON.parse(
        localStorage.getItem(cacheKey(budgetId)) ?? "null",
      );
      if (
        !this.hasUsableShared &&
        cache?.version === 1 &&
        cache.budget_id === budgetId &&
        validShared(cache.values) &&
        typeof cache.saved_at === "string"
      ) {
        this.shared = cache.values;
        this.hasUsableShared = true;
        this.cachedAt = cache.saved_at;
      }
    } catch {
      /* A readable server remains usable without a local cache. */
    }
    try {
      const { data: checkpoint } = await request<{
        data: { epoch: string; snapshot: string };
      }>(`/budgets/${budgetId}/checkpoint`);
      const result = await request<{ data: SharedPreferences }>(
        `/budgets/${budgetId}/preferences?${new URLSearchParams({ snapshot: checkpoint.snapshot })}`,
      );
      if (generation !== this.generation) return;
      if (!validShared(result.data))
        throw new Error("Server returned unsupported display preferences.");
      this.budgetEpoch = checkpoint.epoch;
      this.shared = result.data;
      this.hasUsableShared = true;
      this.cachedAt = new Date().toISOString();
      try {
        localStorage.setItem(
          cacheKey(budgetId),
          JSON.stringify({
            version: 1,
            budget_id: budgetId,
            saved_at: this.cachedAt,
            values: result.data,
          }),
        );
      } catch {
        /* Cache is optional; mutations are protected separately. */
      }
    } catch (error) {
      if (generation === this.generation) this.error = errorMessage(error);
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }
  async saveShared(
    budgetId: string,
    changes: Record<string, unknown>,
    key: string,
    epoch?: string,
  ): Promise<SharedPreferences> {
    const { data } = await request<{ data: SharedPreferences }>(
      `/budgets/${budgetId}/preferences`,
      changes,
      key,
      "PATCH",
      undefined,
      false,
      epoch,
    );
    if (!validShared(data))
      throw new Error("Server returned unsupported display preferences.");
    if (epoch && budgetId === this.budgetId && epoch !== this.budgetEpoch)
      return data; // A receipt from before restore must not replace current preferences.
    if (budgetId === this.budgetId) {
      this.shared = data;
      this.hasUsableShared = true;
      this.cachedAt = new Date().toISOString();
    }
    try {
      localStorage.setItem(
        cacheKey(budgetId),
        JSON.stringify({
          version: 1,
          budget_id: budgetId,
          saved_at: new Date().toISOString(),
          values: data,
        }),
      );
    } catch {
      this.error =
        "Saved on the server, but the local preferences cache could not be updated.";
    }
    return data;
  }
  clearBudget() {
    this.generation++;
    this.budgetId = "";
    this.hasUsableShared = false;
    this.budgetEpoch = undefined;
    this.shared = defaultShared();
    this.error = "";
    this.cachedAt = null;
    this.loading = false;
  }
}
export const preferences = new Preferences();
