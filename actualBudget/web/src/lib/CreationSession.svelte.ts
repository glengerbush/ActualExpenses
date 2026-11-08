import { errorMessage, request, RequestError } from "./api";
import {
  clearDraft,
  prepareSubmission,
  readDraft,
  saveDraft,
  type CreationDraft,
} from "./drafts";

/** One recoverable creation form. Persistence precedes every network mutation. */
export class CreationSession<D extends CreationDraft, Result> {
  draft = $state.raw<D | null>(null);
  busy = $state(false);
  error = $state("");
  restored = $state(false);
  constructor(private readonly create: () => D) {}
  load() {
    this.error = "";
    try {
      const fresh = this.create();
      const saved = readDraft(fresh);
      this.draft = (saved ?? fresh) as D;
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
    try {
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
      );
      clearDraft(submitted, true);
      return result.data;
    } catch (error) {
      // A confirmed 4xx rejection permits correction; unknown outcomes stay immutable.
      if (
        error instanceof RequestError &&
        !error.uncertain &&
        this.draft.pending
      ) {
        try {
          const editable = { ...this.draft, pending: null } as D;
          saveDraft(editable, this.draft.pending);
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
