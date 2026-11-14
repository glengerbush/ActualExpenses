<script lang="ts">
  import { onMount } from "svelte";
  import {
    request,
    list,
    uuid,
    errorMessage,
    RequestError,
    type Budget,
  } from "./api";
  import {
    readCatalog,
    saveCatalog,
    readBudget,
    refreshBudget,
    type BudgetCache,
  } from "./offline/cache";
  import { readValue, writeValue } from "./offline/storage";
  import {
    entries,
    receipts,
    enqueue,
    retryEntry,
    deleteOfflineExpense,
    type OutboxEntry,
  } from "./offline/outbox";
  import {
    startDelivery,
    deliverPending,
    subscribeOffline,
    offlineStatus,
  } from "./offline/delivery";
  import { setReachable } from "./offline/events";
  import { preferences } from "./preferences.svelte";
  import { nextEditClock, observeVersions } from "./edit-clock";
  import { formatAmount, formatDate } from "./money";
  import type { Transaction, PendingMutation } from "./register-api";
  import { readTransactionDraft } from "./register-storage";
  import TransactionForm from "./TransactionForm.svelte";
  interface View {
    epoch: string;
    snapshot: string;
    recent: Transaction[];
    retained: Transaction[];
    unavailable: string[];
  }
  const selectionKey = "actualExpenses.iphone.budget",
    viewKey = (id: string) => `expense-view:${id}`,
    retainedKey = (id: string) => `expense-retained:${id}`;
  let budgets = $state.raw<Budget[]>([]),
    budget = $state.raw<Budget | null>(null);
  let references = $state.raw<BudgetCache | null>(null),
    view = $state.raw<View | null>(null),
    queue = $state.raw<OutboxEntry[]>([]);
  let retained = $state(""),
    busy = $state(true),
    disconnected = $state(!offlineStatus.reachable),
    compatible = $state(true);
  let error = $state(""),
    notice = $state(""),
    editor = $state(false),
    recovery = $state(false);
  let editing = $state.raw<Transaction | null>(null),
    queued = $state.raw<OutboxEntry | null>(null),
    deleting = $state.raw<Transaction | null>(null);
  let alive = true,
    generation = 0,
    queueGeneration = 0,
    lastReceipt = "";
  let connectionError = "";
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  const activeQueue = $derived(
    queue.filter((e) => e.pending.budgetEpoch === references?.epoch),
  );
  const localRows = $derived(new Map(activeQueue.map((e) => [e.row.id, e])));
  const rows = $derived.by(() => {
    const confirmed =
      view?.epoch === references?.epoch && view
        ? [...view.recent, ...view.retained]
        : [];
    const merged = new Map(confirmed.map((row) => [row.id, row]));
    for (const entry of activeQueue) {
      if (entry.pending.method === "DELETE") merged.delete(entry.row.id);
      else merged.set(entry.row.id, entry.row);
    }
    const sorted = [...merged.values()]
      .filter(
        (r) =>
          !r.deleted &&
          r.amount < 0 &&
          !r.transfer_id &&
          !r.is_parent &&
          !r.parent_id,
      )
      .sort(
        (a, b) =>
          b.date.localeCompare(a.date) ||
          Number(localRows.has(b.id)) - Number(localRows.has(a.id)),
      );
    const recent = sorted.slice(0, 10);
    return [
      ...recent,
      ...sorted.filter(
        (r) =>
          !recent.includes(r) && (r.id === retained || localRows.has(r.id)),
      ),
    ];
  });
  const problems = $derived(
    queue.filter(
      (e) => e.blocked || e.pending.budgetEpoch !== references?.epoch,
    ),
  );
  function flash(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 4000);
  }
  function install(cache: BudgetCache) {
    references = cache;
    preferences.useCachedBudget(
      cache.budgetId,
      cache.preferences,
      cache.savedAt,
      cache.epoch,
    );
  }
  async function localChanges() {
    const id = budget?.id,
      run = ++queueGeneration;
    if (!id) return;
    try {
      const [pending, completed] = await Promise.all([
        entries(id),
        receipts(id),
      ]);
      if (!alive || run !== queueGeneration || budget?.id !== id) return;
      queue = pending;
      disconnected = !offlineStatus.reachable;
      const latest = completed.at(-1);
      if (latest && latest.key !== lastReceipt) {
        lastReceipt = latest.key;
        if (latest.message) flash(latest.message);
        if (!busy && !editor && !disconnected) void sync();
      }
    } catch (e) {
      if (alive) error = errorMessage(e);
    }
  }
  async function choose(next: Budget) {
    ++generation;
    budget = next;
    references = null;
    view = null;
    queue = [];
    error = "";
    editor = false;
    deleting = null;
    recovery = false;
    try {
      await writeValue(selectionKey, next.id);
      const [cache, savedView, savedRetention, pending, completed] =
        await Promise.all([
          readBudget(next.id),
          readValue<View>(viewKey(next.id)),
          readValue<string>(retainedKey(next.id)),
          entries(next.id),
          receipts(next.id),
        ]);
      if (!alive || budget?.id !== next.id) return;
      if (cache) install(cache);
      view = savedView ?? null;
      retained = savedRetention ?? "";
      queue = pending;
      lastReceipt = completed.at(-1)?.key ?? "";
      recovery = !!readTransactionDraft(next.id, true);
    } catch (e) {
      error = errorMessage(e);
    }
    await sync();
  }
  async function sync() {
    const id = budget?.id,
      run = ++generation;
    busy = true;
    try {
      if (!navigator.onLine) {
        setReachable(false);
        return;
      }
      const { data: info } = await request<{
        data: { api_version: number; capabilities: string[] };
      }>("/info");
      if (!alive || run !== generation) return;
      compatible =
        info.api_version === 1 &&
        [
          "companion_expenses",
          "expense_kind_precondition",
          "unified_ledger",
        ].every((c) => info.capabilities?.includes(c));
      if (!compatible)
        throw Error(
          "Update actualBudget to use expense entry. Saved work remains on this device.",
        );
      setReachable(true);
      if (error === connectionError) error = "";
      connectionError = "";
      if (!id) {
        const next = await list<Budget>("/budgets");
        if (!alive || run !== generation) return;
        budgets = next;
        await saveCatalog(next);
      } else {
        await deliverPending();
        const cache = await refreshBudget(id),
          pending = await entries(id);
        const keep = [
          ...new Set(
            [retained, ...pending.map((e) => e.row.id)].filter(Boolean),
          ),
        ];
        if (keep.length > 100)
          throw Error("Sync saved expenses before adding more.");
        const { data } = await request<{ data: View }>(
          `/budgets/${id}/companion?${new URLSearchParams({ retain: keep.join(","), snapshot: cache.snapshot })}`,
        );
        if (!alive || run !== generation || budget?.id !== id) return;
        if (data.epoch !== cache.epoch)
          throw Error("This budget was restored. Refresh before editing.");
        install(cache);
        view = data;
        queue = pending;
        for (const row of [...data.recent, ...data.retained])
          observeVersions(row.field_versions);
        await writeValue(viewKey(id), data);
        lastReceipt = (await receipts(id)).at(-1)?.key ?? "";
      }
    } catch (e) {
      if (alive && run === generation) {
        if (e instanceof RequestError && !e.code) {
          setReachable(false);
          connectionError = errorMessage(e);
          if (!references) error = connectionError;
        } else {
          if (
            e instanceof RequestError &&
            ["not_found", "budget_restored"].includes(e.code ?? "")
          )
            compatible = false;
          error = errorMessage(e);
        }
      }
    } finally {
      if (alive && run === generation) {
        busy = false;
        disconnected = !offlineStatus.reachable;
      }
    }
  }
  async function initialize() {
    try {
      const [catalog, selected] = await Promise.all([
        readCatalog(),
        readValue<string>(selectionKey),
      ]);
      if (!alive) return;
      budgets = catalog?.budgets ?? [];
      const existing = budgets.find((b) => b.id === selected);
      if (existing) await choose(existing);
      else await sync();
    } catch (e) {
      error = errorMessage(e);
      busy = false;
    }
  }
  async function switchBudget() {
    ++generation;
    ++queueGeneration;
    budget = null;
    references = null;
    view = null;
    queue = [];
    editor = false;
    deleting = null;
    error = "";
    try {
      await writeValue(selectionKey, null);
      await sync();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function editable(row: Transaction) {
    if (!compatible || busy) return false;
    const local = localRows.get(row.id);
    return local
      ? local.draft?.companion === true &&
          local.offlineCreated &&
          !local.attempted &&
          !local.claim &&
          !local.blocked
      : !disconnected;
  }
  function open(row: Transaction | null = null) {
    if (row && !editable(row)) return;
    editing = row;
    queued = row ? (localRows.get(row.id) ?? null) : null;
    editor = true;
    deleting = null;
  }
  async function captured(row: Transaction, created: boolean) {
    if (created && budget) {
      await writeValue(retainedKey(budget.id), row.id);
      retained = row.id;
    }
  }
  async function completed(_row: Transaction | null, message?: string) {
    editor = false;
    editing = null;
    queued = null;
    recovery = false;
    flash(message || "Expense saved.");
    await localChanges();
    if (!disconnected) await sync();
  }
  async function remove() {
    if (!budget || !deleting || !editable(deleting)) return;
    const row = deleting,
      local = localRows.get(row.id);
    busy = true;
    try {
      const pending: PendingMutation = {
        path: `/budgets/${budget.id}/transactions/${row.id}`,
        method: "DELETE",
        key: uuid(),
        edit_clock: nextEditClock(row.field_versions),
        budgetEpoch: references!.epoch,
        expenseOnly: true,
      };
      if (local) await deleteOfflineExpense(local, pending);
      else
        await enqueue({
          budgetId: budget.id,
          pending,
          row,
          draft: null,
          offlineCreated: false,
        });
      deleting = null;
      await deliverPending();
      const receipt = (await receipts(budget.id)).find(
        (r) => r.key === pending.key,
      );
      const outstanding = (await entries(budget.id)).find(
        (e) => e.key === pending.key,
      );
      if (!outstanding?.blocked)
        flash(
          receipt?.message ||
            (outstanding ? "Change saved." : "Expense deleted."),
        );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
      await localChanges();
      if (!disconnected) await sync();
    }
  }
  async function retry(entry: OutboxEntry, cancel = false) {
    try {
      await retryEntry(entry, cancel);
      await sync();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function foreground() {
    if (document.visibilityState === "visible" && !editor && !busy) void sync();
  }
  onMount(() => {
    const stopPreferences = preferences.startDevice(),
      stopDelivery = startDelivery();
    const stop = subscribeOffline(() => {
      disconnected = !offlineStatus.reachable;
      void localChanges();
    });
    void initialize();
    return () => {
      alive = false;
      ++generation;
      clearTimeout(noticeTimer);
      stop();
      stopDelivery();
      stopPreferences();
    };
  });
</script>

<svelte:window
  ononline={() => {
    if (!busy && !editor) void sync();
  }}
/>
<svelte:document onvisibilitychange={foreground} />
<main class="expense-app" id="main">
  <header>
    <div>
      <h1>{budget ? budget.name : "actualExpenses"}</h1>
      {#if budget}<p>Recent expenses</p>{/if}
    </div>
    <div class="header-actions">
      {#if budget}<button disabled={editor || busy} onclick={switchBudget}
          >Budgets</button
        >{/if}
      <button
        aria-label="Refresh expenses"
        title="Refresh expenses"
        disabled={busy || editor}
        onclick={() => sync()}
        ><svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          aria-hidden="true"
          ><path d="M20 7v5h-5M20 12a8 8 0 1 0-2.3 5.7" /></svg
        ></button
      >
    </div>
  </header>
  {#if error}<div class="failure" role="alert">
      <span>{error}</span><button
        aria-label="Dismiss error"
        onclick={() => (error = "")}>Dismiss</button
      >
    </div>{/if}
  {#if notice}<div class="toast" role="status">{notice}</div>{/if}
  {#if !budget}
    <h2>Choose a budget</h2>
    {#if busy && !budgets.length}<p role="status">
        Loading budgets…
      </p>{:else if !budgets.length}<p>
        No budgets yet. Create one in the full web app.
      </p>{/if}
    <div class="budget-list">
      {#each budgets as item (item.id)}<button
          disabled={busy || !compatible}
          onclick={() => choose(item)}
          >{item.name}<svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"><path d="m9 5 7 7-7 7" /></svg
          ></button
        >{/each}
    </div>
  {:else if references}
    {#if problems.length}<section
        class="saved-issues"
        aria-label="Saved expense issues"
      >
        {#each problems as entry (entry.key)}<p role="alert">
            {entry.blocked ||
              "This saved expense belongs to an earlier budget version."}
          </p>
          <button disabled={busy || disconnected} onclick={() => retry(entry)}
            >Retry saved expense</button
          ><button
            disabled={busy || disconnected}
            onclick={() => retry(entry, true)}
            >Cancel saved expense safely</button
          >{/each}
      </section>{/if}
    {#if editor}<section
        class="expense-editor"
        aria-label={editing ? "Edit expense" : "Add expense"}
      >
        <TransactionForm
          expenseOnly
          budgetId={budget.id}
          accounts={references.accounts}
          payees={references.payees.filter((p) => !p.transfer_account_id)}
          categories={references.categories.filter((c) => !c.is_income)}
          defaultAccountId={references.ledgerAccountId}
          initial={editing}
          {queued}
          {disconnected}
          oncaptured={captured}
          oncomplete={completed}
          oncancel={() => {
            editor = false;
            editing = null;
            queued = null;
            recovery = !!readTransactionDraft(budget!.id, true);
          }}
        />
      </section>{:else}
      <button
        class="primary add-expense"
        disabled={!compatible || !references.ledgerAccountId}
        onclick={() => open()}
        >{recovery ? "Resume expense" : "+ Add expense"}</button
      >
      {#if !rows.length}<div class="empty">
          <h2>No expenses yet</h2>
          <p>Add your first purchase here.</p>
        </div>{/if}
      <ul class="expense-list" aria-label="Recent expenses">
        {#each rows as row (row.id)}<li data-expense-id={row.id}>
            <button
              class="expense-row"
              disabled={!editable(row)}
              onclick={() => open(row)}
              aria-label={`Edit ${row.payee_name || "expense"} ${formatAmount(row.amount)}`}
              ><span class="details"
                ><strong>{row.payee_name || "No payee"}</strong><span
                  ><span
                    class="category-dot"
                    style:background={row.category_color || "var(--muted)"}
                  ></span>{row.category_name || "Uncategorized"} ·
                  <time datetime={row.date}>{formatDate(row.date)}</time></span
                ></span
              ><strong class="amount">{formatAmount(row.amount)}</strong
              ></button
            >
            <button
              class="delete"
              aria-label={`Delete ${row.payee_name || "expense"} ${formatAmount(row.amount)}`}
              disabled={!editable(row)}
              onclick={() => (deleting = row)}
              ><svg
                width="19"
                height="19"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                aria-hidden="true"
                ><path
                  d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 10v8m4-8v8"
                /></svg
              ></button
            >
            {#if deleting?.id === row.id}<div class="delete-confirm">
                <span>Delete this expense?</span><button
                  class="danger"
                  disabled={busy}
                  onclick={remove}>Delete expense</button
                ><button disabled={busy} onclick={() => (deleting = null)}
                  >Keep</button
                >
              </div>{/if}
          </li>{/each}
      </ul>
    {/if}
  {:else}<p role="status">
      {busy
        ? "Loading expenses…"
        : "Reconnect and refresh to load this budget."}
    </p>{/if}
</main>

<style>
  .expense-app {
    max-width: 38rem;
    margin: 0 auto;
    padding: max(1rem, env(safe-area-inset-top))
      max(1rem, env(safe-area-inset-right))
      max(2rem, env(safe-area-inset-bottom))
      max(1rem, env(safe-area-inset-left));
    min-height: 100dvh;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding-bottom: 1.25rem;
  }
  h1 {
    font-size: 1.35rem;
    line-height: 1.2;
    margin: 0;
    overflow-wrap: anywhere;
  }
  header p {
    color: var(--muted);
    margin: 0.3rem 0 0;
    font-size: 0.9rem;
  }
  .header-actions {
    display: flex;
    gap: 0.35rem;
    flex-shrink: 0;
  }
  button {
    min-height: 44px;
  }
  .header-actions button {
    padding: 0.6rem;
  }
  h2 {
    font-size: 1.15rem;
    margin: 1rem 0;
  }
  .budget-list {
    display: grid;
  }
  .budget-list button {
    display: flex;
    justify-content: space-between;
    align-items: center;
    text-align: left;
    padding: 1rem;
    border-radius: 0;
    border-width: 0 0 1px;
  }
  .add-expense {
    width: 100%;
    margin: 0.25rem 0 1.25rem;
    min-height: 48px;
    font-size: 1rem;
  }
  .expense-list {
    padding: 0;
    margin: 0;
    list-style: none;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    border-bottom: 1px solid var(--line);
  }
  .expense-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    flex: 1;
    min-width: 0;
    border: 0;
    border-radius: 0;
    text-align: left;
    padding: 1rem 0;
    background: transparent;
    color: var(--text);
  }
  .expense-row:disabled {
    opacity: 1;
    cursor: default;
  }
  .details {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
    flex: 1 1 12rem;
  }
  .details strong {
    overflow-wrap: anywhere;
  }
  .details > span {
    font-weight: 400;
    color: var(--muted);
    font-size: 0.82rem;
    overflow-wrap: anywhere;
  }
  .category-dot {
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    margin-right: 0.35rem;
  }
  time {
    white-space: nowrap;
  }
  .amount {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .delete {
    border: 0;
    background: transparent;
    padding: 0.75rem 0.25rem 0.75rem 0.7rem;
    color: var(--muted);
    min-width: 44px;
  }
  .delete-confirm {
    width: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    padding: 0 0 1rem;
  }
  .delete-confirm span {
    flex: 1;
  }
  .failure {
    border: 1px solid var(--error);
    color: var(--error);
    padding: 0.75rem;
    margin-bottom: 1rem;
    display: flex;
    gap: 0.5rem;
    align-items: start;
    overflow-wrap: anywhere;
  }
  .failure span {
    flex: 1;
  }
  .toast {
    position: fixed;
    z-index: 20;
    bottom: max(1rem, env(safe-area-inset-bottom));
    left: 1rem;
    right: 1rem;
    max-width: 36rem;
    margin: auto;
    background: var(--accent);
    color: var(--primary-text);
    padding: 0.85rem 1rem;
    border-radius: 0.5rem;
  }
  .empty {
    padding: 1.5rem 0;
  }
  .empty p {
    color: var(--muted);
  }
  .expense-editor {
    padding-top: 0.25rem;
  }
  .expense-editor :global(.entry-pair) {
    display: block;
  }
  .expense-editor :global(.entry-pair > div) {
    margin-bottom: 1rem;
  }
  .expense-editor :global(input),
  .expense-editor :global(select) {
    font-size: 1rem;
    min-height: 44px;
  }
  .saved-issues {
    margin-bottom: 1rem;
  }
  @media (max-width: 350px) {
    header {
      flex-wrap: wrap;
    }
    .expense-row {
      gap: 0.4rem;
    }
  }
</style>
