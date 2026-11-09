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
  import { readDraft } from "./lib/drafts";
  import BudgetForm from "./lib/BudgetForm.svelte";
  import AccountForm from "./lib/AccountForm.svelte";
  import AccountView from "./lib/AccountView.svelte";
  import Register from "./lib/Register.svelte";
  import Settings from "./lib/Settings.svelte";
  import ThemeRecovery from "./lib/themes/ThemeRecovery.svelte";
  import ImportBudget from "./lib/ImportBudget.svelte";
  import { preferences } from "./lib/preferences.svelte";
  import { t } from "./lib/i18n.svelte";
  let budgets = $state.raw<Budget[]>([]);
  let accounts = $state.raw<Account[]>([]);
  let budget = $state.raw<Budget | null>(null);
  let account = $state.raw<Account | null>(null);
  let loading = $state(true);
  let error = $state("");
  let recoveryError = $state("");
  let form = $state<"budget" | "account" | "settings" | "import" | null>(null);
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
          t(
            "This server uses an unsupported API version. Update the app and server together.",
          ),
        );
      budgets = await list<Budget>("/budgets");
      recoveryError = "";
      try {
        if (readDraft({ kind: "budget" })) {
          form = "budget";
          return;
        }
      } catch (e) {
        recoveryError = errorMessage(e);
      }
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
    recoveryError = "";
    budget = value;
    account = null;
    accounts = [];
    loading = true;
    error = "";
    form = null;
    retry = () => openBudget(value, accountId);
    try {
      const [accountRows] = await Promise.all([
        list<Account>(`/budgets/${value.id}/accounts`),
        preferences.loadBudget(value.id),
      ]);
      accounts = accountRows;
      account = accounts.find((a) => a.id === accountId) ?? null;
      saveSelection(value.id, account?.id);
      try {
        if (readDraft({ kind: "account", budget_id: value.id }))
          form = "account";
      } catch (e) {
        recoveryError = errorMessage(e);
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
      await focusContent();
    }
  }
  async function showBudgets() {
    recoveryError = "";
    budget = null;
    preferences.clearBudget();
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
  async function showForm(
    value: "budget" | "account" | "settings" | "import" | null,
  ) {
    recoveryError = "";
    form = value;
    await focusContent();
  }
  async function budgetCreated(value: Budget) {
    budgets = [...budgets.filter((b) => b.id !== value.id), value];
    await openBudget(value);
  }
  async function accountCreated(value: Account) {
    accounts = [...accounts.filter((a) => a.id !== value.id), value];
    form = null;
    await selectAccount(value);
  }
  async function refreshAccounts() {
    const budgetId = budget?.id;
    if (!budgetId) return;
    const updated = await list<Account>(`/budgets/${budgetId}/accounts`);
    if (budget?.id !== budgetId) return;
    accounts = updated;
    account = updated.find((item) => item.id === account?.id) ?? null;
  }
  onMount(() => {
    const stopDevicePreferences = preferences.startDevice();
    void initialize();
    return stopDevicePreferences;
  });
</script>

<a class="skip-link" href="#main">{t("Skip to content")}</a>
<ThemeRecovery />
<div class="app-shell">
  <aside class="sidebar" aria-label={t("Budget navigation")}>
    <div class="brand">actualBudget</div>
    {#if budget}
      <button class="quiet" onclick={showBudgets} disabled={!!form || loading}
        >{t("All budgets")}</button
      >
      <div class="budget-name">{budget.name}</div>
      <p class="hint method">
        {budget.budget_type === "tracking"
          ? t("Tracking budget")
          : t("Envelope budget")}
      </p>
      <nav aria-label={t("Accounts")}>
        <button
          class={{ current: !account && !form }}
          aria-current={!account && !form ? "page" : undefined}
          onclick={() => selectAccount(null)}
          disabled={!!form || loading}>{t("All accounts")}</button
        >
        {#each accounts as item (item.id)}<button
            class={{ current: account?.id === item.id && !form }}
            aria-current={account?.id === item.id && !form ? "page" : undefined}
            onclick={() => selectAccount(item)}
            disabled={!!form || loading}
            ><span>{item.name}</span><span class="nav-amount"
              >{preferences.hasUsableShared
                ? formatAmount(item.balance)
                : "—"}</span
            ></button
          >{/each}
      </nav>
      <button
        class={{ current: form === "settings" }}
        aria-current={form === "settings" ? "page" : undefined}
        onclick={() => showForm("settings")}
        disabled={!!form || loading}>{t("Settings")}</button
      >
    {:else}<p class="muted sidebar-description">
        {t("Your accounts, together.")}
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
    {#if recoveryError && !form}<p class="error" role="alert">
        {recoveryError}
      </p>{/if}
    {#if preferences.deviceError && form !== "settings"}<p
        class="error"
        role="alert"
      >
        {t(preferences.deviceError)}
      </p>{/if}
    {#if preferences.error && form !== "settings"}<div role="alert">
        <p class="error">
          {t("Display preferences could not be refreshed.")}
          {preferences.error}
        </p>
        <button
          onclick={() => budget && preferences.loadBudget(budget.id)}
          disabled={preferences.loading}>{t("Retry preferences")}</button
        >
      </div>{/if}
    {#if loading}<div class="loading" role="status">
        <h1>{t("Opening your budget…")}</h1>
        <div class="skeleton"></div>
        <div class="skeleton short"></div>
      </div>
    {:else if error}<section>
        <h1>{t("Unable to open your data")}</h1>
        <p class="error" role="alert">{error}</p>
        <button class="primary" onclick={retry}>{t("Retry")}</button>
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
    {:else if form === "settings" && budget}<Settings
        budgetId={budget.id}
        onclose={() => showForm(null)}
      />
    {:else if form === "import"}<ImportBudget
        oncreated={budgetCreated}
        oncancel={() => showForm(null)}
      />
    {:else if !budget}
      <div class="page-heading">
        <div>
          <h1>{t("Your budgets")}</h1>
          <p class="muted">{t("Choose a budget or start a new one.")}</p>
        </div>
        <div class="budget-actions">
          <button onclick={() => showForm("import")}
            >{t("Import Actual budget")}</button
          >
          <button class="primary" onclick={() => showForm("budget")}
            >{t("New budget")}</button
          >
        </div>
      </div>
      {#if budgets.length}<ul class="budget-list">
          {#each budgets as item (item.id)}<li>
              <button onclick={() => openBudget(item)}
                ><span class="list-name">{item.name}</span><span class="muted"
                  >{item.budget_type === "tracking"
                    ? t("Tracking")
                    : t("Envelope")}</span
                ><span>{t("Open")}</span></button
              >
            </li>{/each}
        </ul>
      {:else}<div class="empty-state">
          <h2>{t("A place for your accounts")}</h2>
          <p>
            {t(
              "Create your first budget, then add an account with its opening balance.",
            )}
          </p>
        </div>{/if}
    {:else if !preferences.hasUsableShared}
      <section>
        <h1>{t("Display preferences are unavailable")}</h1>
        <p>
          {t(
            "Refresh preferences before viewing amounts or entering new transactions. Saved requests keep their original amounts.",
          )}
        </p>
      </section>
      {#key budget.id + ":recovery:" + (account?.id ?? "all")}
        <Register
          budgetId={budget.id}
          accountId={account?.id ?? null}
          onchanged={refreshAccounts}
        />
      {/key}
    {:else if account}<AccountView
        {account}
        budgetId={budget.id}
        onchanged={refreshAccounts}
      />
    {:else}
      <div class="page-heading">
        <div>
          <h1>{t("Accounts")}</h1>
          <p class="muted">{budget.name}</p>
        </div>
        <button class="primary" onclick={() => showForm("account")}
          >{t("Add account")}</button
        >
      </div>
      {#if accounts.length}<div class="table-wrap">
          <table>
            <caption class="visually-hidden"
              >{t("Accounts and current balances")}</caption
            ><thead
              ><tr
                ><th>{t("Account")}</th><th>{t("Type")}</th><th class="amount"
                  >{t("Balance")}</th
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
                    >{item.off_budget ? t("Off budget") : t("On budget")}</td
                  ><td class="amount">{formatAmount(item.balance)}</td></tr
                >{/each}</tbody
            >
          </table>
        </div>
      {:else}<div class="empty-state">
          <h2>{t("Add your first account")}</h2>
          <p>
            {t(
              "Start with an everyday spending account, savings, or a credit card. Enter its current balance to begin.",
            )}
          </p>
        </div>{/if}
      {#key budget.id + ":all"}<Register
          budgetId={budget.id}
          accountId={null}
          onchanged={refreshAccounts}
        />{/key}
    {/if}
  </main>
</div>

<style>
  .budget-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
  }
</style>
