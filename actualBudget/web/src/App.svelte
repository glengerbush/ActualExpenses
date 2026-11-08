<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    errorMessage,
    list,
    request,
    loadSelection,
    saveSelection,
    type Budget,
    type Account,
  } from "./lib/api";
  import { formatAmount } from "./lib/money";
  import BudgetForm from "./lib/BudgetForm.svelte";
  import AccountForm from "./lib/AccountForm.svelte";
  import AccountView from "./lib/AccountView.svelte";
  let budgets = $state.raw<Budget[]>([]);
  let accounts = $state.raw<Account[]>([]);
  let budget = $state.raw<Budget | null>(null);
  let account = $state.raw<Account | null>(null);
  let loading = $state(true);
  let error = $state("");
  let form = $state<"budget" | "account" | null>(null);
  let main: HTMLElement | undefined;
  let retry = $state<() => Promise<void>>(initialize);
  async function focusContent() {
    await tick();
    main?.focus();
  }
  async function initialize() {
    loading = true;
    error = "";
    retry = initialize;
    try {
      const { data: info } = await request<{ data: { api_version: number } }>(
        "/info",
      );
      if (info.api_version !== 1)
        throw new Error(
          "This server uses an unsupported API version. Update the app and server together.",
        );
      budgets = await list<Budget>("/budgets");
      const saved = loadSelection();
      const found = budgets.find((b) => b.id === saved.budgetId);
      if (found) await openBudget(found, saved.accountId);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
  async function openBudget(value: Budget, accountId?: string) {
    budget = value;
    account = null;
    accounts = [];
    loading = true;
    error = "";
    form = null;
    retry = () => openBudget(value, accountId);
    try {
      accounts = await list<Account>(`/budgets/${value.id}/accounts`);
      account = accounts.find((a) => a.id === accountId) ?? null;
      saveSelection(value.id, account?.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
      await focusContent();
    }
  }
  async function showBudgets() {
    budget = null;
    account = null;
    accounts = [];
    form = null;
    error = "";
    retry = initialize;
    saveSelection("");
    await focusContent();
  }
  async function selectAccount(value: Account | null) {
    account = value;
    saveSelection(budget!.id, value?.id);
    await focusContent();
  }
  async function showForm(value: "budget" | "account" | null) {
    form = value;
    await focusContent();
  }
  async function budgetCreated(value: Budget) {
    budgets = [...budgets, value];
    await openBudget(value);
  }
  async function accountCreated(value: Account) {
    accounts = [...accounts, value];
    form = null;
    await selectAccount(value);
  }
  onMount(() => {
    void initialize();
  });
</script>

<a class="skip-link" href="#main">Skip to content</a>
<div class="app-shell">
  <aside class="sidebar" aria-label="Budget navigation">
    <div class="brand">actualBudget</div>
    {#if budget}
      <button class="quiet" onclick={showBudgets} disabled={!!form || loading}
        >All budgets</button
      >
      <div class="budget-name">{budget.name}</div>
      <p class="hint method">
        {budget.budget_type === "tracking"
          ? "Tracking budget"
          : "Envelope budget"}
      </p>
      <nav aria-label="Accounts">
        <button
          class={{ current: !account }}
          aria-current={!account ? "page" : undefined}
          onclick={() => selectAccount(null)}
          disabled={!!form || loading}>All accounts</button
        >
        {#each accounts as item (item.id)}<button
            class={{ current: account?.id === item.id }}
            aria-current={account?.id === item.id ? "page" : undefined}
            onclick={() => selectAccount(item)}
            disabled={!!form || loading}
            ><span>{item.name}</span><span class="nav-amount"
              >{formatAmount(item.balance)}</span
            ></button
          >{/each}
      </nav>
    {:else}<p class="muted sidebar-description">
        Your accounts, together.
      </p>{/if}
  </aside>
  <main
    id="main"
    {@attach (element) => {
      main = element;
      return () => {
        main = undefined;
      };
    }}
    tabindex="-1"
    aria-busy={loading}
  >
    {#if loading}<div class="loading" role="status">
        <h1>Opening your budget…</h1>
        <div class="skeleton"></div>
        <div class="skeleton short"></div>
      </div>
    {:else if error}<section>
        <h1>Unable to open your data</h1>
        <p class="error" role="alert">{error}</p>
        <button class="primary" onclick={retry}>Retry</button>
      </section>
    {:else if form === "budget"}<BudgetForm
        oncreated={budgetCreated}
        oncancel={() => showForm(null)}
      />
    {:else if form === "account" && budget}<AccountForm
        budgetId={budget.id}
        oncreated={accountCreated}
        oncancel={() => showForm(null)}
      />
    {:else if !budget}
      <div class="page-heading">
        <div>
          <h1>Your budgets</h1>
          <p class="muted">Choose a budget or start a new one.</p>
        </div>
        <button class="primary" onclick={() => showForm("budget")}
          >New budget</button
        >
      </div>
      {#if budgets.length}<ul class="budget-list">
          {#each budgets as item (item.id)}<li>
              <button onclick={() => openBudget(item)}
                ><span class="list-name">{item.name}</span><span class="muted"
                  >{item.budget_type === "tracking"
                    ? "Tracking"
                    : "Envelope"}</span
                ><span>Open</span></button
              >
            </li>{/each}
        </ul>
      {:else}<div class="empty-state">
          <h2>A place for your accounts</h2>
          <p>
            Create your first budget, then add an account with its opening
            balance.
          </p>
        </div>{/if}
    {:else if account}<AccountView {account} />
    {:else}
      <div class="page-heading">
        <div>
          <h1>Accounts</h1>
          <p class="muted">{budget.name}</p>
        </div>
        <button class="primary" onclick={() => showForm("account")}
          >Add account</button
        >
      </div>
      {#if accounts.length}<div class="table-wrap">
          <table>
            <caption class="visually-hidden"
              >Accounts and current balances</caption
            ><thead
              ><tr
                ><th>Account</th><th>Type</th><th class="amount">Balance</th
                ></tr
              ></thead
            ><tbody
              >{#each accounts as item (item.id)}<tr
                  ><td
                    ><button
                      class="text-button"
                      onclick={() => selectAccount(item)}>{item.name}</button
                    ></td
                  ><td class="muted"
                    >{item.off_budget ? "Off budget" : "On budget"}</td
                  ><td class="amount">{formatAmount(item.balance)}</td></tr
                >{/each}</tbody
            >
          </table>
        </div>
      {:else}<div class="empty-state">
          <h2>Add your first account</h2>
          <p>
            Start with an everyday spending account, savings, or a credit card.
            Enter its current balance to begin.
          </p>
        </div>{/if}
    {/if}
  </main>
</div>
