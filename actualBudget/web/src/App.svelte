<script lang="ts">
  import {
    readCatalog,
    saveCatalog,
    readBudget,
    refreshBudget,
  } from "./lib/offline/cache";
  import {
    startDelivery,
    subscribeOffline,
    offlineStatus,
  } from "./lib/offline/delivery";
  import { setReachable } from "./lib/offline/events";
  import { onMount, tick } from "svelte";
  import {
    errorMessage,
    RequestError,
    list,
    request,
    loadSelection,
    saveSelection,
    type Budget,
    type Account,
    type AccountGroup,
  } from "./lib/api";
  import { formatAmount } from "./lib/money";
  import { readDraft } from "./lib/drafts";
  import BudgetForm from "./lib/BudgetForm.svelte";
  import AccountForm from "./lib/AccountForm.svelte";
  import AccountView from "./lib/AccountView.svelte";
  import AccountManagement from "./lib/AccountManagement.svelte";
  import ReferenceManagement from "./lib/ReferenceManagement.svelte";
  import { readReferenceDraft } from "./lib/reference-actions";
  import AccountNavigation from "./lib/AccountNavigation.svelte";
  import { readAccountDraft, sortedAccounts } from "./lib/account-actions";
  import BudgetPlan from "./lib/BudgetPlan.svelte";
  import { readImport } from "./lib/transaction-import-storage";
  import TransactionImport from "./lib/TransactionImport.svelte";
  import Rules from "./lib/Rules.svelte";
  import Schedules from "./lib/Schedules.svelte";
  import { schedulesKey } from "./lib/schedules";
  import {
    foregroundSchedules,
    readScheduleJob,
  } from "./lib/schedule-foreground";
  import { rulesKey } from "./lib/rules";
  import { planningKey } from "./lib/planning";
  import { automationKey } from "./lib/budget-automations";
  import Register from "./lib/Register.svelte";
  import Categories from "./lib/Categories.svelte";
  import Settings from "./lib/Settings.svelte";
  import RecoveryTools from "./lib/RecoveryTools.svelte";
  import Reports from "./lib/Reports.svelte";
  let reportEntryKey = $state(0);
  import { writeValue } from "./lib/offline/storage";
  import { readRepair } from "./lib/recovery";
  import NavigationTools from "./lib/NavigationTools.svelte";
  import {
    readView,
    rememberView,
    type NavigationCommand,
  } from "./lib/navigation";
  import ThemeRecovery from "./lib/themes/ThemeRecovery.svelte";
  import PendingRecovery from "./lib/offline/PendingRecovery.svelte";
  import ImportBudget from "./lib/ImportBudget.svelte";
  import BudgetManagement from "./lib/BudgetManagement.svelte";
  import RestoreBudget from "./lib/RestoreBudget.svelte";
  import type { ManagedBudget, LifecycleKind } from "./lib/lifecycle";
  import { preferences } from "./lib/preferences.svelte";
  import { t } from "./lib/i18n.svelte";
  let disconnected = $state(!offlineStatus.reachable);
  let offlineError = $state("");
  let compatible = $state(true);
  let cachedAt = $state<string | null>(null);
  let navigation = 0;
  let budgets = $state.raw<Budget[]>([]);
  let accounts = $state.raw<Account[]>([]);
  let accountGroups = $state.raw<AccountGroup[]>([]);
  let budget = $state.raw<Budget | null>(null);
  let account = $state.raw<Account | null>(null);
  let loading = $state(true);
  let error = $state("");
  let recoveryError = $state("");
  let scheduleError = $state("");
  let form = $state<
    | "references"
    | "organize"
    | "budget"
    | "account"
    | "planning"
    | "transaction-files"
    | "rules"
    | "schedules"
    | "reports"
    | "settings"
    | "categories"
    | "import"
    | "manage"
    | "restore"
    | "recovery"
    | "pending"
    | null
  >(null);
  let importSource = $state<"actual" | "ynab4">("actual");
  let rulesPayee = $state("");
  let managedBudget = $state.raw<ManagedBudget | null>(null);
  let deletedBudgets = $state.raw<ManagedBudget[]>([]);
  let deletedLoaded = $state(false);
  let main: HTMLElement | undefined;
  let retry = $state<() => Promise<void>>(initialize);
  // Register only delivered destinations. Availability shares the sidebar's edit guards.
  let commands = $derived.by((): NavigationCommand[] => {
    const locked = !!form || loading;
    const result: NavigationCommand[] = [
      {
        id: "recovery",
        label: "Recovery tools",
        keywords: "repair recalculate reconnect cache reset",
        disabled: locked,
        run: () => showForm("recovery"),
      },
      {
        id: "budgets",
        label: "All budgets",
        disabled: locked,
        run: showBudgets,
      },
      {
        id: "pending",
        label: "Review pending expenses",
        disabled: locked,
        run: () => showForm("pending"),
      },
    ];
    if (!budget) {
      for (const value of budgets)
        result.push({
          id: `budget:${value.id}`,
          label: `Open budget: ${value.name}`,
          disabled: locked,
          run: () => openBudget(value),
        });
      for (const [id, label] of [
        ["budget", "New budget"],
        ["import", "Import Actual budget"],
        ["restore", "Restore backup"],
      ] as const)
        result.push({
          id,
          label,
          disabled: locked || disconnected || !compatible,
          run: () => {
            if (id === "import") importSource = "actual";
            showForm(id);
          },
        });
      result.push({
        id: "ynab-import",
        label: "Import YNAB budget",
        keywords: "YNAB4 nYNAB JSON migration",
        disabled: locked || disconnected || !compatible,
        run: () => {
          importSource = "ynab4";
          showForm("import");
        },
      });
      return result;
    }
    result.push(
      {
        id: "organize",
        label: "Organize accounts",
        disabled: locked || disconnected || !compatible,
        run: () => showForm("organize"),
      },
      {
        id: "account",
        label: "Add account",
        disabled: locked || disconnected || !compatible,
        run: () => showForm("account"),
      },
      {
        id: "manage",
        label: "Manage budget",
        disabled: locked || disconnected || !compatible,
        run: () => manage(budget!),
      },
    );
    result.push({
      id: "custom-reports",
      label: "Custom reports",
      disabled: locked || !compatible,
      run: async () => {
        await writeValue(`view:reporting-tab:${budget!.id}`, "custom");
        reportEntryKey++;
        showForm("reports");
      },
    });
    const views = [
      ["planning", "Budget", false],
      ["categories", "Categories", disconnected],
      ["references", "Payees and tags", disconnected],
      ["transaction-files", "Transaction files", false],
      ["rules", "Rules", false],
      ["schedules", "Schedules", false],
      ["reports", "Dashboards and reports", false],
      ["settings", "Settings", false],
    ] as const;
    result.push({
      id: "accounts",
      label: "All accounts",
      disabled: locked,
      run: () => selectAccount(null),
    });
    for (const [id, label, requiresConnection] of views)
      result.push({
        id,
        label,
        disabled:
          locked || requiresConnection || (id !== "settings" && !compatible),
        run: () => {
          rulesPayee = "";
          return showForm(id);
        },
      });
    for (const value of accounts)
      result.push({
        id: `account:${value.id}`,
        label: `Account: ${value.name}${value.closed ? " (closed)" : ""}`,
        keywords: "register transactions",
        disabled: locked,
        run: () => selectAccount(value),
      });
    return result;
  });
  async function focusContent() {
    await tick();
    main?.focus();
  }
  async function initialize() {
    const generation = ++navigation;
    loading = true;
    error = "";
    retry = initialize;
    try {
      const cached = await readCatalog().catch((e) => {
        recoveryError = errorMessage(e);
        return null;
      });
      if (generation !== navigation) return;
      if (cached) {
        budgets = cached.budgets;
        cachedAt = cached.savedAt;
        loading = false;
      }
      if (readDraft({ kind: "budget" })) {
        form = "budget";
        loading = false;
        return;
      }
      const saved = loadSelection();
      const found = budgets.find((item) => item.id === saved.budgetId);
      if (found) {
        await openBudget(found, saved.accountId);
        return;
      }
      if (!offlineStatus.reachable) {
        loading = false;
        return;
      }
      const { data: info } = await request<{ data: { api_version: number } }>(
        "/info",
      );
      if (info.api_version !== 1) {
        offlineStatus.compatible = false;
        throw new Error(
          t(
            "This server uses an unsupported API version. Update the app and server together.",
          ),
        );
      }
      const fresh = await list<Budget>("/budgets");
      if (generation !== navigation) return;
      budgets = fresh;
      await saveCatalog(fresh);
      cachedAt = new Date().toISOString();
      setReachable(true);
      const selected = fresh.find((item) => item.id === saved.budgetId);
      if (selected) await openBudget(selected, saved.accountId);
    } catch (e) {
      if (generation === navigation) {
        error = errorMessage(e);
        if (e instanceof RequestError && !e.code) setReachable(false, error);
      }
    } finally {
      if (generation === navigation) loading = false;
    }
  }
  async function openBudget(value: Budget, accountId?: string) {
    const generation = ++navigation;
    recoveryError = "";
    scheduleError = "";
    budget = value;
    account = null;
    accounts = [];
    loading = true;
    error = "";
    form = readView(value.id);
    retry = () => openBudget(value, accountId);
    try {
      const cached = await readBudget(value.id).catch((e) => {
        recoveryError = errorMessage(e);
        return null;
      });
      if (generation !== navigation) return;
      if (cached) {
        accounts = sortedAccounts(cached.accounts);
        accountGroups = cached.account_groups ?? [];
        account = accounts.find((item) => item.id === accountId) ?? null;
        preferences.useCachedBudget(
          value.id,
          cached.preferences,
          cached.savedAt,
          cached.epoch,
        );
        cachedAt = cached.savedAt;
        loading = false;
      }
      saveSelection(value.id, account?.id ?? accountId);
      if (localStorage.getItem(schedulesKey(value.id)) !== null)
        form = "schedules";
      if (localStorage.getItem(rulesKey(value.id)) !== null) form = "rules";
      if (
        localStorage.getItem(planningKey(value.id)) !== null ||
        localStorage.getItem(automationKey(value.id)) !== null
      )
        form = "planning";
      try {
        if (readReferenceDraft(value.id)) {
          form = "references";
          loading = false;
          return;
        }
        if (readAccountDraft(value.id)) {
          form = "organize";
          loading = false;
          return;
        }
        if (readDraft({ kind: "account", budget_id: value.id })) {
          form = "account";
          loading = false;
          return;
        }
        if (await readImport(value.id)) {
          form = "transaction-files";
          loading = false;
          return;
        }
        if (await readRepair(value.id)) {
          form = "recovery";
          loading = false;
          return;
        }
      } catch (e) {
        recoveryError = errorMessage(e);
      }
      if (!offlineStatus.reachable) return;
      const { data: info } = await request<{ data: { api_version: number } }>(
        "/info",
      );
      if (info.api_version !== 1) {
        offlineStatus.compatible = false;
        throw new Error(
          t(
            "This server uses an unsupported API version. Update the app and server together.",
          ),
        );
      }
      scheduleError = "";
      try {
        await foregroundSchedules(value.id);
        const held = readScheduleJob(value.id);
        if (held?.review) scheduleError = held.review;
      } catch (e) {
        scheduleError = errorMessage(e);
      }
      const fresh = await refreshBudget(value.id);
      if (generation !== navigation) return;
      accounts = sortedAccounts(fresh.accounts);
      accountGroups = fresh.account_groups ?? [];
      account = accounts.find((item) => item.id === accountId) ?? null;
      preferences.useCachedBudget(
        value.id,
        fresh.preferences,
        fresh.savedAt,
        fresh.epoch,
      );
      cachedAt = fresh.savedAt;
      budget = { ...value, epoch: fresh.epoch };
      saveSelection(value.id, account?.id);
    } catch (e) {
      if (generation === navigation) {
        error = errorMessage(e);
        if (e instanceof RequestError && !e.code) setReachable(false, error);
      }
    } finally {
      if (generation === navigation) {
        loading = false;
        await focusContent();
      }
    }
  }
  async function refreshCatalog() {
    const generation = navigation;
    try {
      const fresh = await list<Budget>("/budgets");
      await saveCatalog(fresh);
      if (generation === navigation) {
        budgets = fresh;
        setReachable(true);
      }
    } catch (e) {
      if (generation === navigation) error = errorMessage(e);
    }
  }
  async function rebuildDownloadedViews() {
    navigation++;
    budget = null;
    account = null;
    accounts = [];
    accountGroups = [];
    budgets = [];
    form = null;
    preferences.clearBudget();
    await initialize();
  }
  async function showBudgets() {
    navigation++;
    recoveryError = "";
    budget = null;
    managedBudget = null;
    preferences.clearBudget();
    account = null;
    accounts = [];
    form = null;
    error = "";
    retry = initialize;
    saveSelection("");
    if (navigator.onLine) {
      setReachable(true);
      void refreshCatalog();
    }
    await focusContent();
  }
  async function selectAccount(value: Account | null) {
    navigation++;
    form = null;
    if (budget) rememberView(budget.id, null);
    account = value;
    saveSelection(budget!.id, value?.id);
    await focusContent();
  }
  async function showForm(
    value:
      | "references"
      | "organize"
      | "budget"
      | "account"
      | "planning"
      | "transaction-files"
      | "rules"
      | "schedules"
      | "reports"
      | "settings"
      | "categories"
      | "import"
      | "manage"
      | "restore"
      | "recovery"
      | "pending"
      | null,
  ) {
    navigation++;
    recoveryError = "";
    form = value;
    if (budget) rememberView(budget.id, value);
    await focusContent();
  }
  async function manage(value: ManagedBudget) {
    managedBudget = value;
    await showForm("manage");
  }
  async function lifecycleChanged(value: ManagedBudget, reason: LifecycleKind) {
    if (reason !== "duplicated") {
      managedBudget = value;
      if (value.deleted) {
        budget = null;
        account = null;
        accounts = [];
        preferences.clearBudget();
        saveSelection("");
      } else if (budget?.id === value.id) budget = value;
    }
    await refreshCatalog();
  }
  async function showDeleted() {
    error = "";
    try {
      deletedBudgets = (
        await request<{ data: ManagedBudget[] }>(
          "/budgets?include_deleted=true",
        )
      ).data.filter((value) => value.deleted);
      deletedLoaded = true;
    } catch (e) {
      error = errorMessage(e);
    }
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
    if (!offlineStatus.reachable) return;
    const generation = navigation;
    const updated = await refreshBudget(budgetId);
    if (budget?.id !== budgetId || generation !== navigation) return;
    accounts = sortedAccounts(updated.accounts);
    accountGroups = updated.account_groups ?? [];
    account = updated.accounts.find((item) => item.id === account?.id) ?? null;
    cachedAt = updated.savedAt;
  }
  onMount(() => {
    const stopDevicePreferences = preferences.startDevice();
    const stopDelivery = startDelivery();
    const stopStatus = subscribeOffline(() => {
      disconnected = !offlineStatus.reachable;
      offlineError = offlineStatus.error;
      compatible = offlineStatus.compatible;
    });
    void initialize();
    return () => {
      navigation++;
      stopStatus();
      stopDelivery();
      stopDevicePreferences();
    };
  });
