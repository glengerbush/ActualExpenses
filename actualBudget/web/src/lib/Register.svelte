<script lang="ts">
  import { onMount } from "svelte";
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
  import { validDate } from "./dates";
  import {
    registerPage,
    references,
    mutate,
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
  let asOf = $state("");
  let filterDate = $state("");
  let showForm = $state(false);
  let editing = $state.raw<Transaction | null>(null);
  let deleting = $state.raw<Transaction | null>(null);
  let pending = $state<SavedAction | null>(null);
  async function load() {
    loading = true;
    error = "";
    try {
      const [page, accountRows, payeeRows, categoryRows] = await Promise.all([
        registerPage(budgetId, accountId, 0, asOf),
        list<Account>(`/budgets/${budgetId}/accounts`),
        references(budgetId, "payees"),
        references(budgetId, "categories"),
      ]);
      register = page;
      accounts = accountRows;
      payees = payeeRows;
      categories = categoryRows;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
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

  async function changed() {
    showForm = false;
    editing = null;
    deleting = null;
    await load();
    try {
      await onchanged();
    } catch (e) {
      error =
        t("Saved. Refresh the account list to see its updated balance.") +
        " " +
        errorMessage(e);
    }
  }
  async function more() {
    if (!register || register.next_offset === null) return;
    loading = true;
    error = "";
    try {
      const page = await registerPage(
        budgetId,
        accountId,
        register.next_offset,
        asOf,
      );
      register = { ...page, data: [...register.data, ...page.data] };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
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
    try {
      if (!pending) {
        if (!row || !action) return;
        pending = {
          path: `/budgets/${budgetId}/transactions/${row.id}${action === "duplicate" ? "/duplicate" : ""}`,
          method: action === "delete" ? "DELETE" : "POST",
          key: uuid(),
          body: action === "duplicate" ? { id: uuid() } : undefined,
          description: `${action === "delete" ? t("Delete") : t("Duplicate")} ${description(row)}`,
        };
      }
      saveRegisterAction(budgetId, pending);
      await mutate(pending);
      acknowledged = true;
      clearRegisterAction(budgetId, pending);
      pending = null;
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof RequestError && !e.uncertain && pending) {
        try {
          clearRegisterAction(budgetId, pending);
          pending = null;
        } catch {
          error = t(
            "Cannot update the saved action on this device. Free storage and retry.",
          );
        }
      }
    } finally {
      busy = false;
    }
    if (acknowledged && !pending) await changed();
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
  {#if register}
    <dl class="register-balances">
      <div>
        <dt>
          {asOf ? t("Balance through selected date") : t("All-date balance")}
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
        asOf = filterDate;
        void load();
      }}
    >
      <DateInput
        id="register-as-of"
        label={t("Show through date")}
        bind:value={filterDate}
        disabled={loading || busy}
      />
      <button
        disabled={loading || busy || (!!filterDate && !validDate(filterDate))}
        >{t("Apply")}</button
      >
      {#if asOf}<button
          type="button"
          onclick={() => {
            asOf = "";
            filterDate = "";
            void load();
          }}
          disabled={loading || busy}>{t("All dates")}</button
        >
      {:else}<span class="hint">{t("Includes future-dated entries.")}</span
        >{/if}
    </form>
  {/if}
  {#if recoveryError}<p class="error" role="alert">{recoveryError}</p>
    <button onclick={recover}>{t("Retry saved work recovery")}</button>{/if}
  {#if !preferences.hasUsableShared}<p class="error" role="alert">
      {t("Load this budget’s display preferences before entering an amount.")}
    </p>
    <button onclick={() => void preferences.loadBudget(budgetId)}
      >{t("Retry display preferences")}</button
    >{/if}
  {#if error}<p class="error" role="alert">{error}</p>
    <button onclick={load} disabled={loading || busy}
      >{t("Retry loading transactions")}</button
    >{/if}
  {#if pending}<div class="pending-action">
      <p role="status">
        {t(
          "The outcome is unconfirmed. Retry this saved request before making another change.",
        )}
      </p>
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
      oncomplete={() => void changed()}
      oncancel={() => {
        showForm = false;
        editing = null;
      }}
    />
  {/if}
  {#if loading}<p role="status">{t("Loading transactions…")}</p>{/if}
  {#if register?.data.length}
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
  {:else if !loading && !error}<p class="empty">
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
