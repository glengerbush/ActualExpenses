import { errorMessage, RequestError, uuid } from "../api";
import { preferences } from "../preferences.svelte";
import { defaultShared, type SharedPreferences } from "./types";
import { validShared } from "./validation";
export interface PreferenceDraft {
  version: 1;
  origin: string;
  budget_id: string;
  draft_id: string;
  base: SharedPreferences;
  values: SharedPreferences;
  pending: { key: string; changes: Record<string, unknown> } | null;
}
const key = (budget: string) => `actualBudget.preferencesDraft.v1.${budget}`;
const equal = (a: unknown, b: unknown) =>
  JSON.stringify(a) === JSON.stringify(b);
function changesBetween(
  base: SharedPreferences,
  values: SharedPreferences,
): Record<string, unknown> {
  const changes: Record<string, unknown> = {};
  for (const name of Object.keys(
    defaultShared(),
  ) as (keyof SharedPreferences)[]) {
    if (name === "revision") continue;
    if (name === "experimental") {
      const changed = Object.fromEntries(
        Object.entries(values.experimental).filter(
          ([flag, enabled]) =>
            base.experimental[flag as keyof typeof base.experimental] !==
            enabled,
        ),
      );
      if (Object.keys(changed).length) changes.experimental = changed;
    } else if (base[name] !== values[name]) changes[name] = values[name];
  }
  return changes;
}
function read(budgetId: string): PreferenceDraft | null {
  let raw: string | null;
  try {
    raw = localStorage.getItem(key(budgetId));
  } catch {
    throw new Error(
      "Cannot read browser storage. Enable it before changing shared preferences.",
    );
  }
  if (!raw) return null;
  let d: PreferenceDraft;
  try {
    d = JSON.parse(raw);
  } catch {
    throw new Error(
      "Saved preference changes are unreadable and have been kept unchanged.",
    );
  }
  if (
    !d ||
    d.version !== 1 ||
    d.origin !== location.origin ||
    d.budget_id !== budgetId ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
      d.draft_id,
    ) ||
    !validShared(d.base) ||
    !validShared(d.values) ||
    (d.pending !== null &&
      (!d.pending ||
        d.pending.key !== d.draft_id ||
        !equal(d.pending.changes, changesBetween(d.base, d.values))))
  )
    throw new Error(
      "Saved preference changes could not be validated and have been kept unchanged.",
    );
  return d;
}
function write(
  draft: PreferenceDraft,
  acknowledged?: PreferenceDraft["pending"],
) {
  const stored = read(draft.budget_id);
  if (stored && stored.draft_id !== draft.draft_id)
    throw new Error("Another settings draft is saved. Reload to recover it.");
  if (
    stored?.pending &&
    !equal(stored.pending, draft.pending) &&
    !equal(stored.pending, acknowledged)
  )
    throw new Error(
      "Another tab has unconfirmed settings. Reload before changing them.",
    );
  try {
    localStorage.setItem(key(draft.budget_id), JSON.stringify(draft));
  } catch {
    throw new Error(
      "Cannot save preference changes to browser storage. Keep this page open, enable storage or free space, then retry. No new request has been sent.",
    );
  }
}
export class SettingsSession {
  draft = $state.raw<PreferenceDraft | null>(null);
  error = $state("");
  status = $state("");
  busy = $state(false);
  constructor(readonly budgetId: string) {}
  load() {
    try {
      const stored = read(this.budgetId);
      if (!stored && !preferences.hasUsableShared)
        throw new Error(
          "Load this budget’s preferences before changing shared settings.",
        );
      this.draft = stored ?? {
        version: 1,
        origin: location.origin,
        budget_id: this.budgetId,
        draft_id: uuid(),
        base: structuredClone($state.snapshot(preferences.shared)),
        values: structuredClone($state.snapshot(preferences.shared)),
        pending: null,
      };
      write(this.draft);
      this.error = "";
      this.status = stored ? "Recovered your saved settings changes." : "";
    } catch (error) {
      this.error = errorMessage(error);
    }
  }
  update(changes: Partial<SharedPreferences>) {
    if (
      !this.draft ||
      this.draft.pending ||
      this.busy ||
      !preferences.hasUsableShared
    )
      return;
    this.draft = {
      ...this.draft,
      values: { ...this.draft.values, ...changes },
    };
    this.status = "";
    try {
      write(this.draft);
      this.error = "";
    } catch (error) {
      this.error = errorMessage(error);
    }
  }
  private remove(expected: PreferenceDraft) {
    const stored = read(this.budgetId);
    if (
      stored &&
      (stored.draft_id !== expected.draft_id ||
        (stored.pending && !equal(stored.pending, expected.pending)))
    )
      throw new Error(
        "Another settings change is saved. Reload before clearing it.",
      );
    try {
      localStorage.removeItem(key(this.budgetId));
    } catch {
      throw new Error(
        "Settings were acknowledged, but browser storage could not be cleared. Retry to confirm them.",
      );
    }
  }
  cancel(): boolean {
    if (!this.draft || this.draft.pending || this.busy) return false;
    try {
      this.remove(this.draft);
      return true;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }
  async save() {
    if (!this.draft || this.busy) return;
    this.error = "";
    this.status = "";
    if (!this.draft.pending && !preferences.hasUsableShared) {
      this.error =
        "Load this budget’s preferences before changing shared settings.";
      return;
    }
    try {
      const changes = changesBetween(this.draft.base, this.draft.values);
      if (!Object.keys(changes).length) {
        this.status = "No changes to save.";
        return;
      }
      const submitted = {
        ...this.draft,
        pending: this.draft.pending ?? { key: this.draft.draft_id, changes },
      };
      write(submitted);
      this.draft = submitted;
      this.busy = true;
      const saved = await preferences.saveShared(
        this.budgetId,
        submitted.pending.changes,
        submitted.pending.key,
      );
      this.remove(submitted);
      this.draft = {
        ...submitted,
        draft_id: uuid(),
        base: saved,
        values: saved,
        pending: null,
      };
      this.status = "Shared preferences saved.";
    } catch (error) {
      if (
        error instanceof RequestError &&
        !error.uncertain &&
        this.draft.pending
      ) {
        try {
          const editable = { ...this.draft, pending: null };
          write(editable, this.draft.pending);
          this.draft = editable;
        } catch (storageError) {
          this.error = errorMessage(storageError);
        }
      }
      this.error ||= errorMessage(error);
    } finally {
      this.busy = false;
    }
  }
}
