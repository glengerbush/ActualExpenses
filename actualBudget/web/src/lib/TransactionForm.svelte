<script lang="ts">
  import DateInput from "./DateInput.svelte";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { parseDateInput, validDate } from "./dates";
  import { preferences } from "./preferences.svelte";
  import { onMount } from "svelte";
  import { uuid, RequestError, errorMessage, type Account } from "./api";
  import { t } from "./i18n.svelte";
  import { today, formatAmountInput, captureAmountFormat } from "./money";
  import { calculateAmount } from "./calculator";
  import {
    mutate,
    type Transaction,
    type Reference,
    type PendingMutation,
  } from "./register-api";
  import {
    readTransactionDraft,
    saveTransactionDraft,
    clearTransactionDraft,
    type TransactionDraft as Draft,
    type TransactionFields as Fields,
  } from "./register-storage";
  let {
    budgetId,
    accounts,
    payees,
    categories,
    initial = null,
    defaultAccountId = null,
    oncomplete,
    oncancel,
  }: {
    budgetId: string;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    initial?: Transaction | null;
    defaultAccountId?: string | null;
    oncomplete: (transaction: Transaction | null, notice?: string) => void;
    oncancel: () => void;
  } = $props();
  let draft = $state<Draft | null>(null);
  let error = $state("");
  let busy = $state(false);
  let restored = $state(false);
  let localClockProblem = $state(false);
  let selectedAccount = $derived(
    accounts.find((a) => a.id === draft?.fields.account_id),
  );
  function persist(value: Draft) {
    saveTransactionDraft(value);
  }
  function load() {
    error = "";
    try {
      const saved = readTransactionDraft(budgetId);
      if (saved) {
        draft = saved;
        restored = true;
      } else {
        if (!preferences.hasUsableShared)
          throw new Error(
            t(
              "Load this budget’s display preferences before entering an amount.",
            ),
          );
        const format = captureAmountFormat();
        draft = {
          version: 1,
          origin: location.origin,
          budgetId,
          draftId: uuid(),
          id: initial?.id ?? uuid(),
          referenceIds: { payee: uuid(), category: uuid() },
          original: initial,
          format,
          dateFormat: preferences.shared.date_format,
          amountDirty: !initial,
          pending: null,
          fields: {
            account_id:
              initial?.account_id ?? defaultAccountId ?? accounts[0]?.id ?? "",
            date: initial?.date ?? today(),
            amount: initial
              ? formatAmountInput(Math.abs(initial.amount), format)
              : "",
            direction: initial && initial.amount >= 0 ? "income" : "expense",
            payee: initial?.payee_id ?? "",
            category: initial?.category_id ?? "",
            notes: initial?.notes ?? "",
            cleared: initial?.cleared ?? false,
            newPayee: "",
            newCategory: "",
            categoryIncome: false,
          },
        };
        persist(draft);
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(load);
  function update<K extends keyof Fields>(field: K, value: Fields[K]) {
    if (!draft || draft.pending) return;
    draft.fields[field] = value;
    draft.editClock = undefined;
    if (field === "amount" || field === "direction") draft.amountDirty = true;
    if (
      field === "account_id" &&
      accounts.find((a) => a.id === value)?.off_budget
    )
      draft.fields.category = "";
    try {
      persist(draft);
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function calculate() {
    if (!draft) return;
    busy = true;
    error = "";
    try {
      const amount = await calculateAmount(draft.fields.amount, draft.format);
      draft.fields.amount = formatAmountInput(Math.abs(amount), draft.format);
      draft.amountDirty = true;
      persist(draft);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function prepare(): Promise<PendingMutation | null> {
    if (!draft) return null;
    if (draft.pending) return draft.pending;
    const saved = readTransactionDraft(budgetId);
    if (saved?.draftId === draft.draftId && saved.pending) {
      throw new Error(
        t(
          "Another unconfirmed transaction is saved. Reload and retry it first.",
        ),
      );
    }
    if (
      saved?.draftId === draft.draftId &&
      saved.editClock &&
      !draft.editClock
    ) {
      if (JSON.stringify(saved.fields) !== JSON.stringify(draft.fields))
        throw new Error(
          t("Another copy of this draft changed. Reload before submitting."),
        );
      draft.editClock = saved.editClock;
    }
    if (!draft.editClock) {
      draft.editClock = nextEditClock(draft.original?.field_versions);
      persist(draft);
    }
    const f = draft.fields;
    const date = parseDateInput(f.date, draft.dateFormat);
    if (!validDate(date)) throw new Error(t("Enter a valid date."));
    const body: Record<string, unknown> = {
      account_id: f.account_id,
      date,
      cleared: f.cleared,
      notes: f.notes,
      payee_id: f.payee === "new" ? null : f.payee || null,
      category_id:
        selectedAccount?.off_budget || f.category === "new"
          ? null
          : f.category || null,
    };
    if (!draft.original || draft.amountDirty) {
      const amount = await calculateAmount(f.amount, draft.format);
      body.amount = (f.direction === "expense" ? -1 : 1) * Math.abs(amount);
    }
    if (f.payee === "new") {
      if (!f.newPayee.trim()) throw new Error(t("Enter a payee name."));
      body.new_payee = {
        id: draft.referenceIds.payee,
        name: f.newPayee.trim(),
      };
    }
    if (f.category === "new" && !selectedAccount?.off_budget) {
      if (!f.newCategory.trim()) throw new Error(t("Enter a category name."));
      body.new_category = {
        id: draft.referenceIds.category,
        name: f.newCategory.trim(),
        is_income: f.categoryIncome,
      };
    }
    if (draft.original) {
      for (const key of Object.keys(body))
        if (
          key in draft.original &&
          body[key] === draft.original[key as keyof Transaction]
        )
          delete body[key];
      if (Object.keys(body).length === 0) return null;
    } else body.id = draft.id;
    return {
      path: `/budgets/${budgetId}/transactions${draft.original ? `/${draft.id}` : ""}`,
      method: draft.original ? "PATCH" : "POST",
      key: draft.draftId,
      edit_clock: draft.editClock,
      body,
    };
  }
  function fieldLabel(field: string) {
    const labels: Record<string, string> = {
      account_id: t("Account"),
      amount: t("Amount"),
      date: t("Date"),
      cleared: t("Cleared"),
      notes: t("Notes"),
      payee_id: t("Payee"),
      category_id: t("Category"),
    };
    return labels[field] ?? t("Transaction details");
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!draft) return;
    busy = true;
    error = "";
    let completed: Transaction | null = null;
    let notice = "";
    let deletedRemotely = false;
    try {
      const pending = await prepare();
      if (!pending) {
        clearTransactionDraft(draft);
        oncancel();
        return;
      }
      const submitted = { ...draft, pending };
      draft = submitted;
      persist(submitted);
      const response = await mutate<Transaction>(pending);
      completed = response.data;
      observeVersions(completed.field_versions);
      if (completed.deleted)
        notice = t(
          "This transaction remains deleted. Its newer field edits were retained without restoring it.",
        );
      if (!completed.deleted && response.conflicts?.length)
        notice =
          t("A newer edit replaced these fields:") +
          " " +
          response.conflicts
            .map((conflict) => fieldLabel(conflict.field))
            .join(", ");
      clearTransactionDraft(draft, true);
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof LocalClockError) localClockProblem = true;
      if (draft && e instanceof RequestError && !e.uncertain && draft.pending) {
        try {
          if (e.code === "clock_skew" || e.code === "clock_overflow") {
            draft.clockRejected = true;
            persist(draft);
          } else if (e.code === "transaction_deleted") {
            clearTransactionDraft(draft, true);
            deletedRemotely = true;
            notice = error;
          } else {
            const editable = { ...draft, pending: null };
            saveTransactionDraft(editable, draft.pending);
            draft = editable;
          }
        } catch (storageError) {
          error = errorMessage(storageError);
        }
      }
      completed = null;
    } finally {
      busy = false;
    }
    if (completed || deletedRemotely) oncomplete(completed, notice);
  }
  function recoverClock(discard = false) {
    if (!draft || busy || (draft.pending && !draft.clockRejected)) return;
    try {
      resetCorrectedClock();
      if (discard && draft.clockRejected) {
        clearTransactionDraft(draft, true);
        oncancel();
      } else if (!draft.pending) {
        draft.editClock = undefined;
        persist(draft);
        localClockProblem = false;
        error = "";
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function cancel() {
    if (!draft || draft.pending || busy) return;
    try {
      clearTransactionDraft(draft);
      oncancel();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section class="transaction-form" aria-labelledby="transaction-form-title">
  <h2 id="transaction-form-title">
    {draft?.original ? t("Edit transaction") : t("Add transaction")}
  </h2>
  {#if restored}<p class="hint" role="status">
      {t("Recovered your saved draft.")}
    </p>{/if}
  {#if draft}
    <form onsubmit={submit} aria-busy={busy}>
      <fieldset disabled={busy || !!draft.pending}>
        <div class="entry-pair">
          <div>
            <label for="transaction-account">{t("Account")}</label><select
              id="transaction-account"
              bind:value={
                () => draft!.fields.account_id, (v) => update("account_id", v)
              }
              required
              >{#each accounts as account (account.id)}<option
                  value={account.id}>{account.name}</option
                >{/each}</select
            >
          </div>
          <div>
            <DateInput
              id="transaction-date"
              label={t("Date")}
              format={draft.dateFormat}
              required
              disabled={busy || !!draft.pending}
              bind:value={() => draft!.fields.date, (v) => update("date", v)}
            />
          </div>
        </div>
        <div class="entry-pair">
          <div>
            <label for="transaction-direction">{t("Transaction type")}</label
            ><select
              id="transaction-direction"
              bind:value={
                () => draft!.fields.direction, (v) => update("direction", v)
              }
              ><option value="expense">{t("Expense")}</option><option
                value="income">{t("Income or refund")}</option
              ></select
            >
          </div>
          <div>
            <label for="transaction-amount">{t("Amount")}</label>
            <div class="amount-entry">
              <input
                id="transaction-amount"
                inputmode="decimal"
                required
                maxlength="256"
                aria-describedby="calculator-hint"
                bind:value={
                  () => draft!.fields.amount, (v) => update("amount", v)
                }
              /><button type="button" onclick={calculate}
                >{t("Calculate")}</button
              >
            </div>
            <p id="calculator-hint" class="hint">
              {t(
                "Use +, −, *, /, ^ and parentheses. Expenses subtract; income adds.",
              )}
            </p>
          </div>
        </div>
        <div class="entry-pair">
          <div>
            <label for="transaction-payee">{t("Payee")}</label><select
              id="transaction-payee"
              bind:value={() => draft!.fields.payee, (v) => update("payee", v)}
              ><option value="">{t("No payee")}</option
              >{#each payees as payee (payee.id)}<option value={payee.id}
                  >{payee.name}</option
                >{/each}<option value="new">{t("Create payee…")}</option
              ></select
            >
            {#if draft.fields.payee === "new"}<label for="new-payee-name"
                >{t("New payee name")}</label
              ><input
                id="new-payee-name"
                required
                maxlength="120"
                bind:value={
                  () => draft!.fields.newPayee, (v) => update("newPayee", v)
                }
              />{/if}
          </div>
          <div>
            <label for="transaction-category">{t("Category")}</label><select
              id="transaction-category"
              disabled={selectedAccount?.off_budget}
              bind:value={
                () => draft!.fields.category, (v) => update("category", v)
              }
              ><option value="">{t("Uncategorized")}</option
              >{#each categories as category (category.id)}<option
                  value={category.id}>{category.name}</option
                >{/each}<option value="new">{t("Create category…")}</option
              ></select
            >
            {#if draft.fields.category === "new" && !selectedAccount?.off_budget}<label
                for="new-category-name">{t("New category name")}</label
              ><input
                id="new-category-name"
                required
                maxlength="120"
                bind:value={
                  () => draft!.fields.newCategory,
                  (v) => update("newCategory", v)
                }
              /><label class="check"
                ><input
                  type="checkbox"
                  bind:checked={
                    () => draft!.fields.categoryIncome,
                    (v) => update("categoryIncome", v)
                  }
                />{t("Income category")}</label
              >{/if}
            {#if selectedAccount?.off_budget}<p class="hint">
                {t("Off-budget entries have no category.")}
              </p>{/if}
          </div>
        </div>
        <label for="transaction-notes">{t("Notes")}</label><textarea
          id="transaction-notes"
          rows="2"
          maxlength="10000"
          bind:value={() => draft!.fields.notes, (v) => update("notes", v)}
        ></textarea>
        <label class="check"
          ><input
            type="checkbox"
            bind:checked={
              () => draft!.fields.cleared, (v) => update("cleared", v)
            }
          />{t("Cleared")}</label
        >
      </fieldset>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if draft.pending && !draft.clockRejected && !busy}<p role="status">
          {t(
            "The outcome is unconfirmed. Retry this saved request before changing its details.",
          )}
        </p>{/if}
      {#if draft.clockRejected}<p role="status">
          {t(
            "The server rejected this request before making changes. Correct the device time. Retry keeps its original edit time; discard this rejected edit to start again.",
          )}
        </p>
        <button type="button" onclick={() => recoverClock(true)} disabled={busy}
          >{t("Discard rejected edit")}</button
        >
      {:else if localClockProblem && !draft.pending}<button
          type="button"
          onclick={() => recoverClock()}
          disabled={busy}>{t("Use corrected device time")}</button
        >{/if}
      <div class="actions">
        <button class="primary" disabled={busy}
          >{busy
            ? t("Saving…")
            : draft.pending
              ? t("Retry saved transaction")
              : t("Save transaction")}</button
        ><button
          type="button"
          onclick={cancel}
          disabled={busy || !!draft.pending}>{t("Cancel")}</button
        >
      </div>
    </form>
  {:else}<p class="error" role="alert">{error}</p>
    <button onclick={load}>{t("Retry draft recovery")}</button>{/if}
</section>

<style>
  .transaction-form {
    margin-block: 1.5rem;
    padding-block: 1rem;
    border-block: 1px solid var(--line);
  }
  h2 {
    margin-top: 0;
  }
  .entry-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }
  .entry-pair > div {
    min-width: 0;
  }
  .amount-entry {
    display: flex;
    gap: 0.5rem;
  }
  .amount-entry input {
    min-width: 0;
    flex: 1 1 12rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .check input {
    width: auto;
  }
  @media (max-width: 700px) {
    .entry-pair {
      grid-template-columns: 1fr;
      gap: 0;
    }
    .amount-entry {
      flex-wrap: wrap;
    }
  }
</style>
