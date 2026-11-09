import { offlineStatus } from "./offline/events";
import {
  errorMessage,
  request,
  RequestError,
  uuid,
  type Account,
  type Budget,
} from "./api";
import {
  clearDraft,
  prepareSubmission,
  readDraft,
  saveDraft,
  type CreationDraft,
  type DraftScope,
} from "./drafts";

/** One recoverable creation form. Persistence precedes every network mutation. */
export class CreationSession<D extends CreationDraft, Result> {
  draft = $state.raw<D | null>(null);
  busy = $state(false);
  error = $state("");
  restored = $state(false);
  status = $state("");
  blocked = $state(false);
  review = $state.raw<{ budget: Budget; accounts: Account[] } | null>(null);
  constructor(
    private readonly create: () => D,
    private readonly scope?: () => DraftScope,
  ) {}
  load() {
    this.error = "";
    try {
      const saved = readDraft(this.scope ? this.scope() : this.create());
      this.draft = (saved ?? this.create()) as D;
      this.restored = saved !== null;
      saveDraft(this.draft);
    } catch (error) {
      this.error = errorMessage(error);
    }
  }
  update(patch: Partial<D["fields"]>) {
    if (!this.draft || this.draft.pending || this.busy) return;
    this.draft = { ...this.draft, fields: { ...this.draft.fields, ...patch } };
    try {
      saveDraft(this.draft);
      this.error = "";
    } catch (error) {
      this.error = errorMessage(error);
    }
  }
  cancel(): boolean {
    if (!this.draft || this.draft.pending || this.busy) return false;
    try {
      clearDraft(this.draft);
      return true;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }
  async submit(): Promise<Result | undefined> {
    if (!this.draft || this.busy) return;
    this.error = "";
    let acknowledged = false;
    try {
      if (!this.draft.pending && !offlineStatus.reachable)
        throw new Error(
          "Reconnect before creating a budget or account. Your draft is saved.",
        );
      const submitted = prepareSubmission(this.draft) as D;
      saveDraft(submitted);
      this.draft = submitted;
      this.busy = true;
      const path =
        submitted.kind === "budget"
          ? "/budgets"
          : `/budgets/${submitted.budget_id}/accounts`;
      const result = await request<{ data: Result }>(
        path,
        submitted.pending!.body,
        submitted.pending!.key,
        "POST",
        undefined,
        false,
        submitted.kind === "account" ? submitted.budget_epoch : undefined,
      );
      acknowledged = true;
      if (submitted.kind === "account") {
        const current = await request<{ data: Account[] }>(
          `/budgets/${submitted.budget_id}/accounts`,
        );
        const present = current.data.find(
          (account) => account.id === submitted.pending!.body.id,
        );
        clearDraft(submitted, true);
        if (!present) {
          this.status =
            "Earlier account creation acknowledged. That account is absent from the restored budget.";
          this.draft = null;
          return;
        }
        return present as Result;
      }
      clearDraft(submitted, true);
      return result.data;
    } catch (error) {
      // A confirmed 4xx rejection permits correction; unknown outcomes stay immutable.
      if (
        error instanceof RequestError &&
        !acknowledged &&
        !error.uncertain &&
        error.code !== "budget_restored" &&
        error.code !== "budget_deleted" &&
        this.draft?.pending
      ) {
        try {
          const editable = { ...this.draft, pending: null } as D;
          saveDraft(editable, this.draft.pending);
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
    if (
      !this.draft ||
      this.draft.kind !== "account" ||
      !this.blocked ||
      this.busy
    )
      return;
    this.busy = true;
    try {
      const { data: budget } = await request<{ data: Budget }>(
        `/budgets/${this.draft.budget_id}?include_deleted=true`,
      );
      const accounts = (budget as Budget & { deleted?: boolean }).deleted
        ? []
        : (
            await request<{ data: Account[] }>(
              `/budgets/${this.draft.budget_id}/accounts`,
            )
          ).data;
      this.review = { budget, accounts };
    } catch (error) {
      this.error = errorMessage(error);
    } finally {
      this.busy = false;
    }
  }
  resolveReviewed(reapply: boolean): boolean {
    const original = this.draft;
    if (
      !original ||
      original.kind !== "account" ||
      !this.blocked ||
      !this.review ||
      this.busy
    )
      return false;
    try {
      // A definitive retired-epoch rejection fences every delayed copy permanently.
      // Retain original request evidence before replacing the active draft.
      if (
        reapply &&
        (this.review.budget as Budget & { deleted?: boolean }).deleted
      )
        throw new Error(
          "Restore this deleted budget before creating another account.",
        );
      localStorage.setItem(
        `actualBudget.retiredCreation.v1.${original.draft_id}`,
        JSON.stringify(original),
      );
      clearDraft(original, true);
      if (reapply) {
        this.draft = {
          ...original,
          draft_id: uuid(),
          budget_epoch: this.review.budget.epoch,
          pending: null,
        } as D;
        saveDraft(this.draft);
        this.error = "";
        this.blocked = false;
        this.review = null;
        this.status =
          "Review the retained fields, then create this account as a new change.";
      } else this.draft = null;
      return true;
    } catch (error) {
      this.error = errorMessage(error);
      return false;
    }
  }
}
