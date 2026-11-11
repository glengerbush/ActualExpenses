<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import {
    request,
    uuid,
    RequestError,
    errorMessage,
    type Account,
  } from "./api";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import { historyOwner } from "./history-owner";
  import { readBulk, writeBulk, type BulkIntent } from "./bulk-storage";
  import { captureAmountFormat, parseAmount, today } from "./money";
  import { parseDateInput } from "./dates";
  import { preferences } from "./preferences.svelte";
  import TransactionGraph from "./TransactionGraph.svelte";
  import DateInput from "./DateInput.svelte";
  import type { Transaction, Reference } from "./register-api";
  let {
    budgetId,
    selected,
    snapshot,
    disabled = false,
    accounts,
    payees,
    categories,
    active = $bindable(false),
    onchanged,
    onclear,
  }: {
    budgetId: string;
    selected: Transaction[];
    snapshot: string;
    disabled?: boolean;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    active?: boolean;
    onchanged: (message: string) => Promise<void>;
    onclear: () => void;
  } = $props();
  interface History {
    undo: { id: string; label: string } | null;
    redo: { id: string; label: string } | null;
    epoch: string;
    edit_clock: string;
  }
  interface MergeReview {
    keeper_id: string;
    before: Record<string, unknown>[];
    after: Record<string, unknown>[];
    deleted_ids: string[];
    protected_count: number;
    protection_token: string;
    snapshot: string;
    epoch: string;
    ids: string[];
  }
  let mergeReview = $state.raw<MergeReview | null>(null),
    confirmProtected = $state(false);
  async function previewMerge() {
    if (!ready || selected.length !== 2) return;
    busy = true;
    error = "";
    syncActive();
    try {
      const ids = selected.map((r) => r.id),
        capturedSnapshot = snapshot,
        capturedEpoch = preferences.budgetEpoch;
      const result = await request<{ data: Omit<MergeReview, "ids"> }>(
        `/budgets/${budgetId}/transactions/merge/preview`,
        { snapshot: capturedSnapshot, ids },
      );
      if (result.data.epoch !== capturedEpoch)
        throw new Error(
          "The budget was restored. Refresh and select the entries again.",
        );
      mergeReview = { ...result.data, ids };
      confirmProtected = false;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
      syncActive();
    }
  }
  let history = $state.raw<History | null>(null);
  let saved = $state.raw<BulkIntent | null>(null);
  let busy = $state(false),
    loading = $state(true),
    error = $state(""),
    notice = $state("");
  let editing = $state(false),
    deleting = $state(false),
    field = $state("cleared"),
    value = $state("true"),
    date = $state(today());
  let notesMode = $state("replace"),
    find = $state(""),
    useRegex = $state(false);
  let format = captureAmountFormat();
  let dateFormat = $state(preferences.shared.date_format);
  let ready = $derived(
    !disabled &&
      !busy &&
      !loading &&
      !saved &&
      !editing &&
      !deleting &&
      !mergeReview,
  );
  function syncActive() {
    active = busy || !!saved || editing || deleting || !!mergeReview;
  }
  export async function refresh() {
    if (disabled) return;
    try {
      const result = await request<{ data: History }>(
        `/budgets/${budgetId}/history`,
        undefined,
        undefined,
        "GET",
        undefined,
        false,
        undefined,
        await historyOwner(),
      );
      history = result.data;
      observeVersions({ history: result.data.edit_clock });
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(() => {
    void (async () => {
      try {
        saved = await readBulk(budgetId);
        if (saved)
          notice =
            "Recovered a saved action. Retry its original request or cancel safely.";
        await refresh();
      } catch (e) {
        error = errorMessage(e);
      } finally {
        loading = false;
        syncActive();
      }
    })();
  });
  function edit() {
    format = captureAmountFormat();
    dateFormat = preferences.shared.date_format;
    editing = true;
    deleting = false;
    syncActive();
  }
  function fieldChanged() {
    value = field === "cleared" ? "true" : "";
  }
  async function prepare(
    action: "update" | "duplicate" | "delete" | "merge" | "undo" | "redo",
  ) {
    if (disabled || busy || saved) return;
    const rows = selected,
      capturedSnapshot = snapshot,
      capturedEpoch = preferences.budgetEpoch,
      capturedHistory = history;
    busy = true;
    syncActive();
    error = "";
    notice = "";
    try {
      if (
        action === "merge" &&
        (!mergeReview || (mergeReview.protected_count > 0 && !confirmProtected))
      )
        throw new Error("Review and confirm the protected graph first.");
      const historyAction = action === "undo" || action === "redo";
      if (!historyAction && (!rows.length || rows.length > 100))
        throw new Error("Select 1–100 visible entries.");
      const fields: Record<string, unknown> = {};
      if (action === "update")
        fields[field] =
          field === "amount"
            ? parseAmount(value, format)
            : field === "date"
              ? parseDateInput(date, dateFormat)
              : field === "cleared"
                ? value === "true"
                : ["payee_id", "category_id"].includes(field)
                  ? value || null
                  : value;
      if (action !== "update") {
        format = captureAmountFormat();
        dateFormat = preferences.shared.date_format;
      }
      const target = historyAction ? capturedHistory?.[action] : null;
      if (historyAction && !target)
        throw new Error("No history action is available. Refresh first.");
      for (const row of rows) {
        observeVersions(row.field_versions);
        for (const child of row.children ?? [])
          observeVersions(child.field_versions);
      }
      const intent: BulkIntent = {
        budgetId,
        revision: uuid(),
        description: historyAction
          ? `${action === "undo" ? "Undo" : "Redo"}: ${target!.label}`
          : action === "merge"
            ? "Merge duplicate entries"
            : `${action === "update" ? "Update" : action === "duplicate" ? "Duplicate" : "Delete"} ${rows.length} selected ${rows.length === 1 ? "entry" : "entries"}`,
        format,
        dateFormat,
        pending: {
          path: `/budgets/${budgetId}/${historyAction ? `history/${action}` : action === "merge" ? "transactions/merge" : "transactions/bulk"}`,
          method: "POST",
          key: uuid(),
          historyOwner: await historyOwner(),
          edit_clock: nextEditClock(),
          budgetEpoch: historyAction
            ? capturedHistory!.epoch
            : action === "merge"
              ? mergeReview!.epoch
              : capturedEpoch,
          body: historyAction
            ? { action_id: target!.id }
            : action === "merge"
              ? {
                  snapshot: mergeReview!.snapshot,
                  ids: mergeReview!.ids,
                  ...(mergeReview!.protected_count > 0
                    ? { protection_token: mergeReview!.protection_token }
                    : {}),
                }
              : {
                  snapshot: capturedSnapshot,
                  action,
                  ids: rows.map((row) => row.id),
                  ...(action === "update"
                    ? {
                        fields,
                        ...(field === "notes"
                          ? {
                              notes: {
                                mode: notesMode,
                                find,
                                use_regex: useRegex,
                              },
                            }
                          : {}),
                      }
                    : {}),
                  ...(action === "duplicate"
                    ? { duplicate_ids: rows.map(() => uuid()) }
                    : {}),
                },
        },
      };
      saved = await writeBulk(budgetId, null, intent);
      editing = false;
      deleting = false;
      mergeReview = null;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
      syncActive();
    }
    if (saved) await send();
  }
  async function send(cancel = false) {
    if (!saved || busy || disabled) return;
    busy = true;
    syncActive();
    error = "";
    notice = "";
    let resolved = false;
    let message = "";
    try {
      const next = {
        ...saved,
        pending: {
          ...saved.pending,
          cancelRequested: saved.pending.cancelRequested || cancel,
        },
      };
      // Every attempt, including retries and cancellation, has a committed checkpoint.
      saved = await writeBulk(budgetId, saved, next);
      const pending = saved!.pending;
      const result = await request<{ data: { affected_count: number } }>(
        pending.path,
        pending.body,
        pending.key,
        pending.method,
        pending.edit_clock,
        pending.cancelRequested,
        pending.budgetEpoch,
        pending.historyOwner,
      );
      message = `${saved!.description} saved. ${result.data.affected_count} ${result.data.affected_count === 1 ? "entry" : "entries"} affected.`;
      resolved = true;
    } catch (e) {
      if (
        e instanceof RequestError &&
        !e.uncertain &&
        e.code === "operation_canceled"
      ) {
        message = "Action canceled. No changes were saved.";
        resolved = true;
      } else error = errorMessage(e);
    }
    if (resolved) {
      try {
        saved = await writeBulk(budgetId, saved, null);
        if (cancel) resetCorrectedClock();
        onclear();
        await onchanged(message);
        await refresh();
        notice = message;
      } catch (e) {
        error = errorMessage(e);
      }
    }
    busy = false;
    syncActive();
  }
  function shortcut(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (
      !ready ||
      !(event.ctrlKey || event.metaKey) ||
      event.altKey ||
      target.closest("input,textarea,select,[contenteditable=true]")
    )
      return;
    const action =
      event.key.toLowerCase() === "z"
        ? event.shiftKey
          ? "redo"
          : "undo"
        : event.key.toLowerCase() === "y"
          ? "redo"
          : null;
    if (action && history?.[action]) {
      event.preventDefault();
      void prepare(action);
    }
  }
</script>

<svelte:window onkeydown={shortcut} />
<div class="bulk-actions" aria-busy={busy || loading}>
  <div class="controls">
    <button
      onclick={() => prepare("undo")}
      disabled={!ready || !history?.undo}
      title={history?.undo?.label}>{t("Undo")}</button
    >
    <button
      onclick={() => prepare("redo")}
      disabled={!ready || !history?.redo}
      title={history?.redo?.label}>{t("Redo")}</button
    >
    <button onclick={refresh} disabled={disabled || busy || !!saved}
      >Refresh history</button
    >
    {#if selected.length}<strong aria-live="polite"
        >{selected.length} selected</strong
      >
      <button onclick={edit} disabled={!ready}>Edit selected</button>
      <button onclick={previewMerge} disabled={!ready || selected.length !== 2}
        >Review duplicate merge</button
      >
      <button onclick={() => prepare("duplicate")} disabled={!ready}
        >Duplicate selected</button
      >
      <button
        onclick={() => {
          deleting = true;
          syncActive();
        }}
        disabled={!ready}>Delete selected</button
      >
      <button
        onclick={onclear}
        disabled={busy || !!saved || editing || deleting || !!mergeReview}
        >Clear selection</button
      >
    {/if}
  </div>
  {#if selected.some((row) => row.parent_id)}<p class="hint">
      Split entries are selected individually. Duplicating any child copies its
      whole split once. Change date or status by selecting the parent.
    </p>{/if}
  {#if mergeReview}
    <section aria-label="Duplicate merge review">
      <h2>Merge two duplicate purchases</h2>
      <p>
        Keep entry {mergeReview.keeper_id}. Remove {mergeReview.deleted_ids
          .length} duplicate entries from the live ledger. Review the complete affected
        split and transfer graph below.
      </p>
      <details open>
        <summary>Before merge</summary><TransactionGraph
          rows={mergeReview.before}
          {accounts}
          {payees}
          {categories}
        />
      </details>
      <details open>
        <summary>After merge</summary><TransactionGraph
          rows={mergeReview.after}
          {accounts}
          {payees}
          {categories}
        />
      </details>
      {#if mergeReview.protected_count > 0}<label
          ><input type="checkbox" bind:checked={confirmProtected} />I reviewed
          the {mergeReview.protected_count} reconciled entries and confirm this exact
          protected merge.</label
        >{/if}
      <div class="controls">
        <button
          onclick={() => prepare("merge")}
          disabled={busy ||
            (mergeReview.protected_count > 0 && !confirmProtected)}
          >Merge reviewed duplicates</button
        ><button
          onclick={() => {
            mergeReview = null;
            syncActive();
          }}
          disabled={busy}>Keep both purchases</button
        >
      </div>
    </section>
  {/if}
  {#if editing}
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void prepare("update");
      }}
    >
      <fieldset disabled={busy || disabled}>
        <legend
          >Change {selected.length} selected {selected.length === 1
            ? "entry"
            : "entries"}</legend
        >
        <label for="bulk-field">Field</label><select
          id="bulk-field"
          bind:value={field}
          onchange={fieldChanged}
        >
          <option value="cleared">Cleared status</option><option value="notes"
            >{t("Notes")}</option
          ><option value="date">{t("Date")}</option><option value="amount"
            >{t("Amount")}</option
          ><option value="payee_id">{t("Payee")}</option><option
            value="category_id">{t("Category")}</option
          >
        </select>
        {#if field === "date"}<DateInput
            id="bulk-date"
            label="Date for selected entries"
            bind:value={date}
            format={dateFormat}
            required
          />
        {:else}{#if field === "notes"}<label for="bulk-notes-mode"
              >Notes action</label
            ><select id="bulk-notes-mode" bind:value={notesMode}
              ><option value="replace">Replace notes</option><option
                value="prepend">Prepend text</option
              ><option value="append">Append text</option><option
                value="find_replace">Find and replace</option
              ></select
            >{#if notesMode === "find_replace"}<label for="bulk-find"
                >Find in notes</label
              ><input id="bulk-find" bind:value={find} maxlength="2048" /><label
                class="regex-choice"
                ><input type="checkbox" bind:checked={useRegex} /> Use regular expression</label
              >{/if}{/if}<label for="bulk-value">New value</label>
          {#if field === "cleared"}<select id="bulk-value" bind:value
              ><option value="true">{t("Cleared")}</option><option value="false"
                >{t("Uncleared")}</option
              ></select
            >
          {:else if ["account_id", "payee_id", "category_id"].includes(field)}<select
              id="bulk-value"
              bind:value
              required={field === "account_id"}
            >
              <option value=""
                >{t(
                  field === "account_id"
                    ? "Select account"
                    : field === "payee_id"
                      ? "No payee"
                      : "Uncategorized",
                )}</option
              >
              {#each field === "account_id" ? accounts : field === "payee_id" ? payees : categories as item (item.id)}<option
                  value={item.id}>{item.name}</option
                >{/each}
            </select>
          {:else if field === "notes"}<textarea
              id="bulk-value"
              bind:value
              maxlength="10000"
              rows="2"></textarea>
          {:else}<input
              id="bulk-value"
              bind:value
              inputmode="decimal"
              required
              maxlength="128"
            />{/if}
        {/if}
        {#if field === "amount"}<p class="hint">
            Use a minus sign for expenses. Split totals must still equal their
            allocations.
          </p>{/if}
        <div class="controls">
          <button class="primary"
            >Apply to {selected.length}
            {selected.length === 1 ? "entry" : "entries"}</button
          ><button
            type="button"
            onclick={() => {
              editing = false;
              syncActive();
            }}>Keep entries unchanged</button
          >
        </div>
      </fieldset>
    </form>
  {/if}
  {#if deleting}<div class="confirmation">
      <p>
        Delete {selected.length} selected {selected.length === 1
          ? "entry"
          : "entries"}? Selected parents include their children. Deleting only
        children must preserve the split total. You can undo this action.
      </p>
      <div class="controls">
        <button onclick={() => prepare("delete")} disabled={disabled || busy}
          >Confirm bulk delete</button
        ><button
          onclick={() => {
            deleting = false;
            syncActive();
          }}
          disabled={busy}>Keep selected entries</button
        >
      </div>
    </div>{/if}
  {#if saved}<div class="confirmation">
      <p>{saved.description}</p>
      <p class="hint">
        The outcome is unconfirmed. Keep this saved request until the server
        confirms its result.
      </p>
      <div class="controls">
        <button onclick={() => send()} disabled={disabled || busy}
          >{saved.pending.cancelRequested
            ? "Retry cancellation"
            : "Retry saved bulk action"}</button
        ><button
          onclick={() => send(true)}
          disabled={disabled || busy || saved.pending.cancelRequested}
          >Cancel saved action safely</button
        >
      </div>
    </div>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if busy}<p role="status">Saving action…</p>{/if}
</div>

<style>
  .bulk-actions {
    border-block: 1px solid var(--line);
    padding-block: 0.75rem;
    margin-block: 1rem;
  }
  .regex-choice {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .regex-choice input {
    width: 1.25rem;
    height: 1.25rem;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }
  form,
  .confirmation {
    margin-block-start: 1rem;
    max-width: 42rem;
  }
  fieldset {
    padding: 0;
    border: 0;
    min-width: 0;
  }
  legend {
    font-weight: 600;
    margin-block-end: 0.75rem;
  }
  .controls strong {
    margin-inline: 0.5rem;
  }
  input,
  select,
  textarea {
    max-width: 100%;
  }
  @media (max-width: 700px) {
    .controls button {
      flex: 1 1 auto;
    }
    .controls strong {
      flex-basis: 100%;
      margin: 0.25rem 0;
    }
  }
</style>
