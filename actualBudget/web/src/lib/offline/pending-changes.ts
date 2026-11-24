/** Owns the durable handoff from an editor draft to a pending change. */
import { uuid } from "../api";
import {
  sameMutation,
  type PendingMutation,
  type Transaction,
} from "../register-api";
import {
  saveTransactionDraft,
  clearTransactionDraft,
  clearRegisterAction,
  type TransactionDraft,
  type SavedAction,
} from "../register-storage";
import { entries, receipts, enqueue, type OutboxEntry } from "./outbox";
import { deliverPending, offlineStatus } from "./delivery";

export async function capturePendingChange({
  draft,
  pending,
  row,
  queued,
  disconnected,
  onDraft,
  onCaptured,
}: {
  draft: TransactionDraft;
  pending: PendingMutation;
  row: Transaction;
  queued: OutboxEntry | null;
  disconnected: boolean;
  onDraft: (draft: TransactionDraft) => void;
  onCaptured?: (row: Transaction, created: boolean) => Promise<void>;
}) {
  const budgetId = draft.budgetId;
  const submittedDraft = {
    ...draft,
    pending,
    clockRejected: false,
    attemptId: uuid(),
  };
  // Replacements stay editable until the atomic IDB handoff. A concurrent claim
  // may turn this intended POST replacement into a distinct PATCH.
  if (!draft.replaces) {
    draft = submittedDraft;
    onDraft(draft);
    saveTransactionDraft(draft);
  }
  const replacement = draft.replaces;
  let replacementPending = pending;
  if (replacement) {
    const delta = pending.body as Record<string, unknown>;
    const body = {
      ...(replacement.pending.body as Record<string, unknown>),
      ...delta,
    };
    for (const [kind, field] of [
      ["new_payee", "payee_id"],
      ["new_category", "category_id"],
    ] as const) {
      if (Object.hasOwn(delta, field) && !Object.hasOwn(delta, kind))
        delete body[kind];
    }
    if (replacement.pending.method === "PATCH" && replacement.baseline) {
      for (const key of Object.keys(body))
        if (
          key in replacement.baseline &&
          JSON.stringify(body[key]) ===
            JSON.stringify(replacement.baseline[key as keyof Transaction])
        )
          delete body[key];
    }
    replacementPending = {
      ...pending,
      path: replacement.pending.path,
      method: replacement.pending.method,
      body,
    };
    submittedDraft.pending = replacementPending;
  }
  const savedEntry = await enqueue({
    budgetId,
    pending: structuredClone(replacementPending),
    row,
    draft: structuredClone(submittedDraft),
    offlineCreated:
      queued?.offlineCreated ??
      (!draft.original &&
        (!draft.beganOnline || disconnected || !offlineStatus.reachable)),
    replace: replacement
      ? {
          key: replacement.key,
          revision: replacement.revision,
          baseline: structuredClone(replacement.baseline ?? null),
          empty:
            replacementPending.method === "PATCH" &&
            Object.keys(replacementPending.body as Record<string, unknown>)
              .length === 0,
        }
      : undefined,
    afterAttempt: replacement
      ? {
          pending: structuredClone(pending),
          original: structuredClone(draft.original!),
        }
      : undefined,
    dependsOn: queued?.attempted ? queued.key : undefined,
  });
  draft = {
    ...submittedDraft,
    original: savedEntry.draft
      ? savedEntry.draft.original
      : submittedDraft.original,
    pending: savedEntry.pending,
  };
  onDraft(draft);
  await onCaptured?.(
    savedEntry.row,
    !submittedDraft.original && !submittedDraft.replaces,
  );
  await deliverPending();
  const receipt = (await receipts(budgetId)).find((r) =>
    sameMutation(r.pending, savedEntry.pending),
  );
  // Ownership transfers only after the IndexedDB transaction has committed.
  clearTransactionDraft(draft, true);
  return { row: receipt?.row ?? null, message: receipt?.message };
}

// Read storage first, then inspect the caller's current draft. An asynchronous read
// must never acknowledge a stale editor snapshot after the user has changed it.
export async function readPendingChanges(budgetId: string) {
  const [items, acknowledged] = await Promise.all([
    entries(budgetId),
    receipts(budgetId),
  ]);
  const owned = (key: string) =>
    items.find((item) => item.key === key)?.pending ??
    acknowledged.find((item) => item.key === key)?.pending;
  return {
    entries: items,
    receipts: acknowledged,
    releaseDraft(draft: TransactionDraft): boolean {
      if (!draft.pending || !sameMutation(owned(draft.draftId), draft.pending))
        return false;
      clearTransactionDraft(draft, true);
      return true;
    },
    releaseAction(action: SavedAction): boolean {
      if (!sameMutation(owned(action.key), action)) return false;
      clearRegisterAction(budgetId, action);
      return true;
    },
    recoverDraft(draft: TransactionDraft) {
      const entry = items.find((item) => item.key === draft.draftId);
      const receipt = acknowledged.find((item) => item.key === draft.draftId);
      const handedOff = entry?.draft ?? receipt?.draft;
      const intent = (value: TransactionDraft) =>
        JSON.stringify([
          value.draftId,
          value.id,
          value.fields,
          value.format,
          value.dateFormat,
          value.budgetEpoch,
          value.editClock,
        ]);
      if (
        draft.replaces &&
        handedOff?.pending &&
        intent(draft) === intent(handedOff) &&
        (!receipt || sameMutation(receipt.pending, handedOff.pending))
      ) {
        const recovered = structuredClone(handedOff);
        saveTransactionDraft(recovered);
        clearTransactionDraft(recovered, true);
        return {
          status: "recovered" as const,
          draft: recovered,
          row: receipt?.row ?? null,
          message: receipt?.message,
        };
      }
      return {
        status: entry || receipt ? ("conflict" as const) : ("absent" as const),
      };
    },
  };
}
