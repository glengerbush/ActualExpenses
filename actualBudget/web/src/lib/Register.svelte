<script lang="ts">
  import { historyOwner } from "./history-owner";
  import {
    readBudget,
    refreshBudget,
    readView,
    saveView,
  } from "./offline/cache";
  import {
    entries,
    receipts,
    deleteOfflineExpense,
    retryEntry,
    enqueue,
    type OutboxEntry,
  } from "./offline/outbox";
  import {
    offlineStatus,
    subscribeOffline,
    deliverPending,
  } from "./offline/delivery";
  import { setReachable } from "./offline/events";
  import { onMount, tick } from "svelte";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { list, uuid, RequestError, errorMessage, type Account } from "./api";
  import { t } from "./i18n.svelte";
  import { formatAmount, formatDate, formatAmountInput } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    readTransactionDraft,
    clearTransactionDraft,
    readRegisterAction,
    saveRegisterAction,
    clearRegisterAction,
    type SavedAction,
  } from "./register-storage";
  import BulkActions from "./BulkActions.svelte";
  import SplitEditor from "./SplitEditor.svelte";
  import RegisterTools from "./RegisterTools.svelte";
  import {
    defaultQuery,
    defaultColumns,
    columnLabels,
    isDefaultQuery,
    type RegisterQuery,
  } from "./register-query";
  import TransactionForm from "./TransactionForm.svelte";
  import DateInput from "./DateInput.svelte";
  import {
    registerPage,
    changes,
    references,
    mutate,
    sameMutation,
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
  let disconnected = $state(!offlineStatus.reachable);
  let compatible = $state(offlineStatus.compatible);
  let cachedAt = $state<string | null>(null);
  let queue = $state.raw<OutboxEntry[]>([]);
  let queued = $state.raw<OutboxEntry | null>(null);
  let receiptKeys: string | null = null;
  let deliveryNotice = $state("");
  let mounted = true;
  async function readPending() {
    try {
      const [items, acknowledged] = await Promise.all([
        entries(budgetId),
        receipts(budgetId),
      ]);
      if (!mounted) return;
      const oldDraft = readTransactionDraft(budgetId);
      if (oldDraft?.pending && recoveredPending) {
        const owned =
          items.find((item) => item.key === oldDraft.draftId)?.pending ??
          acknowledged.find((item) => item.key === oldDraft.draftId)?.pending;
        if (owned && sameMutation(owned, oldDraft.pending)) {
          clearTransactionDraft(oldDraft, true);
          recoveredPending = false;
          showForm = false;
          editing = null;
        }
      }
      const oldAction = readRegisterAction(budgetId);
      if (oldAction) {
        const owned =
          items.find((item) => item.key === oldAction.key)?.pending ??
          acknowledged.find((item) => item.key === oldAction.key)?.pending;
        if (owned && sameMutation(owned, oldAction)) {
          clearRegisterAction(budgetId, oldAction);
          pending = null;
        }
      }
      queue = items.filter(
        (item) => !accountId || item.row.account_id === accountId,
      );
      disconnected = !offlineStatus.reachable;
      compatible = offlineStatus.compatible;
      const keys = acknowledged.map((item) => item.key).join(",");
      deliveryNotice = acknowledged.at(-1)?.message ?? "";
      if (
        receiptKeys !== null &&
        keys !== receiptKeys &&
        offlineStatus.reachable
      ) {
        void load();
        void onchanged().catch(() => {});
        notice = acknowledged.at(-1)?.message ?? "";
      }
      receiptKeys = keys;
    } catch (e) {
      recoveryError = errorMessage(e);
    }
  }
  async function editQueued(item: OutboxEntry) {
    queued = item;
    editing = item.attempted ? item.row : null;
    showForm = true;
  }
  async function deleteQueued(item: OutboxEntry) {
    try {
      await deleteOfflineExpense(item, {
        path: `/budgets/${budgetId}/transactions/${item.row.id}`,
        method: "DELETE",
        key: uuid(),
        edit_clock: nextEditClock(item.row.field_versions),
        budgetEpoch: item.pending.budgetEpoch,
      });
      await readPending();
      void deliverPending();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function retryQueued(item: OutboxEntry, cancel = false) {
    try {
      await retryEntry(item, cancel);
      setReachable(navigator.onLine);
      await deliverPending();
      if (
        cancel &&
        (await receipts(budgetId)).some((receipt) => receipt.key === item.key)
      )
        resetCorrectedClock();
      await readPending();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let registerQuery = $state<RegisterQuery>(defaultQuery());
  let confirmedQuery = $state<RegisterQuery>(defaultQuery());
  let extraBalances = $state(true);
  let visibleColumns = $state<string[]>([...defaultColumns]);
  let appliedColumns = $derived(
    visibleColumns.filter((column) => column !== "account" || !accountId),
  );
  async function applyRegisterQuery(
    query: RegisterQuery,
    columns: string[],
    date: string,
    showExtraBalances = true,
  ) {
    extraBalances = showExtraBalances;
    registerQuery = query;
    visibleColumns = columns;
    return userRefresh(date);
  }
  function cell(row: Transaction, column: string) {
    switch (column) {
      case "date":
        return formatDate(row.date);
      case "account":
        return row.account_name;
      case "payee":
        return row.payee_name ?? t("No payee");
      case "notes":
        return row.notes;
      case "group":
        return row.category_group_name ?? "";
      case "category":
        return row.is_parent
          ? `${row.children?.length ?? 0} splits`
          : (row.category_name ?? t("Uncategorized"));
      case "amount":
        return formatAmount(row.amount);
      case "payment":
        return row.amount < 0 ? formatAmount(-row.amount) : "";
      case "deposit":
        return row.amount > 0 ? formatAmount(row.amount) : "";
      case "balance":
        return row.running_balance_error
          ? "Outside exact range"
          : row.running_balance == null
            ? "Unavailable"
            : formatAmount(row.running_balance);
      case "status":
        return row.reconciled
          ? t("Reconciled")
          : row.cleared
            ? t("Cleared")
            : t("Uncleared");
      default:
        return "";
    }
  }
  function navigateRow(event: KeyboardEvent) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const target = event.target as HTMLElement;
    if (target.tagName !== "BUTTON") return;
    const row = target.closest("tr");
    const table = row?.closest("table");
    const rows = Array.from(
      table?.querySelectorAll<HTMLTableRowElement>(
        "tbody tr:not(.delete-confirmation)",
      ) ?? [],
    );
    const index = rows.indexOf(row as HTMLTableRowElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : Math.max(
              0,
              Math.min(
                rows.length - 1,
                index + (event.key === "ArrowDown" ? 1 : -1),
              ),
            );
    const button = rows[next]?.querySelector<HTMLButtonElement>(
      "button:not(:disabled)",
    );
    if (button) {
      event.preventDefault();
      button.focus();
    }
  }
  let selectedIds = $state<string[]>([]);
  let bulkActive = $state(false);
  let bulkActions = $state<{ refresh: () => Promise<void> } | undefined>();
  let selectionAnchor: string | null = null;
  function selectRow(event: MouseEvent, row: Transaction) {
    const checked = (event.currentTarget as HTMLInputElement).checked;
    const rows = selectionRows;
    let ids = [row.id];
    if (event.shiftKey && selectionAnchor) {
      const a = rows.findIndex((item) => item.id === selectionAnchor),
        b = rows.findIndex((item) => item.id === row.id);
      if (a >= 0)
        ids = rows
          .slice(Math.min(a, b), Math.max(a, b) + 1)
          .map((item) => item.id);
    }
    const next = new Set(selectedIds);
    for (const id of ids) {
      if (checked) next.add(id);
      else next.delete(id);
    }
    if (next.size > 100) {
      error = "Select at most 100 entries per action.";
      return;
    }
    selectedIds = [...next];
    selectionAnchor = row.id;
  }
  let register = $state.raw<RegisterPage | null>(null);
  let expandedSplits = $state<string[]>([]);
  let selectionRows = $derived(
    register?.data.flatMap((row) => [
      row,
      ...(expandedSplits.includes(row.id) ? (row.children ?? []) : []),
    ]) ?? [],
  );
  let selectedRows = $derived(
    selectionRows.filter((row) => selectedIds.includes(row.id)),
  );
  let accounts = $state.raw<Account[]>([]);
  let payees = $state.raw<Reference[]>([]);
  let categories = $state.raw<Reference[]>([]);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let recoveryError = $state("");
  let localClockProblem = $state(false);
  let viewMatchesEpoch = $derived(
    !register ||
      !preferences.budgetEpoch ||
      register.snapshot.slice(2, 34) === preferences.budgetEpoch,
  );
  let asOf = $derived(register?.as_of ?? "");
  let retryCutoff = "";
  let loadGeneration = 0;
  let notice = $state("");
  let filterDate = $state("");
  let showForm = $state(false);
  let recoveredPending = $state(false);
  let editing = $state.raw<Transaction | null>(null);
  let splitEditor = $state<
    { open: (row: Transaction) => Promise<void> } | undefined
  >();
  let splitActive = $state(false);
  let deleting = $state.raw<Transaction | null>(null);
  let deletionEpoch = $state<string | undefined>();
  let pending = $state<SavedAction | null>(null);
  async function load() {
    return refresh(asOf);
  }
  async function userRefresh(cutoff: string) {
    setReachable(navigator.onLine);
    return refresh(cutoff);
  }
  async function refresh(cutoff: string) {
    const generation = ++loadGeneration;
    const requestedQuery = $state.snapshot(registerQuery);
    retryCutoff = cutoff;
    loading = true;
    error = "";
    notice = "";
    try {
      const [cached, cachedBudget] = await Promise.all([
        (isDefaultQuery(requestedQuery)
          ? readView(budgetId, accountId, cutoff)
          : Promise.resolve(null)
        ).catch((e) => {
          error = errorMessage(e);
          return null;
        }),
        readBudget(budgetId).catch((e) => {
          error = errorMessage(e);
          return null;
        }),
      ]);
      if (generation !== loadGeneration) return false;
      if (cached) {
        register = cached.page;
        confirmedQuery = defaultQuery();
        filterDate = cached.page.as_of ?? "";
        cachedAt = cached.savedAt;
        loading = offlineStatus.reachable;
      }
      if (cachedBudget) {
        accounts = cachedBudget.accounts;
        payees = cachedBudget.payees;
        categories = cachedBudget.categories;
        preferences.useCachedBudget(
          budgetId,
          cachedBudget.preferences,
          cachedBudget.savedAt,
          cachedBudget.epoch,
        );
      }
      if (!offlineStatus.reachable) {
        disconnected = true;
        return !!cached;
      }
      const [page, referenceData] = await Promise.all([
        registerPage(budgetId, accountId, 0, cutoff, undefined, requestedQuery),
        refreshBudget(budgetId),
      ]);
      if (generation !== loadGeneration) return false;
      for (const row of page.data) observeVersions(row.field_versions);
      register = page;
      confirmedQuery = requestedQuery;
      try {
        cachedAt = isDefaultQuery(requestedQuery)
          ? await saveView(budgetId, accountId, page)
          : null;
      } catch (e) {
        cachedAt = null;
        error = errorMessage(e);
      }
      register = page;
      filterDate = page.as_of ?? "";
      accounts = referenceData.accounts;
      payees = referenceData.payees;
      categories = referenceData.categories;
      preferences.useCachedBudget(
        budgetId,
        referenceData.preferences,
        referenceData.savedAt,
        referenceData.epoch,
      );
      disconnected = false;
      selectedIds = [];
      return true;
    } catch (e) {
      if (generation === loadGeneration) {
        error = errorMessage(e);
        if (e instanceof RequestError && !e.code) {
          setReachable(false, error);
          disconnected = true;
        }
      }
      return false;
    } finally {
      if (generation === loadGeneration) {
        loading = false;
        void tick().then(() => bulkActions?.refresh());
      }
    }
  }
  function recover() {
    recoveryError = "";
    try {
      const saved = readTransactionDraft(budgetId);
      showForm = !!saved;
      recoveredPending = !!saved?.pending;
      pending = readRegisterAction(budgetId);
    } catch (e) {
      recoveryError = errorMessage(e);
    }
  }
  onMount(() => {
    recover();
    void readPending();
    void load();
    const stop = subscribeOffline(() => void readPending());
    return () => {
      mounted = false;
      loadGeneration++;
      stop();
    };
  });

  async function changed(message = "") {
    showForm = false;
    editing = null;
    queued = null;
    deleting = null;
    await readPending();
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
    setReachable(navigator.onLine);
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
    setReachable(navigator.onLine);
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
        confirmedQuery,
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
      if (generation === loadGeneration) {
        loading = false;
        void tick().then(() => bulkActions?.refresh());
      }
    }
  }
  function open(transaction: Transaction | null) {
    if (transaction && disconnected) return;
    if (transaction?.is_parent || transaction?.parent_id) {
      const parent = transaction.parent_id
        ? register?.data.find((row) => row.id === transaction.parent_id)
        : transaction;
      if (parent) void splitEditor?.open(parent);
      return;
    }
    queued = null;
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
          historyOwner: await historyOwner(),
          edit_clock: nextEditClock(row.field_versions),
          budgetEpoch:
            action === "delete" ? deletionEpoch : preferences.budgetEpoch,
          body: action === "duplicate" ? { id: uuid() } : undefined,
          description: `${action === "delete" ? t("Delete") : t("Duplicate")} ${description(row)}`,
        };
      }
      pending = { ...pending, clockRejected: false, attemptId: uuid() };
      saveRegisterAction(budgetId, pending);
      const submitted = pending;
      if (row && !row.is_parent && !row.parent_id) {
        const preview = {
          ...row,
          id:
            action === "duplicate"
              ? (pending.body as { id: string }).id
              : row.id,
        };
        await enqueue({
          budgetId,
          pending: $state.snapshot(pending),
          row: preview,
          offlineCreated: false,
        });
        await deliverPending();
        const receipt = (await receipts(budgetId)).find((r) =>
          sameMutation(r.pending, submitted),
        );
        terminalNotice =
          receipt?.message ?? t("Saved on this device. Waiting to sync.");
      } else await mutate(pending);
      acknowledged = true;
      clearRegisterAction(budgetId, submitted);
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
      disabled={loading ||
        busy ||
        showForm ||
        splitActive ||
        bulkActive ||
        !!pending ||
        !!recoveryError}>{t("Refresh transactions")}</button
    >
    <button
      class="primary"
      onclick={() => open(null)}
      disabled={loading ||
        busy ||
        bulkActive ||
        !!pending ||
        showForm ||
        splitActive ||
        accounts.length === 0 ||
        !!recoveryError ||
        !preferences.hasUsableShared ||
        !viewMatchesEpoch ||
        !compatible}>{t("Add transaction")}</button
    >
  </div>
  {#if register && preferences.hasUsableShared && viewMatchesEpoch}
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
      {#if extraBalances}
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
      {/if}
    </dl>
    <form
      class="date-filter"
      onsubmit={(event) => {
        event.preventDefault();
        void userRefresh(filterDate);
      }}
    >
      <DateInput
        id="register-as-of"
        label={t("Show through date")}
        bind:value={filterDate}
        disabled={loading || busy}
      />
      <button disabled={loading || busy || disconnected}>{t("Apply")}</button>
      {#if asOf}<button
          type="button"
          onclick={() => {
            void userRefresh("");
          }}
          disabled={loading || busy}>{t("All dates")}</button
        >
      {:else}<span class="hint">{t("Includes future-dated entries.")}</span
        >{/if}
    </form>
  {/if}
  <BulkActions
    bind:this={bulkActions}
    bind:active={bulkActive}
    {budgetId}
    selected={selectedRows}
    snapshot={register?.snapshot ?? ""}
    {accounts}
    {payees}
    {categories}
    disabled={disconnected ||
      loading ||
      busy ||
      showForm ||
      splitActive ||
      !!pending ||
      !!recoveryError ||
      !preferences.hasUsableShared ||
      !viewMatchesEpoch}
    onchanged={() => changed()}
    onclear={() => {
      selectedIds = [];
    }}
  />
  <RegisterTools
    locked={bulkActive}
    {accounts}
    {payees}
    {categories}
    {budgetId}
    {accountId}
    {disconnected}
    cutoff={asOf}
    onapply={applyRegisterQuery}
  />
  {#if register?.filtered_balances && !isDefaultQuery(confirmedQuery)}<p
      role="status"
      data-testid="filtered-totals"
    >
      {register.filtered_count} matching transactions · Total {formatAmount(
        register.filtered_balances.total,
      )} · Cleared {formatAmount(register.filtered_balances.cleared)} · Uncleared
      {formatAmount(register.filtered_balances.uncleared)}
    </p>{/if}
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
  {#if deliveryNotice && deliveryNotice !== notice}<p role="status">
      {deliveryNotice}
    </p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>
    <button onclick={() => userRefresh(retryCutoff)} disabled={loading || busy}
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
  {#if !viewMatchesEpoch}<p role="alert">
      {t(
        "This saved view belongs to an earlier budget restore. Refresh before using its balances. Pending work keeps its original budget generation.",
      )}
    </p>{/if}
  {#if cachedAt}<p class="hint" role="status">
      {disconnected
        ? t("Disconnected. Cached view from")
        : t("View saved on this device at")}
      {new Date(cachedAt).toLocaleString()}. {t(
        "Balances include confirmed transactions only.",
      )}
    </p>{/if}
  {#if queue.length}<section
      class="pending-action"
      data-testid="pending-expenses"
      aria-label={t("Pending expenses")}
    >
      <h3>{t("Pending expenses")}</h3>
      {#each queue as item (item.key)}<div class="pending-expense">
          <p>
            {formatAmountInput(item.row.amount, item.format)} · {formatDate(
              item.row.date,
            )} · {item.row.notes || item.row.payee_name || t("No payee")}
          </p>
          <p role="status">
            {item.blocked ||
              (item.pending.method === "DELETE"
                ? t("Deletion waiting to sync")
                : t("Saved on this device. Waiting to sync."))}
          </p>
          {#if item.offlineCreated && item.pending.method !== "DELETE"}<button
              onclick={() => void editQueued(item)}
              disabled={showForm || splitActive || busy || !!item.blocked}
              >{t("Edit pending expense")}</button
            ><button
              onclick={() => void deleteQueued(item)}
              disabled={showForm || splitActive || busy || !!item.blocked}
              >{t("Delete pending expense")}</button
            >{/if}
          <button
            onclick={() => void retryQueued(item)}
            disabled={showForm || splitActive || busy}
            >{t("Retry pending expense")}</button
          >
          {#if item.blocked}<button
              onclick={() => void retryQueued(item, true)}
              disabled={showForm || splitActive || busy}
              >{t("Cancel pending change safely")}</button
            >{/if}
        </div>{/each}
    </section>{/if}
  {#if showForm && (accounts.length || recoveredPending) && !recoveryError}
    <TransactionForm
      {budgetId}
      {accounts}
      {payees}
      {categories}
      initial={editing}
      {queued}
      {disconnected}
      defaultAccountId={accountId}
      oncomplete={(_transaction, message) => void changed(message)}
      oncancel={() => {
        showForm = false;
        editing = null;
        queued = null;
        void tick().then(() => bulkActions?.refresh());
      }}
    />
  {/if}
  <SplitEditor
    {budgetId}
    bind:this={splitEditor}
    {accounts}
    {payees}
    {categories}
    {disconnected}
    bind:active={splitActive}
    oncomplete={() => changed()}
  />
  {#if loading}<p role="status">{t("Loading transactions…")}</p>{/if}
  {#if register?.data.length && preferences.hasUsableShared && viewMatchesEpoch}
    <p class="hint">
      Use Space to select a row; Shift-click selects a range. Selection applies
      to loaded rows only. Use Ctrl+Z to undo, Ctrl+Shift+Z to redo outside text
      fields.
    </p>
    <label class="select-loaded"
      ><input
        type="checkbox"
        aria-label="Select all loaded entries"
        checked={!!register.data.length &&
          selectionRows.every((row) => selectedIds.includes(row.id))}
        disabled={disconnected ||
          loading ||
          busy ||
          bulkActive ||
          showForm ||
          splitActive ||
          selectionRows.length > 100}
        onchange={(event) => {
          selectedIds = event.currentTarget.checked
            ? selectionRows.map((row) => row.id)
            : [];
        }}
      /> Select loaded entries</label
    >
    <table class="register-table">
      <caption class="visually-hidden"
        >Transactions sorted by {confirmedQuery.sort}, {confirmedQuery.direction ===
        "asc"
          ? "ascending"
          : "descending"}</caption
      >
      <thead
        ><tr
          ><th>Selection</th>{#each appliedColumns as column (column)}<th
              class:number={[
                "amount",
                "payment",
                "deposit",
                "balance",
              ].includes(column)}>{t(columnLabels[column])}</th
            >{/each}<th>{t("Actions")}</th></tr
        ></thead
      >
      <tbody>
        {#each selectionRows as row (row.id)}
          <tr
            class:selected={selectedIds.includes(row.id)}
            class:split-child={!!row.parent_id}
          >
            <td data-label="Select"
              ><label class="row-selection"
                ><input
                  type="checkbox"
                  aria-label={`Select ${description(row)}`}
                  checked={selectedIds.includes(row.id)}
                  disabled={disconnected ||
                    loading ||
                    busy ||
                    bulkActive ||
                    showForm ||
                    splitActive}
                  onclick={(event) => selectRow(event, row)}
                /><span class="visually-hidden">Select entry</span></label
              ></td
            >
            {#each appliedColumns as column (column)}<td
                class:number={[
                  "amount",
                  "payment",
                  "deposit",
                  "balance",
                ].includes(column)}
                class:entry-notes={column === "notes"}
                class:amount={column === "amount"}
                data-label={t(columnLabels[column])}>{cell(row, column)}</td
              >{/each}
            <td class="row-actions"
              >{#if row.is_parent}<button
                  disabled={bulkActive || busy}
                  onclick={() => {
                    if (expandedSplits.includes(row.id)) {
                      expandedSplits = expandedSplits.filter(
                        (id) => id !== row.id,
                      );
                      selectedIds = selectedIds.filter(
                        (id) =>
                          !(row.children ?? []).some(
                            (child) => child.id === id,
                          ),
                      );
                    } else expandedSplits = [...expandedSplits, row.id];
                  }}
                  aria-expanded={expandedSplits.includes(row.id)}
                  >{expandedSplits.includes(row.id)
                    ? "Hide split entries"
                    : "Select split entries"}</button
                >{/if}<button
                onkeydown={navigateRow}
                onclick={() => open(row)}
                disabled={disconnected ||
                  loading ||
                  busy ||
                  showForm ||
                  splitActive ||
                  bulkActive ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared ||
                  row.reconciled}
                aria-label={`${t("Edit")} ${description(row)}`}
                >{t("Edit")}</button
              ><button
                onclick={() => {
                  const parent = row.parent_id
                    ? register?.data.find((item) => item.id === row.parent_id)
                    : row;
                  if (parent) void splitEditor?.open(parent);
                }}
                disabled={disconnected ||
                  loading ||
                  busy ||
                  showForm ||
                  splitActive ||
                  bulkActive ||
                  !!pending ||
                  !!recoveryError ||
                  row.reconciled ||
                  row.starting_balance}
                hidden={!!row.parent_id}
                >{row.is_parent
                  ? t("Edit splits")
                  : t("Split transaction")}</button
              ><button
                onkeydown={navigateRow}
                onclick={() => void runAction("duplicate", row)}
                disabled={disconnected ||
                  loading ||
                  busy ||
                  showForm ||
                  splitActive ||
                  bulkActive ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared}
                hidden={!!row.parent_id}
                aria-label={`${t("Duplicate")} ${description(row)}`}
                >{t("Duplicate")}</button
              ><button
                onkeydown={navigateRow}
                onclick={() => {
                  deleting = row;
                  deletionEpoch = preferences.budgetEpoch;
                }}
                disabled={disconnected ||
                  loading ||
                  busy ||
                  showForm ||
                  splitActive ||
                  bulkActive ||
                  !!pending ||
                  !!recoveryError ||
                  !preferences.hasUsableShared ||
                  row.reconciled}
                hidden={!!row.parent_id}
                aria-label={`${t("Delete")} ${description(row)}`}
                >{t("Delete")}</button
              ></td
            >
          </tr>
          {#if deleting?.id === row.id}<tr class="delete-confirmation"
              ><td colspan={appliedColumns.length + 2}
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
        ? !isDefaultQuery(confirmedQuery)
          ? "No transactions match these filters."
          : t("No transactions yet. Add an income or expense to begin.")
        : t("Create an account before recording transactions.")}
    </p>{/if}
</section>

<style>
  .split-child {
    background: var(--hover);
  }
  .register {
    container-type: inline-size;
  }
  tr.selected {
    background: var(--selected);
  }
  input[type="checkbox"] {
    width: 1.25rem;
    height: 1.25rem;
  }
  .select-loaded {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .row-selection {
    display: inline-flex;
    min-height: 44px;
    min-width: 44px;
    align-items: center;
    justify-content: center;
  }
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
  .pending-expense {
    overflow-wrap: anywhere;
  }
  .pending-expense + .pending-expense {
    border-top: 1px solid var(--line);
    margin-top: 1rem;
    padding-top: 1rem;
  }
  .pending-expense button {
    margin: 0 0.5rem 0.5rem 0;
    max-width: 100%;
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
  @container (max-width: 45rem) {
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
