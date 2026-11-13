<script lang="ts">
  import RefreshIcon from "./RefreshIcon.svelte";
  import { outsideClick } from "./outside-click";
  import Categories from "./Categories.svelte";
  import type { CategoryLaunch } from "./category-colors";
  import CategoryColor from "./CategoryColor.svelte";
  import { t } from "./i18n.svelte";
  import { onMount, onDestroy, tick } from "svelte";
  import { SvelteDate } from "svelte/reactivity";
  import { request, uuid, errorMessage } from "./api";
  import {
    mutate,
    requestCancellation,
    type Transaction,
  } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import { preferences } from "./preferences.svelte";
  import {
    formatAmount,
    formatAmountInput,
    parseAmount,
    captureAmountFormat,
    today,
  } from "./money";
  import { calculateAmount } from "./calculator";
  import {
    planningKey,
    readPlanningDraft,
    type PlanningDraft,
    type PlanView,
    type PlanMonth,
    type PlanCategory,
  } from "./planning";
  let {
    budgetId,
    disconnected,
    onclose,
    onchanged,
    onaccount,
  }: {
    budgetId: string;
    disconnected: boolean;
    onclose: () => void;
    onchanged: (method: "tracking") => Promise<void>;
    onaccount: (id: string) => void;
  } = $props();
  let view = $state.raw<PlanView | null>(null),
    draft = $state<PlanningDraft | null>(null);
  let month = $state(today().slice(0, 7)),
    count = $state(3),
    busy = $state(false),
    loading = $state(true),
    error = $state(""),
    notice = $state(""),
    storageBroken = $state(false);
  type Group = {
    id: string;
    name: string;
    resolved_color: string;
    is_income: boolean;
    sort_order: number | null;
  };
  let categoryGroups = $state.raw<Group[]>([]);
  let categoryEditor = $state<CategoryLaunch | null>(null);
  async function loadGroups() {
    const groups: Group[] = [];
    let offset: number | null = 0;
    do {
      const result: { data: Group[]; next_offset: number | null } =
        await request(
          `/budgets/${budgetId}/category-groups?limit=1000&offset=${offset}`,
        );
      groups.push(...result.data);
      offset = result.next_offset;
    } while (offset !== null);
    categoryGroups = groups;
  }
  let collapsedGroups = $state<string[]>([]);
  function grouped(categories: PlanCategory[]) {
    const result: {
      id: string;
      name: string;
      color: string;
      order: number;
      income: boolean;
      categories: PlanCategory[];
    }[] = categoryGroups.map((g) => ({
      id: g.id,
      name: g.name,
      color: g.resolved_color,
      order: g.sort_order ?? 0,
      income: g.is_income,
      categories: [],
    }));
    for (const category of categories) {
      const id =
        category.group_id ??
        (category.is_income ? "ungrouped-income" : "ungrouped-expense");
      let group = result.find((g) => g.id === id);
      if (!group) {
        group = {
          id,
          name:
            category.group_name ||
            (category.is_income ? "Income" : "Ungrouped"),
          color: category.category_group_color ?? "#397C8C",
          order: category.group_id
            ? (category.group_sort_order ?? 0)
            : Infinity,
          income: category.is_income,
          categories: [],
        };
        result.push(group);
      }
      group.categories.push(category);
    }
    return result.sort(
      (a, b) =>
        Number(b.income) - Number(a.income) ||
        a.order - b.order ||
        a.name.localeCompare(b.name),
    );
  }
  function groupTotal(
    categories: PlanCategory[],
    field: "budgeted" | "actual" | "balance",
  ) {
    const total = categories.reduce((sum, c) => sum + BigInt(c[field]), 0n);
    return total > BigInt(Number.MAX_SAFE_INTEGER) ||
      total < BigInt(Number.MIN_SAFE_INTEGER)
      ? "—"
      : formatAmount(Number(total));
  }
  let history = $state<{
    undo: { id: string; label: string } | null;
    redo: { id: string; label: string } | null;
  }>({ undo: null, redo: null });
  let detail = $state<{
    category: PlanCategory;
    month: string;
    offset: number;
    data: Transaction[];
    total_count: number;
    actual_total: number;
    snapshot: string;
  } | null>(null);
  let checkpoint: string | null = null;
  let openedRowDraft: string | null = null;
  let quickTimer: ReturnType<typeof setTimeout> | undefined;
  let lastInputAt = 0;
  let flushOnIdle = false;
  onDestroy(() => clearTimeout(quickTimer));
  function isQuick(m: PlanMonth, c: PlanCategory) {
    return !!draft?.quick && draft.month === m.month && draft.category === c.id;
  }
  function scheduleQuick() {
    clearTimeout(quickTimer);
    quickTimer = setTimeout(
      () => {
        void flushQuick();
      },
      Math.max(0, 1000 - (Date.now() - lastInputAt)),
    );
  }
  function quickInput(m: PlanMonth, c: PlanCategory, amount: string) {
    if ((!draft && busy) || storageBroken || disconnected) return;
    if (!draft) {
      open("set", m, c);
      const created = draft as PlanningDraft | null;
      if (!created) return;
      created.quick = true;
    }
    if (!isQuick(m, c)) return;
    try {
      save({ ...$state.snapshot(draft!), amount, quickClock: nextEditClock() });
      error = "";
      notice = "";
      lastInputAt = Date.now();
      flushOnIdle = false;
      scheduleQuick();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function flushQuick(blur = false) {
    clearTimeout(quickTimer);
    if (!draft?.quick) return;
    if (busy) {
      if (blur) flushOnIdle = true;
      return;
    }
    if (!draft.pending) await send();
  }
  const amounts = ["set"];
  const categoryActions = ["set", "carryover", "copy_year"];
  const labels: Record<string, string> = {
    set: "Edit allocation",
    carryover: "Carryover setting",
    copy_previous: "Copy previous month",
    copy_year: "Copy through year end",
    undo: "Undo",
    redo: "Redo",
  };
  let current = $derived(view?.months[month]);
  function save(next: PlanningDraft | null) {
    if (localStorage.getItem(planningKey(budgetId)) !== checkpoint)
      throw new Error(
        "Another tab changed this budget draft. Reload to review its saved work.",
      );
    const raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(planningKey(budgetId));
    else localStorage.setItem(planningKey(budgetId), raw);
    checkpoint = raw;
    draft = next;
    if (!next) openedRowDraft = null;
  }
  function field<K extends keyof PlanningDraft>(
    key: K,
    value: PlanningDraft[K],
  ) {
    if (!draft || draft.pending || busy) return;
    const next = { ...$state.snapshot(draft), [key]: value };
    try {
      save(next);
      error = "";
    } catch (e) {
      draft = next;
      error = errorMessage(e);
    }
  }
  async function refresh() {
    if (disconnected) {
      loading = false;
      error =
        "Reconnect to load confirmed budget figures. Saved requests stay on this device.";
      return;
    }
    loading = true;
    error = "";
    try {
      const result = await request<{ data: PlanView }>(
        `/budgets/${budgetId}/budget?month=${month}&count=${count}`,
      );
      view = result.data;
      await loadGroups();
      observeVersions({ budget: view.edit_clock });
      const owner = await historyOwner();
      history = (
        await request<{ data: typeof history }>(
          `/budgets/${budgetId}/history`,
          undefined,
          undefined,
          "GET",
          undefined,
          false,
          undefined,
          owner,
        )
      ).data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    try {
      draft = readPlanningDraft(budgetId);
      const savedCategory = localStorage.getItem(
        `actualBudget.categories.v1.${budgetId}`,
      );
      if (savedCategory) {
        const saved = JSON.parse(savedCategory);
        if (saved.draft || saved.pending)
          categoryEditor = { kind: saved.draft?.kind ?? "categories" };
      }
      checkpoint = localStorage.getItem(planningKey(budgetId));
      if (draft) month = draft.month;
    } catch (e) {
      storageBroken = true;
      error = errorMessage(e);
      loading = false;
      return;
    }
    void refresh();
  });
  const monthNames = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  function shortMonth(value: string) {
    return `${monthNames[Number(value.slice(5)) - 1]}-${value.slice(2, 4)}`;
  }
  function chooseMonth(input: HTMLInputElement) {
    const match = /^([a-z]{3})-(\d{2})$/i.exec(input.value.trim());
    const index = match
      ? monthNames.findIndex((m) => m.toLowerCase() === match[1].toLowerCase())
      : -1;
    if (!match || index < 0) {
      input.value = shortMonth(month);
      error = "Enter a month like Nov-26.";
      return;
    }
    month = `20${match[2]}-${String(index + 1).padStart(2, "0")}`;
    input.value = shortMonth(month);
    void refresh();
  }
  function shift(delta: number) {
    const date = new SvelteDate(month + "-01T12:00:00Z");
    date.setUTCMonth(date.getUTCMonth() + delta);
    month = date.toISOString().slice(0, 7);
    void refresh();
  }
  function open(action: string, m: PlanMonth, category?: PlanCategory) {
    if (!view || busy || storageBroken) return;
    if (draft) {
      error =
        "Finish or discard the saved plan before starting another action.";
      return;
    }
    try {
      save({
        version: 1,
        epoch: view.epoch,
        month: m.month,
        action,
        category: category?.id ?? "",
        amount: formatAmountInput(category?.budgeted ?? 0),
        method: view.method,
        enabled: !(category?.carryover ?? false),
        format: captureAmountFormat(),
        pending: null,
      });
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function calculate() {
    if (!draft) return;
    busy = true;
    try {
      const n = await calculateAmount(draft.amount, draft.format);
      save({
        ...$state.snapshot(draft),
        amount: formatAmountInput(n, draft.format),
      });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function discard() {
    if (!draft || draft.pending || busy) return;
    try {
      save(null);
      error = "";
      notice = "Budget draft discarded.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function send(cancel = false) {
    if (!draft || busy || storageBroken) return;
    let followup = false;
    busy = true;
    error = "";
    notice = "";
    try {
      if (disconnected)
        throw new Error(
          "Reconnect before changing the budget. Your draft is saved.",
        );
      if (!draft.pending) {
        if (!view || view.epoch !== draft.epoch)
          throw new Error(
            "This budget was restored. Keep any needed values, discard this draft and reopen the current plan.",
          );
        if (draft.method !== "tracking" || !labels[draft.action])
          throw new Error(
            "This saved action is no longer supported. Discard it and open the current plan.",
          );
        let body: Record<string, unknown> = {
          snapshot: view.snapshot,
          action: draft.action,
          month: draft.month,
        };
        if (
          categoryActions.includes(draft.action) ||
          (draft.action === "copy_previous" && draft.category)
        )
          body.category = draft.category;
        if (amounts.includes(draft.action))
          body.amount = parseAmount(draft.amount, draft.format);
        if (draft.action === "carryover") body.enabled = draft.enabled;
        let path = `/budgets/${budgetId}/budget`;
        if (draft.action === "undo" || draft.action === "redo") {
          const available = history[draft.action];
          if (!available)
            throw new Error("Refresh the available history action.");
          body = { action_id: available.id };
          path = `/budgets/${budgetId}/history/${draft.action}`;
        }
        const editClock = draft.quickClock ?? nextEditClock();
        const pending = {
          path,
          method: "POST" as const,
          body,
          key: uuid(),
          historyOwner: await historyOwner(),
          edit_clock: editClock,
          budgetEpoch: draft.epoch,
        };
        save({ ...$state.snapshot(draft), pending });
      }
      if (cancel) requestCancellation(draft.pending!);
      save($state.snapshot(draft)); // CAS and durable storage must succeed before every dispatch.
      const submitted = draft.pending!;
      try {
        await mutate(submitted, cancel);
      } catch (e) {
        if (
          e instanceof Error &&
          "code" in e &&
          e.code === "operation_canceled"
        ) {
          resetCorrectedClock();
          save({ ...$state.snapshot(draft), pending: null });
          notice =
            "The saved request was canceled. Review the retained fields before submitting a new plan.";
          return;
        }
        throw e;
      }
      const quick = draft.quick;
      if (!quick) save(null);
      detail = null;
      notice = cancel
        ? "The original request was already saved."
        : "Budget change saved.";
      await refresh();
      if (view) await onchanged(view.method);
      if (quick && draft) {
        try {
          followup =
            parseAmount(draft.amount, draft.format) !==
            (submitted.body as Record<string, unknown>).amount;
        } catch {
          followup = true;
        }
        if (followup) save({ ...$state.snapshot(draft), pending: null });
        else save(null);
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
      if (followup && draft?.quick && !draft.pending && !error) {
        if (flushOnIdle) {
          flushOnIdle = false;
          void flushQuick();
        } else scheduleQuick();
      }
    }
  }
  async function showDetails(m: PlanMonth, c: PlanCategory, offset = 0) {
    if (!view || busy) return;
    busy = true;
    error = "";
    try {
      const r = await request<{
        data: Transaction[];
        total_count: number;
        actual_total: number;
        snapshot: string;
      }>(
        `/budgets/${budgetId}/budget/transactions?month=${m.month}&category=${c.id}&offset=${offset}&limit=100&snapshot=${encodeURIComponent(detail?.category.id === c.id && detail.month === m.month ? detail.snapshot : view.snapshot)}`,
      );
      detail = { ...r, category: c, month: m.month, offset };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function historyAction(direction: "undo" | "redo") {
    if (current) open(direction, current);
  }
  function title(m: string) {
    return new Intl.DateTimeFormat("en-US", {
      month: "long",
      year: "numeric",
      timeZone: "UTC",
    }).format(new Date(m + "-01T12:00:00Z"));
  }
  function switchCategoryAction(action: string) {
    if (!draft || draft.pending || busy || !view) return;
    const m = view.months[draft.month];
    const c = m?.categories.find((c) => c.id === draft?.category);
    if (!m || !c) return;
    save({
      ...$state.snapshot(draft),
      action,
    });
  }
  function editRow(m: PlanMonth, c: PlanCategory) {
    if (busy || disconnected || categoryEditor || draft) return;
    open("set", m, c);
    openedRowDraft = draft ? JSON.stringify($state.snapshot(draft)) : null;
    void tick().then(() => document.getElementById("plan-amount")?.focus());
  }
</script>

{#snippet planEditor()}{#if draft && view}
    <section
      class="plan-form"
      aria-labelledby="plan-action-title"
      {@attach outsideClick(() => {
        if (
          draft &&
          !draft.pending &&
          !busy &&
          !storageBroken &&
          openedRowDraft !== null &&
          JSON.stringify($state.snapshot(draft)) === openedRowDraft
        ) {
          try {
            save(null);
            error = "";
            notice = "";
          } catch (e) {
            error = errorMessage(e);
          }
        }
      })}
    >
      <h2 id="plan-action-title">
        {labels[draft.action] ?? "Retired saved action"}
      </h2>
      <p>{title(draft.month)}</p>
      {#if draft.category}<div class="editor-tools">
          {#each [["set", "Allocation"], ["copy_year", "Copy year"], ["copy_previous", "Copy previous"], ["carryover", "Carryover"]] as [action, label] (action)}
            <button
              type="button"
              disabled={busy || !!draft.pending || disconnected}
              onclick={() => switchCategoryAction(action)}>{label}</button
            >
          {/each}
          <button
            type="button"
            disabled={busy || !!draft.pending || disconnected}
            onclick={() => {
              if (!draft) return;
              const selected = draft.category;
              discard();
              categoryEditor = { kind: "categories", id: selected };
            }}>Category settings</button
          >
          <button
            type="button"
            disabled={busy || disconnected}
            onclick={() => {
              if (!draft) return;
              const m = view?.months[draft.month];
              const c = m?.categories.find((c) => c.id === draft?.category);
              if (m && c) void showDetails(m, c);
            }}>Transactions</button
          >
        </div>{/if}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void send();
        }}
        aria-busy={busy}
      >
        <fieldset disabled={busy || !!draft.pending}>
          {#if draft.category}<p>
              {view.months[draft.month]?.categories.find(
                (c) => c.id === draft!.category,
              )?.name ?? "Saved category"}
            </p>{/if}
          {#if amounts.includes(draft.action)}<label for="plan-amount"
              >{t("Amount")}</label
            >
            <div class="amount-entry">
              <input
                id="plan-amount"
                inputmode="decimal"
                maxlength="256"
                value={draft.amount}
                oninput={(e) => field("amount", e.currentTarget.value)}
              /><button
                type="button"
                onclick={() => void calculate()}
                disabled={disconnected}>Calculate</button
              >
            </div>
            <p class="hint">
              Amounts use the format captured when this draft opened. Positive
              allocations add to the plan.
            </p>{/if}
          {#if draft.action === "carryover"}<label class="check"
              ><input
                type="checkbox"
                checked={draft.enabled}
                onchange={(e) => field("enabled", e.currentTarget.checked)}
              />Carry this category’s remaining plan to the following month</label
            >
            <p class="hint">
              Applies from this month through the current planning horizon.
            </p>{/if}
          {#if draft.action === "copy_year"}<p>
              Copy this allocation to later available months of the same year.
            </p>{/if}
          {#if draft.action === "copy_previous"}<p>
              Copy category allocations from the previous month. Carryover stays
              separate.
            </p>{/if}
        </fieldset>
        {#if draft.pending}<p role="status">
            The outcome is unconfirmed. Retry the original saved request.
          </p>
          <button
            type="button"
            onclick={() => void send()}
            disabled={busy || disconnected}>Retry saved budget request</button
          ><button
            type="button"
            onclick={() => void send(true)}
            disabled={busy || disconnected}
            >Cancel saved budget request safely</button
          >{:else}<div class="actions">
            <button
              class="primary"
              type="submit"
              disabled={busy || disconnected}>Save plan</button
            ><button type="button" onclick={discard} disabled={busy}
              >Discard budget draft</button
            >
          </div>{/if}
      </form>
    </section>
  {/if}{/snippet}

<section aria-labelledby="planning-title">
  <div class="page-heading">
    <div>
      <h1 id="planning-title">{t("Budget")}</h1>
      <p class="muted">Compare planned income and expenses with actuals.</p>
    </div>
    <button onclick={onclose} disabled={busy}>Close budget view</button>
  </div>
  {#if categoryEditor}<section
      class="inline-category-editor"
      aria-label="Category and group editor"
    >
      <button
        class="close-category-editor"
        onclick={() => (categoryEditor = null)}>Close editor</button
      >
      <Categories
        {budgetId}
        launch={categoryEditor}
        onclose={() => (categoryEditor = null)}
        onchanged={async () => {
          await refresh();
          if (view) await onchanged(view.method);
        }}
      />
    </section>{/if}
  <div class="month-controls">
    <div class="month-picker">
      <button
        class="month-arrow"
        aria-label="Previous month"
        title="Previous month"
        onclick={() => shift(-1)}
        disabled={busy || loading || !!draft}
      >
        <svg
          aria-hidden="true"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"><path d="m14 5-7 7 7 7" /></svg
        >
      </button>
      <div class="month-input">
        <input
          aria-label="Budget month"
          value={shortMonth(month)}
          placeholder="Nov-26"
          maxlength="7"
          disabled={busy || loading || !!draft}
          onchange={(e) => chooseMonth(e.currentTarget)}
          onkeydown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
        />
        <button
          class="month-arrow"
          title="This month"
          aria-label="This month"
          disabled={busy || loading || !!draft}
          onclick={() => {
            month = today().slice(0, 7);
            void refresh();
          }}
        >
          <svg
            aria-hidden="true"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
            ><path d="m9 4-5 5 5 5M4 9h10a6 6 0 0 1 0 12" /></svg
          >
        </button>
      </div>
      <button
        class="month-arrow"
        aria-label="Next month"
        title="Next month"
        onclick={() => shift(1)}
        disabled={busy || loading || !!draft}
      >
        <svg
          aria-hidden="true"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"><path d="m10 5 7 7-7 7" /></svg
        >
      </button>
    </div>
    <label class="months-count"
      ><span class="visually-hidden">Months displayed</span><select
        aria-label="Months displayed"
        bind:value={count}
        onchange={() => void refresh()}
        disabled={busy || loading || !!draft}
      >
        {#each [1, 2, 3, 6, 12] as n (n)}<option value={n}
            >{n} {n === 1 ? "month" : "months"}</option
          >{/each}
      </select></label
    >
    <div class="plan-tools" role="group" aria-label="Budget actions">
      <button
        class="icon-button"
        onclick={() => historyAction("undo")}
        disabled={!current ||
          !history.undo ||
          !!draft ||
          busy ||
          loading ||
          disconnected}
        aria-label={t("Undo")}
        title={history.undo ? `${t("Undo")}: ${history.undo.label}` : t("Undo")}
        ><svg
          aria-hidden="true"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"
          ><path d="m9 4-5 5 5 5M4 9h10a6 6 0 0 1 0 12" /></svg
        ></button
      ><button
        class="icon-button"
        onclick={() => historyAction("redo")}
        disabled={!current ||
          !history.redo ||
          !!draft ||
          busy ||
          loading ||
          disconnected}
        aria-label={t("Redo")}
        title={history.redo ? `${t("Redo")}: ${history.redo.label}` : t("Redo")}
        ><svg
          aria-hidden="true"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"
          ><path d="m15 4 5 5-5 5M20 9H10a6 6 0 0 0 0 12" /></svg
        ></button
      ><button
        class="icon-button"
        onclick={() => void refresh()}
        disabled={busy || loading}
        aria-label={t("Refresh budget")}
        title={t("Refresh budget")}><RefreshIcon /></button
      >
    </div>
  </div>
  {#if disconnected}<p role="status">
      Budget changes need a connection. Displayed figures are the last confirmed
      view.
    </p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p
      role="status"
    >
      {notice}
    </p>{/if}
  {#if storageBroken}<p>
      Keep browser data for recovery; the unreadable draft has not been removed.
    </p>
    <button onclick={() => location.reload()}>Reload saved budget work</button
    >{/if}
  {#if loading}<p role="status">Loading confirmed budget figures…</p>{/if}
  {#if draft?.quick}<div class="quick-status" aria-live="polite">
      {#if busy}<span>Saving…</span>{:else}
        {#if draft.pending}<button
            onclick={() => void send()}
            disabled={disconnected}>Retry saved amount</button
          ><button onclick={() => void send(true)} disabled={disconnected}
            >Cancel saved request</button
          >
        {:else}<button onclick={() => void send()} disabled={disconnected}
            >Save amount</button
          ><button onclick={discard}>Discard amount</button>{/if}
      {/if}
    </div>{/if}
  {#if view && current}
    {#if draft && !draft.category}{@render planEditor()}{/if}
    <div class="month-grid">
      {#each Object.values(view.months) as m (m.month)}
        <section class="budget-month" aria-labelledby={`month-${m.month}`}>
          <h2 id={`month-${m.month}`}>{title(m.month)}</h2>
          <dl class="summary">
            {#each [["Planned income", "total_budget_income"], ["Planned expenses", "total_budgeted"], ["Projected savings", "total_saved"], ["Actual income", "total_income"], ["Actual spending", "total_spent"], ["Final savings", "real_saved"]] as [label, key] (key)}<div
              >
                <dt>{label}</dt>
                <dd data-testid={`${m.month}-${key}`}>
                  {formatAmount(m.summary[key])}
                </dd>
              </div>{/each}
          </dl>
          <div class="month-actions">
            <button
              onclick={() => open("copy_previous", m)}
              disabled={!!draft || busy || disconnected}
              >Copy previous month</button
            >
          </div>
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (The scroll region itself supports Home/End; child controls retain native keys.) -->
          <div
            class="budget-table-scroll"
            tabindex="0"
            role="region"
            aria-label={`Budget categories for ${title(m.month)}`}
            onkeydown={(event) => {
              if (
                event.target === event.currentTarget &&
                (event.key === "Home" || event.key === "End")
              ) {
                event.preventDefault();
                event.currentTarget.scrollLeft =
                  event.key === "End" ? event.currentTarget.scrollWidth : 0;
              }
            }}
          >
            <table>
              <caption class="visually-hidden"
                >{title(m.month)} allocations and actuals</caption
              ><thead
                ><tr
                  ><th
                    ><div class="group-label">
                      {t("Category")}<button
                        class="add-icon"
                        aria-label="Add group"
                        title="Add group"
                        disabled={busy ||
                          disconnected ||
                          !!draft ||
                          !!categoryEditor}
                        onclick={() =>
                          (categoryEditor = { kind: "category-groups" })}
                        ><svg
                          aria-hidden="true"
                          width="18"
                          height="18"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.75"><path d="M12 5v14M5 12h14" /></svg
                        ></button
                      >
                    </div></th
                  ><th class="amount">Planned</th><th class="amount">Actual</th
                  ><th class="amount">{t("Remaining")}</th></tr
                ></thead
              ><tbody
                >{#each grouped(m.categories) as group (group.id)}
                  <tr class="budget-group" style:--group-color={group.color}>
                    <th scope="row"
                      ><div class="group-label">
                        <button
                          class="group-toggle"
                          aria-expanded={!collapsedGroups.includes(group.id)}
                          onclick={() => {
                            collapsedGroups = collapsedGroups.includes(group.id)
                              ? collapsedGroups.filter((id) => id !== group.id)
                              : [...collapsedGroups, group.id];
                          }}
                        >
                          <svg
                            aria-hidden="true"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.75"
                            ><path
                              d={collapsedGroups.includes(group.id)
                                ? "m9 5 7 7-7 7"
                                : "m5 9 7 7 7-7"}
                            /></svg
                          >{group.name}
                        </button>
                        {#if categoryGroups.some((g) => g.id === group.id)}<CategoryColor
                            {budgetId}
                            id={group.id}
                            kind="category-groups"
                            name={group.name}
                            color={group.color}
                            disabled={busy || disconnected || !!draft}
                            onchanged={refresh}
                          />{/if}
                        <button
                          class="add-icon"
                          aria-label={`Add category to ${group.name}`}
                          title={`Add category to ${group.name}`}
                          disabled={busy ||
                            disconnected ||
                            !!draft ||
                            !!categoryEditor}
                          onclick={() => {
                            collapsedGroups = collapsedGroups.filter(
                              (id) => id !== group.id,
                            );
                            categoryEditor = {
                              kind: "categories",
                              groupId: categoryGroups.some(
                                (g) => g.id === group.id,
                              )
                                ? group.id
                                : "",
                              income: group.income,
                            };
                          }}
                          ><svg
                            aria-hidden="true"
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.75"
                            ><path d="M12 5v14M5 12h14" /></svg
                          ></button
                        >
                        {#if categoryGroups.some((g) => g.id === group.id)}<button
                            class="add-icon"
                            aria-label={`Edit group ${group.name}`}
                            title={`Edit group ${group.name}`}
                            disabled={busy ||
                              disconnected ||
                              !!draft ||
                              !!categoryEditor}
                            onclick={() =>
                              (categoryEditor = {
                                kind: "category-groups",
                                id: group.id,
                              })}
                            ><svg
                              aria-hidden="true"
                              width="18"
                              height="18"
                              viewBox="0 0 24 24"
                              fill="none"
                              stroke="currentColor"
                              stroke-width="1.75"
                              ><path
                                d="m16 3 5 5-12 12-6 1 1-6Z M14 5l5 5"
                              /></svg
                            ></button
                          >{/if}
                      </div></th
                    >
                    <td class="amount"
                      >{groupTotal(group.categories, "budgeted")}</td
                    ><td class="amount"
                      >{groupTotal(group.categories, "actual")}</td
                    ><td class="amount"
                      >{groupTotal(group.categories, "balance")}</td
                    >
                  </tr>
                  {#if !collapsedGroups.includes(group.id)}
                    {#each group.categories as c (c.id)}
                      <tr
                        class="budget-category"
                        class:editing={draft?.category === c.id &&
                          draft.month === m.month}
                        style:--category-color={c.category_color ??
                          "transparent"}
                      >
                        <th scope="row"
                          ><div class="category-label">
                            <span
                              class="category-swatch"
                              style:background={c.category_color ?? group.color}
                              aria-hidden="true"
                            ></span><span class="category-name">{c.name}</span>
                            <button
                              class="add-icon"
                              aria-label={`Edit ${c.name} for ${title(m.month)}`}
                              title={`Edit ${c.name}`}
                              disabled={busy ||
                                disconnected ||
                                !!draft ||
                                !!categoryEditor}
                              onclick={() => editRow(m, c)}
                            >
                              <svg
                                aria-hidden="true"
                                width="16"
                                height="16"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.75"
                                ><path
                                  d="m16 3 5 5-12 12-6 1 1-6Z M14 5l5 5"
                                /></svg
                              >
                            </button>
                          </div></th
                        >
                        <td class="amount"
                          ><input
                            class="planned-input"
                            aria-label={`Planned ${c.name} for ${title(m.month)}`}
                            inputmode="decimal"
                            maxlength="256"
                            value={isQuick(m, c)
                              ? draft!.amount
                              : formatAmountInput(c.budgeted)}
                            disabled={disconnected ||
                              !!categoryEditor ||
                              (!!draft && !isQuick(m, c)) ||
                              (busy && !isQuick(m, c))}
                            oninput={(e) =>
                              quickInput(m, c, e.currentTarget.value)}
                            onblur={() => void flushQuick(true)}
                            onkeydown={(e) => {
                              if (e.key === "Enter") {
                                e.preventDefault();
                                void flushQuick(true);
                              }
                            }}
                          /></td
                        >
                        <td class="amount">{formatAmount(c.actual)}</td>
                        <td class="amount">{formatAmount(c.balance)}</td>
                      </tr>
                      {#if draft?.category === c.id && draft.month === m.month && !draft.quick}<tr
                          class="inline-plan-row"
                          ><td colspan="4">{@render planEditor()}</td></tr
                        >{/if}
                    {/each}{/if}{/each}</tbody
              >
            </table>
          </div>
          {#if !m.categories.length}<p>
              Create categories to start planning.
            </p>{/if}
        </section>{/each}
    </div>
  {/if}
  {#if detail}<section
      class="category-detail"
      aria-labelledby="plan-detail-title"
    >
      <div class="page-heading">
        <h2 id="plan-detail-title">
          {detail.category.name} · {title(detail.month)}
        </h2>
        <button onclick={() => (detail = null)}>Close category detail</button>
      </div>
      <p>
        {detail.total_count} contributing transactions · {formatAmount(
          detail.actual_total,
        )}
      </p>
      <div class="table-wrap">
        <table>
          <thead
            ><tr
              ><th>{t("Date")}</th><th>{t("Account")}</th><th>{t("Notes")}</th
              ><th class="amount">{t("Amount")}</th></tr
            ></thead
          ><tbody
            >{#each detail.data as row (row.id)}<tr
                ><td>{row.date}</td><td
                  ><button onclick={() => onaccount(row.account_id)}
                    >{row.account_name}</button
                  ></td
                ><td
                  >{row.payee_name ?? ""}{row.parent_id
                    ? " (split allocation)"
                    : ""}</td
                ><td class="amount">{formatAmount(row.amount)}</td></tr
              >{/each}</tbody
          >
        </table>
      </div>
      {#if !detail.data.length}<p>
          No contributing transactions this month.
        </p>{/if}
      <div class="actions">
        <button
          disabled={busy || detail.offset === 0}
          onclick={() =>
            view?.months[detail!.month] &&
            void showDetails(
              view.months[detail!.month],
              detail!.category,
              Math.max(0, detail!.offset - 100),
            )}>Previous transactions</button
        ><button
          disabled={busy ||
            detail.offset + detail.data.length >= detail.total_count}
          onclick={() =>
            view?.months[detail!.month] &&
            void showDetails(
              view.months[detail!.month],
              detail!.category,
              detail!.offset + 100,
            )}>Next transactions</button
        >
      </div>
    </section>{/if}
</section>

<style>
  .month-picker,
  .month-input {
    display: flex;
    align-items: center;
    gap: 0.15rem;
  }
  .month-input {
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .month-input input {
    width: 6.3rem;
    border: 0;
    background: transparent;
    padding: 0.4rem 0.5rem;
    font-variant-numeric: tabular-nums;
  }
  .month-arrow {
    display: grid;
    place-items: center;
    padding: 0;
    width: 32px;
    min-height: 36px;
    border: 0;
    background: transparent;
  }
  .month-controls {
    align-items: center;
  }
  .category-name {
    flex: 1;
  }
  .planned-input {
    width: 7rem;
    min-width: 0;
    text-align: right;
    font: inherit;
    font-variant-numeric: tabular-nums;
    border: 1px solid transparent;
    background: transparent;
    padding: 0.35rem;
  }
  .planned-input:hover,
  .planned-input:focus {
    border-color: var(--line);
    background: var(--surface);
  }
  .quick-status {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    min-height: 2.5rem;
  }
  @media (pointer: coarse) {
    .month-arrow {
      width: 44px;
      min-height: 44px;
    }
  }

  .category-swatch {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex: 0 0 12px;
    margin-right: 0.6rem;
  }
  .budget-category:hover,
  .budget-category.editing {
    background: color-mix(
      in srgb,
      var(--category-color) 14%,
      var(--surface, #fff)
    );
  }
  .budget-category:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .inline-plan-row td {
    padding: 1rem;
  }
  .inline-plan-row .plan-form {
    border: 0;
    padding: 0;
    margin: 0;
  }
  .budget-month {
    scroll-snap-align: start;
  }

  thead th:first-child,
  .budget-group th,
  .budget-category th {
    min-width: 14rem;
  }
  .inline-category-editor {
    margin-block: 1.5rem;
    padding: 1.25rem;
    border: 1px solid var(--line);
    border-radius: 12px;
  }
  .add-icon {
    display: inline-grid;
    place-items: center;
    width: 44px;
    min-height: 44px;
    padding: 0;
    background: transparent;
    border: 0;
  }

  .budget-group {
    background: color-mix(
      in srgb,
      var(--group-color) 12%,
      var(--surface, #fff)
    );
  }
  .budget-group th,
  .budget-group td {
    font-weight: 650;
    border-top: 1px solid var(--line);
  }
  .group-label,
  .category-label {
    display: flex;
    align-items: center;
  }
  .category-label {
    padding-left: 1.25rem;
  }
  .group-toggle {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    border: 0;
    background: transparent;
    font: inherit;
    padding: 0.25rem 0;
    min-height: 44px;
    text-align: left;
  }
  .budget-category {
    background: color-mix(
      in srgb,
      var(--category-color) 12%,
      var(--surface, #fff)
    );
  }

  .month-controls,
  .plan-tools,
  .month-actions,
  .editor-tools {
    display: flex;
    align-items: end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .month-controls label {
    display: grid;
    gap: 0.25rem;
  }
  .plan-tools {
    flex-wrap: nowrap;
  }
  .icon-button {
    width: 44px;
    height: 44px;
    padding: 0;
    display: grid;
    place-items: center;
    border-color: transparent;
    background: transparent;
  }
  .month-grid {
    position: relative;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(min(100%, 32rem), 1fr);
    width: 100%;
    max-width: 100%;
    min-width: 0;
    overflow-x: auto;
    align-items: start;
    scroll-snap-type: x proximity;
    gap: 2rem;
  }
  .budget-month {
    min-width: 0;
    padding: 1rem 0;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem 2rem;
    margin: 1rem 0;
  }
  .summary dt {
    font-size: 0.875rem;
  }
  .summary dd {
    margin: 0.25rem 0 0;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .month-actions {
    margin: 1rem 0;
  }
  .budget-table-scroll {
    overflow-x: auto;
    max-width: 100%;
  }
  .budget-table-scroll table {
    min-width: 30rem;
  }
  .budget-table-scroll th .hint {
    display: block;
    font-weight: 400;
  }
  .editor-tools {
    gap: 0.4rem;
  }
  .plan-form,
  .category-detail {
    border-top: 1px solid var(--border);
    padding: 1.5rem 0;
    margin-top: 1rem;
  }
  .plan-form form {
    max-width: 36rem;
  }
  .category-detail .table-wrap {
    max-width: 100%;
    overflow-x: auto;
  }
  .amount-entry {
    display: flex;
    gap: 0.5rem;
  }
  .amount-entry input {
    min-width: 0;
    flex: 1;
  }
  @media (max-width: 700px) {
    .month-controls {
      display: flex;
    }
    .month-controls > label {
      min-width: 0;
      flex: 1 1 100%;
    }
    .month-controls input,
    .month-controls select {
      width: 100%;
      min-width: 0;
    }
    .summary {
      gap: 1rem;
    }
    .summary > div {
      min-width: 8rem;
    }

    .editor-tools {
      min-width: 12rem;
    }
  }
</style>
