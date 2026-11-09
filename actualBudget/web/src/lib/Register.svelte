<script lang="ts">
  import { onMount } from "svelte";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { list, uuid, RequestError, errorMessage, type Account } from "./api";
  import { t } from "./i18n.svelte";
  import { formatAmount, formatDate } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    readTransactionDraft,
    readRegisterAction,
    saveRegisterAction,
    clearRegisterAction,
    type SavedAction,
  } from "./register-storage";
  import TransactionForm from "./TransactionForm.svelte";
  import DateInput from "./DateInput.svelte";
  import {
    registerPage,
    changes,
    references,
    mutate,
    requestCancellation,
    type RegisterPage,
    type Transaction,
    type Reference,
  } from "./register-api";
  let {
    budgetId,
    accountId = null,
    onchanged = async () => {},
  }: {
    budgetId: string;
    accountId?: string | null;
    onchanged?: () => Promise<void>;
  } = $props();
  let register = $state.raw<RegisterPage | null>(null);
  let accounts = $state.raw<Account[]>([]);
  let payees = $state.raw<Reference[]>([]);
  let categories = $state.raw<Reference[]>([]);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let recoveryError = $state("");
  let localClockProblem = $state(false);
  let asOf = $derived(register?.as_of ?? "");
  let retryCutoff = "";
  let loadGeneration = 0;
  let notice = $state("");
  let filterDate = $state("");
  let showForm = $state(false);
  let editing = $state.raw<Transaction | null>(null);
  let deleting = $state.raw<Transaction | null>(null);
  let pending = $state<SavedAction | null>(null);
  async function load() {
    return refresh(asOf);
  }
  async function refresh(cutoff: string) {
    const generation = ++loadGeneration;
    retryCutoff = cutoff;
    loading = true;
    error = "";
    notice = "";
    try {
      const [page, accountRows, payeeRows, categoryRows] = await Promise.all([
        registerPage(budgetId, accountId, 0, cutoff),
        list<Account>(`/budgets/${budgetId}/accounts`),
        references(budgetId, "payees"),
        references(budgetId, "categories"),
      ]);
      if (generation !== loadGeneration) return false;
      for (const row of page.data) observeVersions(row.field_versions);
      register = page;
      filterDate = page.as_of ?? "";
      accounts = accountRows;
      payees = payeeRows;
      categories = categoryRows;
      return true;
    } catch (e) {
      if (generation === loadGeneration) error = errorMessage(e);
      return false;
    } finally {
      if (generation === loadGeneration) loading = false;
    }
  }
  function recover() {
    recoveryError = "";
    try {
      showForm = !!readTransactionDraft(budgetId);
      pending = readRegisterAction(budgetId);
    } catch (e) {
      recoveryError = errorMessage(e);
    }
  }
  onMount(() => {
    recover();
    void load();
  });

  async function changed(message = "") {
    showForm = false;
    editing = null;
    deleting = null;
    await load();
    notice = message;
    try {
      await onchanged();
    } catch (e) {
      error =
        t("Saved. Refresh the account list to see its updated balance.") +
        " " +
        errorMessage(e);
    }
  }
  async function refreshChanges() {
    if (!register) {
      await load();
      return;
    }
    loading = true;
    error = "";
    notice = "";
    try {
      let cursor = register.snapshot;
      let changed = false,
        changedPreferences = false;
      for (;;) {
        const page = await changes(budgetId, cursor);
        changed ||= page.data.length > 0;
        changedPreferences ||= page.data.some(
          (change) => change.entity === "preferences",
        );
        cursor = page.next_cursor;
        if (!page.has_more) break;
      }
      if (changed) {
        if (changedPreferences) await preferences.loadBudget(budgetId);
        const loaded = await load();
        if (loaded) {
          await onchanged();
          notice = t("Transactions refreshed.");
        }
      } else notice = t("Transactions are up to date.");
    } catch (e) {
      if (e instanceof RequestError && e.code === "cursor_invalid") {
        if (await load())
          notice = t(
            "The saved checkpoint changed. Transactions were reloaded.",
          );
      } else error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
  async function more() {
    if (!register || register.next_offset === null) return;
    const previous = register;
    const generation = ++loadGeneration;
    loading = true;
    error = "";
    notice = "";
    try {
      const page = await registerPage(
        budgetId,
        accountId,
        previous.next_offset!,
        previous.as_of ?? "",
        previous.snapshot,
      );
      if (generation === loadGeneration)
        register = { ...page, data: [...previous.data, ...page.data] };
    } catch (e) {
      if (generation !== loadGeneration) return;
      if (e instanceof RequestError && e.code === "register_changed") {
        const refreshed = await refresh(previous.as_of ?? "");
        notice = refreshed
          ? t("Data changed. The transaction list was refreshed.")
          : t("Data changed. Retry loading before continuing.");
      } else error = errorMessage(e);
    } finally {
      if (generation === loadGeneration) loading = false;
    }
  }
  function open(transaction: Transaction | null) {
    editing = transaction;
    showForm = true;
    deleting = null;
  }
  function description(row: Transaction) {
    return `${formatAmount(row.amount)} · ${row.payee_name ?? t("No payee")} · ${formatDate(row.date)}`;
  }
  async function runAction(action?: "delete" | "duplicate", row?: Transaction) {
    if (recoveryError) return;
    busy = true;
    error = "";
    let acknowledged = false;
    let terminalNotice = "";
    try {
      if (!pending) {
        if (!row || !action) return;
        pending = {
          path: `/budgets/${budgetId}/transactions/${row.id}${action === "duplicate" ? "/duplicate" : ""}`,
          method: action === "delete" ? "DELETE" : "POST",
          key: uuid(),
          edit_clock: nextEditClock(row.field_versions),
          body: action === "duplicate" ? { id: uuid() } : undefined,
          description: `${action === "delete" ? t("Delete") : t("Duplicate")} ${description(row)}`,
        };
      }
      pending = { ...pending, clockRejected: false, attemptId: uuid() };
      saveRegisterAction(budgetId, pending);
      await mutate(pending);
      acknowledged = true;
      clearRegisterAction(budgetId, pending);
      pending = null;
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof LocalClockError) localClockProblem = true;
      if (e instanceof RequestError && !e.uncertain && pending) {
        try {
          if (e.code === "clock_skew" || e.code === "clock_overflow") {
            const rejected = { ...pending, clockRejected: true };
            saveRegisterAction(budgetId, rejected);
            pending = rejected;
          } else {
            clearRegisterAction(budgetId, pending);
            pending = null;
            if (
              e.code === "deletion_superseded" ||
              e.code === "transaction_deleted" ||
              e.code === "operation_canceled"
            )
              terminalNotice = error;
          }
        } catch {
          error = t(
            "Cannot update the saved action on this device. Free storage and retry.",
          );
        }
      }
    } finally {
      busy = false;
    }
    if ((acknowledged || terminalNotice) && !pending)
      await changed(terminalNotice);
  }
  async function recoverClock() {
    if (busy || (pending && !pending.clockRejected)) return;
    if (pending) {
      busy = true;
      error = "";
      let resolved = false;
      let terminalNotice = "";
      try {
        pending = {
          ...pending,
          cancelRequested: true,
          clockRejected: false,
          attemptId: uuid(),
        };
        requestCancellation(pending);
        saveRegisterAction(budgetId, pending);
        await mutate(pending, true);
        terminalNotice = t(
          "This change was already saved. Its saved result has been restored.",
        );
        resolved = true;
      } catch (e) {
        if (
          e instanceof RequestError &&
          !e.uncertain &&
          e.code === "operation_canceled"
        ) {
          resolved = true;
          terminalNotice = errorMessage(e);
        } else error = errorMessage(e);
      }
      if (resolved) {
        try {
          clearRegisterAction(budgetId, pending);
          pending = null;
          resetCorrectedClock();
          localClockProblem = false;
        } catch (e) {
          error = errorMessage(e);
          resolved = false;
        }
      }
      busy = false;
      if (resolved) await changed(terminalNotice);
      return;
    }
    try {
      resetCorrectedClock();
      localClockProblem = false;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section
  class="register"
  aria-labelledby="register-title"
  aria-busy={loading || busy}
>
  <div class="register-heading">
    <h2 id="register-title">{t("Transactions")}</h2>
    <button
      onclick={refreshChanges}
      disabled={loading || busy || showForm || !!pending || !!recoveryError}
      >{t("Refresh transactions")}</button
    >
    <button
      class="primary"
      onclick={() => open(null)}
      disabled={loading ||
        busy ||
        !!pending ||
        showForm ||
        accounts.length === 0 ||
        !!recoveryError ||
        !preferences.hasUsableShared}>{t("Add transaction")}</button
    >
  </div>
  {#if register && preferences.hasUsableShared}
    <dl class="register-balances">
      <div>
        <dt>
          {asOf
            ? t("Balance through {{date}}", { date: formatDate(asOf) })
            : t("All-date balance")}
        </dt>
        <dd data-testid="register-total">
          {#if accountId}<span data-testid="account-balance"
              >{formatAmount(register.balances.total)}</span
            >{:else}{formatAmount(register.balances.total)}{/if}
        </dd>
      </div>
      <div>
        <dt>{t("Cleared")}</dt>
        <dd data-testid="register-cleared">
          {formatAmount(register.balances.cleared)}
        </dd>
      </div>
      <div>
        <dt>{t("Uncleared")}</dt>
        <dd data-testid="register-uncleared">
          {formatAmount(register.balances.uncleared)}
        </dd>
      </div>
    </dl>
    <form
      class="date-filter"
      onsubmit={(event) => {
        event.preventDefault();
        void refresh(filterDate);
      }}
    >
      <DateInput
        id="register-as-of"
        label={t("Show through date")}
        bind:value={filterDate}
        disabled={loading || busy}
      />
      <button disabled={loading || busy}>{t("Apply")}</button>
      {#if asOf}<button
          type="button"
          onclick={() => {
            void refresh("");
          }}
          disabled={loading || busy}>{t("All dates")}</button
        >
      {:else}<span class="hint">{t("Includes future-dated entries.")}</span
        >{/if}
    </form>
  {/if}
  {#if pending?.clockRejected}<p role="status">
      {t(
        "This attempt was rejected. Correct the device time before retrying. Discard asks the server to cancel it; an already saved change will be restored.",
      )}
    </p>
    <button onclick={recoverClock} disabled={busy}
      >{t("Discard rejected action")}</button
    >
  {:else if localClockProblem && !pending}<button
      onclick={recoverClock}
      disabled={busy}>{t("Use corrected device time")}</button
    >{/if}
  {#if recoveryError}<p class="error" role="alert">{recoveryError}</p>
    <button onclick={recover}>{t("Retry saved work recovery")}</button>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>
    <button onclick={() => refresh(retryCutoff)} disabled={loading || busy}
      >{t("Retry loading transactions")}</button
    >{/if}
  {#if pending}<div class="pending-action">
      {#if !pending.clockRejected}<p role="status">
          {t(
            pending.cancelRequested
              ? "Cancellation is not confirmed. Retry to learn whether the change was canceled or already saved."
              : "The outcome is unconfirmed. Retry this saved request before making another change.",
          )}
        </p>{/if}
      <p>{pending.description}</p>
      <button onclick={() => void runAction()} disabled={busy}
        >{t("Retry saved action")}</button
      >
    </div>{/if}
  {#if showForm && accounts.length && !recoveryError}
    <TransactionForm
      {budgetId}
      {accounts}
      {payees}
      {categories}
      initial={editing}
      defaultAccountId={accountId}
      oncomplete={(_transaction, message) => void changed(message)}
      oncancel={() => {
        showForm = false;
        editing = null;
      }}
    />
  {/if}
  {#if loading}<p role="status">{t("Loading transactions…")}</p>{/if}
  {#if register?.data.length && preferences.hasUsableShared}
    <table class="register-table">
      <caption class="visually-hidden"
        >{t("Account transactions, newest first")}</caption
      ><thead
        ><tr
          ><th>{t("Date")}</th>{#if !accountId}<th>{t("Account")}</th>{/if}<th
            >{t("Payee and notes")}</th
          ><th>{t("Category")}</th><th class="number">{t("Amount")}</th><th
            >{t("Status")}</th
          >{#if accountId}<th class="number">{t("Running balance")}</th>{/if}<th
            >{t("Actions")}</th
          ></tr
        ></thead
      ><tbody>
        {#each register.data as row (row.id)}
          <tr>
            <td data-label={t("Date")}>{formatDate(row.date)}</td
            >{#if !accountId}<td data-label={t("Account")}
                >{row.account_name}</td
              >{/if}
            <td data-label={t("Payee and notes")}
              ><span>{row.payee_name ?? t("No payee")}</span
              >{#if row.notes}<span class="entry-notes">{row.notes}</span
                >{/if}</td
            >
            <td data-label={t("Category")}
              >{row.category_name ?? t("Uncategorized")}</td
            ><td class="number amount" data-label={t("Amount")}
              >{formatAmount(row.amount)}</td
            >
            <td data-label={t("Status")}
              >{row.reconciled
                ? t("Reconciled")
                : row.cleared
                  ? t("Cleared")
                  : t("Uncleared")}</td
            >
            {#if accountId}<td class="number" data-label={t("Running balance")}
                >{formatAmount(row.running_balance ?? 0)}</td
              >{/if}
            <td class="row-actions"
              ><button
                onclick={() => open(row)}
                disabled={loading ||
                  busy ||
                  showForm ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared ||
                  row.reconciled}
                aria-label={`${t("Edit")} ${description(row)}`}
                >{t("Edit")}</button
              ><button
                onclick={() => void runAction("duplicate", row)}
                disabled={loading ||
                  busy ||
                  showForm ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared}
                aria-label={`${t("Duplicate")} ${description(row)}`}
                >{t("Duplicate")}</button
              ><button
                onclick={() => (deleting = row)}
                disabled={loading ||
                  busy ||
                  showForm ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared ||
                  row.reconciled}
                aria-label={`${t("Delete")} ${description(row)}`}
                >{t("Delete")}</button
              ></td
            >
          </tr>
          {#if deleting?.id === row.id}<tr class="delete-confirmation"
              ><td colspan={8}
                ><p>{t("Delete this transaction?")} {description(row)}</p>
                <button
                  onclick={() => void runAction("delete", row)}
                  disabled={busy}>{t("Confirm delete")}</button
                ><button onclick={() => (deleting = null)} disabled={busy}
                  >{t("Keep transaction")}</button
                ></td
              ></tr
            >{/if}
        {/each}
      </tbody>
    </table>
    {#if register.next_offset !== null}<button
        onclick={more}
        disabled={loading || busy}>{t("Load more transactions")}</button
      >{/if}
  {:else if preferences.hasUsableShared && !loading && !error}<p class="empty">
      {accounts.length
        ? t("No transactions yet. Add an income or expense to begin.")
        : t("Create an account before recording transactions.")}
    </p>{/if}
</section>

<style>
  .register {
    margin-top: 2rem;
    min-width: 0;
  }
  .register-heading {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }
  .register-heading h2 {
    margin: 0;
  }
  .register-balances {
    display: flex;
    flex-wrap: wrap;
    gap: 1.5rem;
    margin-block: 1.5rem;
  }
  .register-balances dt {
    color: var(--muted);
    font-size: 0.875rem;
  }
  .register-balances dd {
    margin: 0.25rem 0 0;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .date-filter {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-bottom: 1rem;
  }
  .register-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  .register-table th,
  .register-table td {
    padding: 0.65rem 0.5rem;
    text-align: left;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }
  .register-table th {
    font-weight: 600;
    color: var(--muted);
  }
  .register-table .number {
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .entry-notes {
    display: block;
    max-width: 28ch;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--muted);
    margin-top: 0.25rem;
  }
  .row-actions {
    min-width: 8rem;
  }
  .row-actions button {
    margin: 0 0.25rem 0.25rem 0;
    font-size: 0.85rem;
  }
  .pending-action,
  .delete-confirmation {
    background: var(--selected);
    color: var(--selected-text);
  }
  .pending-action {
    padding: 1rem;
  }
  .delete-confirmation button {
    margin-right: 0.5rem;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  @media (max-width: 1000px) {
    .register-table {
      font-size: 0.85rem;
    }
    .register-table th,
    .register-table td {
      padding: 0.5rem 0.25rem;
    }
    .row-actions {
      min-width: 5rem;
    }
  }
  @media (max-width: 700px) {
    .register-heading {
      align-items: flex-start;
    }
    .register-table thead {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
    }
    .register-table tr {
      display: grid;
      grid-template-columns: 1fr 1fr;
      border-bottom: 1px solid var(--line);
      padding: 0.75rem 0;
    }
    .register-table td {
      display: block;
      border: 0;
      min-width: 0;
      overflow-wrap: anywhere;
    }
    .register-table td::before {
      content: attr(data-label);
      display: block;
      font-size: 0.75rem;
      color: var(--muted);
      font-weight: 400;
      margin-bottom: 0.2rem;
    }
    .register-table .number {
      text-align: left;
      white-space: normal;
    }
    .register-table .amount {
      font-weight: 600;
    }
    .row-actions,
    .delete-confirmation td {
      grid-column: 1/-1;
    }
    .row-actions button {
      min-height: 44px;
    }
    .entry-notes {
      max-width: none;
    }
  }
</style>