</script>

<a class="skip-link" href="#main">{t("Skip to content")}</a>
<ThemeRecovery />
<div class="app-shell" class:privacy-mode={!!preferences.shared.privacy_mode}>
  <aside class="sidebar" aria-label={t("Budget navigation")}>
    <div class="brand">actualBudget</div>
    <NavigationTools
      {commands}
      budgetId={budget?.id}
      offline={disconnected}
      busy={loading}
    />

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
      {#key budget.id}<AccountNavigation
          budgetId={budget.id}
          {accounts}
          groups={accountGroups}
          selectedId={account?.id ?? null}
          disabled={!!form || loading}
          onselect={selectAccount}
        />{/key}

      <button
        onclick={() => showForm("planning")}
        disabled={!!form || loading || !compatible}>Budget</button
      >
      <button
        onclick={() => showForm("categories")}
        disabled={!!form || loading || disconnected}>{t("Categories")}</button
      >
      <button
        onclick={() => showForm("references")}
        disabled={!!form || loading || disconnected || !compatible}
        >{t("Payees and tags")}</button
      >
      <button
        onclick={() => showForm("transaction-files")}
        disabled={!!form || loading || !compatible}>Transaction files</button
      >
      <button
        onclick={() => {
          rulesPayee = "";
          void showForm("rules");
        }}
        disabled={!!form || loading || !compatible}>Rules</button
      >
      <button
        onclick={() => showForm("schedules")}
        disabled={!!form || loading || !compatible}>Schedules</button
      >
      <button
        onclick={() => showForm("reports")}
        disabled={!!form || loading || !compatible}
        >{t("Dashboards and reports")}</button
      >
      <button
        class={{ current: form === "settings" }}
        aria-current={form === "settings" ? "page" : undefined}
        onclick={() => showForm("settings")}
        disabled={(!!form && form !== "settings") || loading}
        >{t("Settings")}</button
      >
      <button
        onclick={() => budget && manage(budget)}
        disabled={!!form || loading || disconnected || !compatible}
        >{t("Manage budget")}</button
      >
    {:else}<p class="muted sidebar-description">
        {t("Your accounts, together.")}
      </p>{/if}
    <button onclick={() => showForm("recovery")} disabled={!!form || loading}
      >Recovery tools</button
    >
    <button onclick={() => showForm("pending")} disabled={!!form || loading}
      >{t("Review pending expenses")}</button
    >
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
    {#if offlineError && offlineError !== error}<p role="alert" class="error">
        {offlineError}
      </p>{/if}
    {#if cachedAt}<p class="hint" role="status">
        {disconnected
          ? t("Disconnected. Saved data from")
          : t("Saved data from")}
        {new Date(cachedAt).toLocaleString()}.
      </p>{/if}
    {#if error && (budget || budgets.length || form)}<p
        class="error"
        role="alert"
      >
        {error}
      </p>
      <button
        onclick={() => {
          setReachable(navigator.onLine);
          void retry();
        }}>{t("Retry connection")}</button
      >{/if}
    {#if scheduleError && budget && form !== "schedules"}<div role="status">
        <p>Scheduled entries need attention: {scheduleError}</p>
        <button onclick={() => showForm("schedules")}
          >Review scheduled entries</button
        >
      </div>{/if}
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
    {:else if error && !budget && !budgets.length && !form}<section>
        <h1>{t("Unable to open your data")}</h1>
        <p class="error" role="alert">{error}</p>
        <button class="primary" onclick={retry}>{t("Retry")}</button>
      </section>
    {:else if form === "recovery"}<RecoveryTools
        {budget}
        onclose={() => showForm(null)}
        onreset={rebuildDownloadedViews}
        onchanged={refreshAccounts}
      />
    {:else if form === "pending"}<PendingRecovery onclose={showBudgets} />
    {:else if form === "manage" && managedBudget}{#key managedBudget.id}<BudgetManagement
          budget={managedBudget}
          onchanged={lifecycleChanged}
          onclose={showBudgets}
        />{/key}
    {:else if form === "restore"}<RestoreBudget
        onrestored={budgetCreated}
        onclose={showBudgets}
      />
    {:else if form === "budget"}<BudgetForm
        oncreated={budgetCreated}
        oncancel={() => showForm(null)}
      />
    {:else if form === "references" && budget}{#key budget.id}<ReferenceManagement
          budgetId={budget.id}
          onchanged={refreshAccounts}
          onrules={(id) => {
            rulesPayee = id;
            void showForm("rules");
          }}
          onclose={() => showForm(null)}
        />{/key}
    {:else if form === "schedules" && budget}{#key budget.id}<Schedules
          budgetId={budget.id}
          {disconnected}
          onclose={() => showForm(null)}
          onchanged={async () => {
            scheduleError = "";
            await refreshAccounts();
          }}
        />{/key}
    {:else if form === "transaction-files" && budget}<TransactionImport
        budgetId={budget.id}
        {accounts}
        onclose={() => showForm(null)}
        onchanged={async () => {
          accounts = sortedAccounts(
            await list<Account>(`/budgets/${budget!.id}/accounts`),
          );
          account = accounts.find((a) => a.id === account?.id) ?? null;
        }}
      />
    {:else if form === "rules" && budget}{#key budget.id}<Rules
          budgetId={budget.id}
          {disconnected}
          initialPayee={rulesPayee}
          onschedules={() => showForm("schedules")}
          onclose={() => showForm(null)}
          onchanged={refreshAccounts}
        />{/key}
    {:else if form === "reports" && budget}{#key budget.id + reportEntryKey}<Reports
          budgetId={budget.id}
          {accounts}
          {disconnected}
          onclose={() => showForm(null)}
        />{/key}
    {:else if form === "organize" && budget}{#key budget.id}<AccountManagement
          budgetId={budget.id}
          selectedId={account?.id ?? ""}
          onchanged={refreshAccounts}
          onclose={() => showForm(null)}
        />{/key}
    {:else if form === "account" && budget}<AccountForm
        budgetId={budget.id}
        oncreated={accountCreated}
        oncancel={() => showForm(null)}
      />
    {:else if form === "categories" && budget}<Categories
        budgetId={budget.id}
        onclose={() => openBudget(budget!, account?.id)}
      />
    {:else if form === "planning" && budget}{#key budget.id}<BudgetPlan
          budgetId={budget.id}
          {disconnected}
          onclose={() => showForm(null)}
          onchanged={async (method) => {
            if (budget) budget = { ...budget, budget_type: method };
            await refreshAccounts();
          }}
          onaccount={(id) => {
            const found = accounts.find((a) => a.id === id);
            if (found) {
              form = null;
              void selectAccount(found);
            }
          }}
        />{/key}
    {:else if form === "settings" && budget}<Settings
        budgetId={budget.id}
        onclose={() => showForm(null)}
      />
    {:else if form === "import"}<ImportBudget
        initialSource={importSource}
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
          <button
            onclick={() => showForm("restore")}
            disabled={disconnected || !compatible}>{t("Restore backup")}</button
          >
          <button
            onclick={() => {
              importSource = "actual";
              showForm("import");
            }}
            disabled={disconnected || !compatible}
            >{t("Import Actual budget")}</button
          >
          <button
            onclick={() => {
              importSource = "ynab4";
              showForm("import");
            }}
            disabled={disconnected || !compatible}
            >{t("Import YNAB budget")}</button
          >
          <button
            class="primary"
            onclick={() => showForm("budget")}
            disabled={disconnected || !compatible}>{t("New budget")}</button
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
      <section aria-label={t("Deleted budget recovery")}>
        <button onclick={showDeleted} disabled={disconnected || !compatible}
          >{t("Find deleted budgets")}</button
        >
        {#if deletedBudgets.length}<ul class="budget-list">
            {#each deletedBudgets as item (item.id)}<li>
                <button onclick={() => manage(item)}
                  ><span>{item.name}</span><span
                    >{t("View recovery backups")}</span
                  ></button
                >
              </li>{/each}
          </ul>{:else if deletedLoaded}<p>{t("No deleted budgets.")}</p>{/if}
      </section>
    {:else if !preferences.hasUsableShared}
      <section>
        <h1>{t("Display preferences are unavailable")}</h1>
        <p>
          {t(
            "Refresh preferences before viewing amounts or entering new transactions. Saved requests keep their original amounts.",
          )}
        </p>
        {#if !preferences.error}<button
            onclick={() => {
              if (!budget) return;
              setReachable(navigator.onLine);
              void openBudget(budget, account?.id);
            }}>{t("Retry preferences")}</button
          >{/if}
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
        onorganize={() => showForm("organize")}
        onschedules={() => showForm("schedules")}
      />
    {:else}
      <div class="page-heading">
        <div>
          <h1>{t("Accounts")}</h1>
          <p class="muted">{budget.name}</p>
        </div>
        <button
          onclick={() => showForm("organize")}
          disabled={disconnected || !compatible}
          >{t("Organize accounts")}</button
        >
        <button
          class="primary"
          onclick={() => showForm("account")}
          disabled={disconnected || !compatible}>{t("Add account")}</button
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
                      onclick={() => selectAccount(item)}
                      >{item.name}{item.closed
                        ? " — " + t("Closed")
                        : ""}</button
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
