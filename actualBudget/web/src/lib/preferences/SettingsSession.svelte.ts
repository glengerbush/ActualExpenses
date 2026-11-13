import { offlineStatus } from "../offline/events";
import { errorMessage, request, RequestError, uuid, type Budget } from "../api";
import { preferences } from "../preferences.svelte";
import { defaultShared, type SharedPreferences } from "./types";
import { validShared } from "./validation";
export interface PreferenceDraft {
  version: 1;
  origin: string;
  budget_id: string;
  draft_id: string;
  budget_epoch?: string;
  base: SharedPreferences;
  values: SharedPreferences;
  pending: { key: string; changes: Record<string, unknown> } | null;
}
const key = (budget: string) => `actualBudget.preferencesDraft.v1.${budget}`;
const equal = (a: unknown, b: unknown) =>
  JSON.stringify(a) === JSON.stringify(b);
function withoutRetiredFeatures(draft: PreferenceDraft): PreferenceDraft {
  const clean = (values: SharedPreferences): SharedPreferences => {
    const experimental = { ...values.experimental } as Record<string, boolean>;
    delete experimental.enableBanking;
    delete experimental.akahuBankSync;
    delete experimental.goalTemplatesEnabled;
    delete experimental.goalTemplatesUIEnabled;
    return {
      ...values,
      experimental: experimental as SharedPreferences["experimental"],
    };
  };
  return { ...draft, base: clean(draft.base), values: clean(draft.values) };
}
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
    (d.budget_epoch !== undefined && !/^[a-f0-9]{32}$/.test(d.budget_epoch)) ||
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
  if (stored && stored.budget_epoch !== draft.budget_epoch)
    throw new Error(
      "These settings belong to an earlier budget version. Review them before making a new change.",
    );
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
  blocked = $state(false);
  review = $state.raw<{
    budget: Budget;
    values: SharedPreferences | null;
  } | null>(null);
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
        budget_epoch: preferences.budgetEpoch,
        base: structuredClone($state.snapshot(preferences.shared)),
        values: structuredClone($state.snapshot(preferences.shared)),
        pending: null,
      };
      // Never rewrite the body/key of an unconfirmed request. Once resolved,
      // keep unrelated settings edits while removing retired connector flags.
      if (!this.draft.pending) this.draft = withoutRetiredFeatures(this.draft);
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
    let acknowledged = false;
    if (!this.draft.pending && !preferences.hasUsableShared) {
      this.error =
        "Load this budget’s preferences before changing shared settings.";
      return;
    }
    try {
      if (!this.draft.pending && !offlineStatus.reachable)
        throw new Error(
          "Reconnect before changing shared settings. Your changes are saved on this device.",
        );
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
        submitted.budget_epoch,
      );
      acknowledged = true;
      const checkpoint = await request<{
        data: { epoch: string; snapshot: string };
      }>(`/budgets/${this.budgetId}/checkpoint`);
      const current = await request<{ data: SharedPreferences }>(
        `/budgets/${this.budgetId}/preferences?snapshot=${encodeURIComponent(checkpoint.data.snapshot)}`,
      );
      await preferences.loadBudget(this.budgetId);
      this.remove(submitted);
      this.draft = {
        ...submitted,
        draft_id: uuid(),
        budget_epoch: checkpoint.data.epoch,
        base: current.data,
        values: current.data,
        pending: null,
      };
      this.status = equal(saved, current.data)
        ? "Shared preferences saved."
        : "Earlier settings change acknowledged. Showing the current restored settings.";
    } catch (error) {
      if (
        error instanceof RequestError &&
        !acknowledged &&
        !error.uncertain &&
        error.code !== "budget_restored" &&
        error.code !== "budget_deleted" &&
        this.draft.pending
      ) {
        try {
          const editable = withoutRetiredFeatures({
            ...this.draft,
            pending: null,
          });
          write(editable, this.draft.pending);
          this.draft = editable;
        } catch (storageError) {
          this.error = errorMessage(storageError);
        }
      }
      this.blocked =
        error instanceof RequestError &&
        (error.code === "budget_restored" || error.code === "budget_deleted");
      this.error ||= errorMessage(error);
    } finally {
      this.busy = false;
    }
  }
  async reviewCurrent() {
    if (!this.draft || !this.blocked || this.busy) return;
    this.busy = true;
    try {
      const { data: budget } = await request<{ data: Budget }>(
        `/budgets/${this.budgetId}?include_deleted=true`,
      );
      const values = (budget as Budget & { deleted?: boolean }).deleted
        ? null
        : (
            await request<{ data: SharedPreferences }>(
              `/budgets/${this.budgetId}/preferences`,
            )
          ).data;
      this.review = { budget, values };
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.busy = false;
    }
  }
  resolveReviewed(reapply: boolean): boolean {
    const original = this.draft;
    if (!original || !this.blocked || !this.review || this.busy) return false;
    try {
      if (reapply && !this.review.values)
        throw new Error(
          "Restore this deleted budget before changing settings.",
        );
      localStorage.setItem(
        `actualBudget.retiredSettings.v1.${original.draft_id}`,
        JSON.stringify(original),
      );
      this.remove(original);
      if (reapply && this.review.values) {
        const changes = changesBetween(original.base, original.values),
          base = this.review.values;
        const values = {
          ...base,
          ...changes,
          experimental: {
            ...base.experimental,
            ...((changes.experimental as Record<string, boolean>) ?? {}),
          },
        };
        this.draft = {
          ...original,
          draft_id: uuid(),
          budget_epoch: this.review.budget.epoch,
          base,
          values,
          pending: null,
        };
        write(this.draft);
        this.error = "";
        this.blocked = false;
        this.review = null;
        this.status = "Review these settings, then save them as a new change.";
      } else this.draft = null;
      return true;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }
}
