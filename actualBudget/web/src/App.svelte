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
  import { readDraft } from "./lib/drafts";
  import BudgetForm from "./lib/BudgetForm.svelte";
  import AccountForm from "./lib/AccountForm.svelte";
  import AccountManagement from "./lib/AccountManagement.svelte";
  import ReferenceManagement from "./lib/ReferenceManagement.svelte";
  import { readReferenceDraft } from "./lib/reference-actions";
  import { readAccountDraft, sortedAccounts } from "./lib/account-actions";
  import BudgetPlan from "./lib/BudgetPlan.svelte";
  import { readImport } from "./lib/transaction-import-storage";
  import ImportHub from "./lib/ImportHub.svelte";
  import SettingsNavigation from "./lib/SettingsNavigation.svelte";
  import Rules from "./lib/Rules.svelte";
  import Schedules from "./lib/Schedules.svelte";
  import ScheduleUpcoming from "./lib/ScheduleUpcoming.svelte";
  import { schedulesKey } from "./lib/schedules";
  import {
    foregroundSchedules,
    readScheduleJob,
  } from "./lib/schedule-foreground";
  import { rulesKey } from "./lib/rules";
  import { planningKey } from "./lib/planning";
  import Register from "./lib/Register.svelte";
  import Settings from "./lib/Settings.svelte";
  import RecoveryTools from "./lib/RecoveryTools.svelte";
  import Reports from "./lib/Reports.svelte";
  let reportEntryKey = $state(0);
  import { writeValue } from "./lib/offline/storage";
  import { readRepair } from "./lib/recovery";
  import NavigationTools from "./lib/NavigationTools.svelte";
  import {
    readBrowserRoute,
    writeBrowserRoute,
    type BrowserRoute,
    readView,
    rememberView,
    type NavigationCommand,
  } from "./lib/navigation";
  import PendingRecovery from "./lib/offline/PendingRecovery.svelte";
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
    | "import"
    | "manage"
    | "restore"
    | "recovery"
    | "pending"
    | null
  >(null);
  const settingsScreen = $derived(
    ["settings", "import", "transaction-files", "recovery", "pending"].includes(
      form ?? "",
    ),
  );
  let importSource = $state<"actual" | "ynab4">("actual");
  let rulesPayee = $state("");
  let managedBudget = $state.raw<ManagedBudget | null>(null);
  let deletedBudgets = $state.raw<ManagedBudget[]>([]);
  let deletedLoaded = $state(false);
  let main: HTMLElement | undefined;
  let retry = $state<() => Promise<void>>(initialize);
  // Register only delivered destinations. Availability shares the sidebar's edit guards.
  let commands = $derived.by((): NavigationCommand[] => {
    const locked = loading;
    const result: NavigationCommand[] = [
      {
        id: "recovery",
        label: t("Recovery tools"),
        keywords: "repair recalculate reconnect cache reset",
        disabled: locked,
        run: () => showForm("recovery"),
      },
      {
        id: "budgets",
        label: t("All budgets"),
        disabled: locked,
        run: showBudgets,
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
          label: t(label),
          disabled: locked || disconnected || !compatible,
          run: () => {
            if (id === "import") importSource = "actual";
            showForm(id);
          },
        });
      result.push({
        id: "ynab-import",
        label: t("Import YNAB budget"),
        keywords: "YNAB4 nYNAB JSON migration",
        disabled: locked || disconnected || !compatible,
        run: () => {
          importSource = "ynab4";
          showForm("import");
        },
      });
      return result;
    }
    result.push({
      id: "manage",
      label: t("Manage budget"),
      disabled: locked || disconnected || !compatible,
      run: () => manage(budget!),
    });
    result.push({
      id: "custom-reports",
      label: t("Custom reports"),
      disabled: locked || !compatible,
      run: async () => {
        await writeValue(`view:reporting-tab:${budget!.id}`, "custom");
        reportEntryKey++;
        showForm("reports");
      },
    });
    result.push({
      id: "crossover",
      label: t("Crossover Point"),
      disabled: locked || !compatible,
      run: async () => {
        await writeValue(`view:reporting-tab:${budget!.id}`, "crossover");
        reportEntryKey++;
        showForm("reports");
      },
    });
    const views = [
      ["planning", "Budget", false],
      ["references", "Payees", disconnected],
      ["import", "Import", false],
      ["rules", "Rules", false],
      ["reports", "Dashboards and reports", false],
      ["settings", "Settings", false],
    ] as const;
    result.push({
      id: "transactions",
      label: t("Transaction history"),
      disabled: locked,
      run: () => selectAccount(null),
    });
    for (const [id, label, requiresConnection] of views)
      result.push({
        id,
        label: t(label),
        disabled:
          locked || requiresConnection || (id !== "settings" && !compatible),
        run: () => {
          rulesPayee = "";
          return showForm(id);
        },
      });
    return result;
  });
  let historyReady = false;
  let restoringHistory = false;
  let historyGeneration = 0;
  let reportTab = $state<string | null>(null);
  let transactionFilter = $state<BrowserRoute["transactions"]>(null);
  const initialRoute = readBrowserRoute();
  function recordNavigation(replace = false) {
    if (!historyReady || restoringHistory) return;
    writeBrowserRoute(
      {
        budgetId: budget?.id ?? "",
        screen: form,
        transactions: transactionFilter,
        report: reportTab,
        source: importSource,
      },
      replace,
    );
  }
  function reportNavigated(value: string, replace = false) {
    reportTab = value;
    recordNavigation(replace);
  }
  async function restoreNavigation(route: BrowserRoute | null) {
    if (!route) return;
    const generation = ++historyGeneration;
    restoringHistory = true;
    try {
      if (route.budgetId && budget?.id !== route.budgetId) {
        const found = budgets.find((b) => b.id === route.budgetId);
        if (!found) {
          await showBudgets();
          return;
        }
        await openBudget(found);
      } else if (!route.budgetId) await showBudgets();
      if (generation !== historyGeneration) return;
      importSource = route.source;
      if (route.screen === "reports") reportEntryKey++;
      if (route.screen === "manage") managedBudget = budget;
      reportTab = route.report;
      transactionFilter = route.transactions ?? null;
      // Old account URLs may reopen durable recovery, never start retired workflows.
      let recovery = true;
      try {
        recovery =
          route.screen === "organize"
            ? !!budget && !!readAccountDraft(budget.id)
            : route.screen === "account"
              ? !!budget &&
                !!readDraft({ kind: "account", budget_id: budget.id })
              : true;
      } catch {
        // Unreadable saved work still needs the editor's recovery message.
      }
      form = recovery ? route.screen : null;
      if (budget) rememberView(budget.id, form);
      await focusContent();
    } finally {
      if (generation === historyGeneration) {
        restoringHistory = false;
        recordNavigation(true);
      }
    }
  }
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
      const saved =
        !historyReady && initialRoute
          ? { budgetId: initialRoute.budgetId, accountId: undefined }
          : loadSelection();
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
    transactionFilter = null;
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
        account = null;
        preferences.useCachedBudget(
          value.id,
          cached.preferences,
          cached.savedAt,
          cached.epoch,
        );
        cachedAt = cached.savedAt;
        loading = false;
      }
      saveSelection(value.id);
      if (localStorage.getItem(schedulesKey(value.id)) !== null)
        form = "schedules";
      if (localStorage.getItem(rulesKey(value.id)) !== null) form = "rules";
      if (
        localStorage.getItem(planningKey(value.id)) !== null ||
        localStorage.getItem(`actualBudget.categories.v1.${value.id}`) !== null
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
      account = null;
      preferences.useCachedBudget(
        value.id,
        fresh.preferences,
        fresh.savedAt,
        fresh.epoch,
      );
      cachedAt = fresh.savedAt;
      budget = { ...value, epoch: fresh.epoch };
      saveSelection(value.id);
    } catch (e) {
      if (generation === navigation) {
        error = errorMessage(e);
        if (e instanceof RequestError && !e.code) setReachable(false, error);
      }
    } finally {
      if (generation === navigation) {
        loading = false;
        recordNavigation();
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
    loading = false;
    recordNavigation();
    if (navigator.onLine) {
      setReachable(true);
      void refreshCatalog();
    }
    await focusContent();
  }
  async function selectAccount(value: Account | null) {
    navigation++;
    transactionFilter = null;
    form = null;
    if (budget) rememberView(budget.id, null);
    account = null;
    saveSelection(budget!.id);
    recordNavigation();
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
      | "import"
      | "manage"
      | "restore"
      | "recovery"
      | "pending"
      | null,
  ) {
    navigation++;
    recoveryError = "";
    if (value !== "reports" || form !== "reports") reportTab = null;
    transactionFilter = null;
    form = value;
    if (budget) rememberView(budget.id, value);
    recordNavigation();
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
  async function startTransactionImport(value: Budget) {
    budgets = [...budgets.filter((b) => b.id !== value.id), value];
    await openBudget(value);
    await showForm("transaction-files");
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
    void initialize().then(async () => {
      if (initialRoute) await restoreNavigation(initialRoute);
      historyReady = true;
      recordNavigation(true);
    });
    return () => {
      navigation++;
      stopStatus();
      stopDelivery();
      stopDevicePreferences();
    };
  });
</script>

<svelte:window onpopstate={() => void restoreNavigation(readBrowserRoute())} />

<a class="skip-link" href="#main">{t("Skip to content")}</a>
<div class="app-shell">
  <aside class="sidebar" aria-label={t("Budget navigation")}>
    <div class="brand">actualBudget</div>
    <NavigationTools {commands} />

    {#if budget}
      <button class="quiet" onclick={showBudgets} disabled={loading}
        >{t("All budgets")}</button
      >
      <div class="budget-name">{budget.name}</div>
      <nav aria-label={t("Views")}>
        <button
          class:current={!form}
          aria-current={!form ? "page" : undefined}
          onclick={() => selectAccount(null)}
          disabled={loading}>{t("Transaction history")}</button
        >

        <button
          class:current={form === "planning"}
          aria-current={form === "planning" ? "page" : undefined}
          onclick={() => showForm("planning")}
          disabled={loading || !compatible}>{t("Budget")}</button
        >
        <button
          class:current={form === "references"}
          aria-current={form === "references" ? "page" : undefined}
          onclick={() => showForm("references")}
          disabled={loading || disconnected || !compatible}
          >{t("Payees")}</button
        >
        <button
          onclick={() => {
            rulesPayee = "";
            void showForm("rules");
          }}
          disabled={loading || !compatible}>{t("Rules")}</button
        >
        <button
          class:current={form === "reports"}
          aria-current={form === "reports" ? "page" : undefined}
          onclick={() => showForm("reports")}
          disabled={loading || !compatible}
          >{t("Dashboards and reports")}</button
        >
        <button
          class={{ current: settingsScreen }}
          aria-current={settingsScreen ? "page" : undefined}
          onclick={() => showForm("settings")}
          disabled={loading}>{t("Settings")}</button
        >
        <button
          onclick={() => budget && manage(budget)}
          disabled={loading || disconnected || !compatible}
          >{t("Manage budget")}</button
        >
      </nav>
    {:else}<p class="muted sidebar-description">
        {t("Your transactions, together.")}
      </p>{/if}
    {#if !budget}<button
        class:current={!form}
        aria-current={!form ? "page" : undefined}
        onclick={showBudgets}
        disabled={loading}>Budgets</button
      ><button
        class:current={settingsScreen}
        aria-current={settingsScreen ? "page" : undefined}
        onclick={() => showForm("settings")}
        disabled={loading}>Settings</button
      >{/if}
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
    {#if cachedAt && disconnected}<p class="hint" role="status">
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
    {#if settingsScreen}<SettingsNavigation
        active={form === "settings"
          ? "general"
          : form === "import" || form === "transaction-files"
            ? "import"
            : "recovery"}
        onchange={(value) => showForm(value)}
      />{/if}
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
    {:else if form === "recovery"}<button onclick={() => showForm("pending")}
        >Troubleshoot saved changes</button
      ><RecoveryTools
        {budget}
        onclose={() => showForm("settings")}
        onreset={rebuildDownloadedViews}
        onchanged={refreshAccounts}
      />
    {:else if form === "pending"}<PendingRecovery
        onclose={() => showForm("recovery")}
      />
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
    {:else if form === "import" || form === "transaction-files"}{#key (budget?.id ?? "") + form}<ImportHub
          {budgets}
          {budget}
          {accounts}
          initialSource={importSource}
          {disconnected}
          transactionsReady={form === "transaction-files"}
          onstart={startTransactionImport}
          oncreated={budgetCreated}
          onclose={() => showForm("settings")}
          onchanged={refreshAccounts}
        />{/key}
    {:else if form === "rules" && budget}{#key budget.id}<Rules
          budgetId={budget.id}
          {disconnected}
          initialPayee={rulesPayee}
          onschedules={() => showForm("schedules")}
          onclose={() => showForm(null)}
          onchanged={refreshAccounts}
        />{/key}
    {:else if form === "reports" && budget}{#key budget.id + reportEntryKey}<Reports
          initialTab={reportTab}
          ontabchange={reportNavigated}
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
    {:else if form === "planning" && budget}{#key budget.id}<BudgetPlan
          budgetId={budget.id}
          {disconnected}
          onchanged={async (method) => {
            if (budget) budget = { ...budget, budget_type: method };
            await refreshAccounts();
          }}
          ontransactions={(category, month) => {
            transactionFilter = { category, month };
            form = null;
            if (budget) rememberView(budget.id, null);
            recordNavigation();
            void focusContent();
          }}
        />{/key}
    {:else if form === "settings"}<Settings
        budgetId={budget?.id}
        onclose={() => showForm(null)}
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
            onclick={() => showForm("import")}
            disabled={disconnected || !compatible}>Import</button
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
                ><span class="list-name">{item.name}</span><span
                  >{t("Open")}</span
                ></button
              >
            </li>{/each}
        </ul>
      {:else}<div class="empty-state">
          <h2>{t("A place for your transactions")}</h2>
          <p>
            {t("Create your first budget, then record your transactions.")}
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
    {:else}
      {#key budget.id + ":all:" + JSON.stringify(transactionFilter)}<Register
          categoryFilter={transactionFilter}
          budgetId={budget.id}
          accountId={null}
          onchanged={refreshAccounts}
        />{/key}
      {#key budget.id + ":" + cachedAt}<ScheduleUpcoming
          budgetId={budget.id}
          onmanage={() => showForm("schedules")}
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
