<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import BudgetTemplates from "./BudgetTemplates.svelte";
  import GoalProgress from "./GoalProgress.svelte";
  import { readAutomationDraft } from "./budget-automations";
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
    onchanged: (method: "tracking" | "envelope") => Promise<void>;
    onaccount: (id: string) => void;
  } = $props();
  let view = $state.raw<PlanView | null>(null),
    draft = $state<PlanningDraft | null>(null);
  let month = $state(today().slice(0, 7)),
    count = $state(1),
    busy = $state(false),
    loading = $state(true),
    error = $state(""),
    notice = $state(""),
    storageBroken = $state(false),
    showHidden = $state(false);
  let showTemplates = $state(false);
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
  const amounts = ["set", "hold", "move", "cover", "cover_overbudgeted"];
  const categoryActions = ["set", "carryover", "copy_year", "category_notes"];
  const labels: Record<string, string> = {
    set: "Edit allocation",
    carryover: "Carryover setting",
    copy_previous: "Copy previous month",
    copy_year: "Copy through year end",
    zero: "Clear allocations",
    hold: "Hold money for next month",
    reset_hold: "Release manual hold",
    move: "Move money",
    cover: "Cover overspending",
    cover_overbudgeted: "Cover overbudgeting",
    notes: "Month notes",
    category_notes: "Category notes",
    method: "Change budgeting method",
    undo: "Undo",
    redo: "Redo",
  };
  let current = $derived(view?.months[month]);
  let expenses = $derived(
    current?.categories.filter(
      (c) => !c.is_income && !c.hidden && !c.group_hidden,
    ) ?? [],
  );
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
      checkpoint = localStorage.getItem(planningKey(budgetId));
      if (draft) month = draft.month;
      const templates = readAutomationDraft(budgetId);
      if (!draft && templates) {
        month = templates.month;
        showTemplates = true;
      }
    } catch (e) {
      storageBroken = true;
      error = errorMessage(e);
      loading = false;
      return;
    }
    void refresh();
  });
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
        from:
          action === "cover_overbudgeted"
            ? (expenses[0]?.id ?? "")
            : "to-budget",
        to: expenses[0]?.id ?? "",
        amount: formatAmountInput(category?.budgeted ?? 0),
        notes: action === "category_notes" ? (category?.notes ?? "") : m.notes,
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
        if (draft.action !== "method" && view.method !== draft.method)
          throw new Error(
            `The budgeting method changed. This draft still belongs to ${draft.method === "tracking" ? "Tracking" : "Envelope"}. Keep any needed values, discard it and reopen the current plan.`,
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
        if (amounts.includes(draft.action)) {
          if (draft.action === "cover" && draft.amount.trim() === "") {
          } else body.amount = parseAmount(draft.amount, draft.format);
        }
        if (["move", "cover"].includes(draft.action)) {
          body.from = draft.from;
          body.to = draft.to;
        }
        if (draft.action === "cover_overbudgeted") body.from = draft.from;
        if (draft.action === "carryover") body.enabled = draft.enabled;
        if (["notes", "category_notes"].includes(draft.action))
          body.notes = draft.notes;
        if (draft.action === "method") body.method = draft.method;
        let path = `/budgets/${budgetId}/budget`;
        if (draft.action === "undo" || draft.action === "redo") {
          const available = history[draft.action];
          if (!available)
            throw new Error("Refresh the available history action.");
          body = { action_id: available.id };
          path = `/budgets/${budgetId}/history/${draft.action}`;
        }
        const pending = {
          path,
          method: "POST" as const,
          body,
          key: uuid(),
          historyOwner: await historyOwner(),
          edit_clock: nextEditClock(),
          budgetEpoch: draft.epoch,
        };
        save({ ...$state.snapshot(draft), pending });
      }
      if (cancel) requestCancellation(draft.pending!);
      save($state.snapshot(draft)); // CAS and durable storage must succeed before every dispatch.
      try {
        await mutate(draft.pending!, cancel);
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
      save(null);
      detail = null;
      notice = cancel
        ? "The original request was already saved."
        : "Budget change saved.";
      await refresh();
      if (view) await onchanged(view.method);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
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
</script>

<section aria-labelledby="planning-title">
  <div class="page-heading">
    <div>
      <h1 id="planning-title">{t("Budget")}</h1>
      <p class="muted">
        {view?.method === "tracking"
          ? "Compare planned income and expenses with actuals."
          : "Allocate available money and review category balances."}
      </p>
    </div>
    <button onclick={onclose} disabled={busy}>Close budget view</button>
  </div>
  <div class="toolbar">
    <button
      onclick={() => {
        showTemplates = true;
        notice = "";
        error = "";
      }}
      disabled={busy || loading || !!draft || showTemplates}
      >{t("Goal templates")}</button
    >
  </div>
  {#if showTemplates}<BudgetTemplates
      {budgetId}
      {month}
      {disconnected}
      onclose={() => (showTemplates = false)}
      onchanged={async () => {
        await refresh();
        if (view) await onchanged(view.method);
      }}
    />{/if}
  <div class="month-controls">
    <button
      onclick={() => shift(-1)}
      disabled={busy || loading || showTemplates}>{t("Previous month")}</button
    ><label
      >Budget month<input
        type="month"
        bind:value={month}
        onchange={() => void refresh()}
        disabled={busy || loading || showTemplates}
      /></label
    ><button
      onclick={() => shift(1)}
      disabled={busy || loading || showTemplates}>{t("Next month")}</button
    ><label
      >Months displayed<select
        bind:value={count}
        onchange={() => void refresh()}
        disabled={busy || loading || showTemplates}
        >{#each [1, 2, 3, 6, 12] as n (n)}<option value={n}>{n}</option
          >{/each}</select
      ></label
    ><button
      onclick={() => void refresh()}
      disabled={busy || loading || showTemplates}>Refresh budget</button
    >
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
  {#if view && current}
    <div class="plan-tools">
      <button
        onclick={() => historyAction("undo")}
        disabled={!history.undo ||
          !!draft ||
          busy ||
          disconnected ||
          showTemplates}
        >{t("Undo")}{history.undo ? ` ${history.undo.label}` : ""}</button
      ><button
        onclick={() => historyAction("redo")}
        disabled={!history.redo ||
          !!draft ||
          busy ||
          disconnected ||
          showTemplates}
        >{t("Redo")}{history.redo ? ` ${history.redo.label}` : ""}</button
      ><button
        onclick={() => open("method", current)}
        disabled={!!draft || busy || disconnected || showTemplates}
        >Change budgeting method</button
      ><label class="check"
        ><input type="checkbox" bind:checked={showHidden} />Show hidden
        categories</label
      >
    </div>
    {#if draft}
      <section class="plan-form" aria-labelledby="plan-action-title">
        <h2 id="plan-action-title">{labels[draft.action]}</h2>
        <p>
          {title(draft.month)} · {draft.method === "tracking"
            ? "Tracking"
            : "Envelope"}
        </p>
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
            {#if ["move", "cover", "cover_overbudgeted"].includes(draft.action)}<label
                for="plan-from">{t("From")}</label
              ><select
                id="plan-from"
                value={draft.from}
                onchange={(e) => field("from", e.currentTarget.value)}
                >{#if draft.action !== "cover_overbudgeted"}<option
                    value="to-budget">Available to budget</option
                  >{/if}{#each expenses as c (c.id)}<option value={c.id}
                    >{c.name}</option
                  >{/each}</select
              >{/if}
            {#if ["move", "cover"].includes(draft.action)}<label for="plan-to"
                >To</label
              ><select
                id="plan-to"
                value={draft.to}
                onchange={(e) => field("to", e.currentTarget.value)}
                >{#if draft.action === "move"}<option value="to-budget"
                    >Available to budget</option
                  >{/if}{#each expenses as c (c.id)}<option value={c.id}
                    >{c.name}</option
                  >{/each}</select
              >{/if}
            {#if draft.action === "method"}<label for="plan-method"
                >Budgeting method</label
              ><select
                id="plan-method"
                value={draft.method}
                onchange={(e) =>
                  field(
                    "method",
                    e.currentTarget.value as "tracking" | "envelope",
                  )}
                ><option value="tracking">Tracking</option><option
                  value="envelope">Envelope</option
                ></select
              >
              <p>
                Each method keeps its allocations. Switching changes the
                budgeting calculation.
              </p>{/if}
            {#if draft.action === "carryover"}<label class="check"
                ><input
                  type="checkbox"
                  checked={draft.enabled}
                  onchange={(e) => field("enabled", e.currentTarget.checked)}
                />{view.method === "envelope"
                  ? current.categories.find((c) => c.id === draft!.category)
                      ?.is_income
                    ? "Hold this category’s income for the following month"
                    : "Carry negative balances to the following month"
                  : "Carry this category’s remaining plan to the following month"}</label
              >
              <p class="hint">
                Applies from this month through the current planning horizon.
                Envelope positive balances always carry.
              </p>{/if}
            {#if ["notes", "category_notes"].includes(draft.action)}<label
                for="plan-notes"
                >{draft.action === "notes"
                  ? "Month notes"
                  : "Category notes"}</label
              ><textarea
                id="plan-notes"
                maxlength="10000"
                rows="5"
                value={draft.notes}
                oninput={(e) => field("notes", e.currentTarget.value)}
              ></textarea>{/if}
            {#if draft.action === "copy_year"}<p>
                Copy this allocation to later available months of the same year.
              </p>{/if}
            {#if draft.action === "copy_previous"}<p>
                Copy visible category allocations from the previous month.
                Carryover, holds and notes stay separate.
              </p>{/if}
            {#if draft.action === "zero"}<p>
                Set this month’s allocations to zero.
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
    {/if}
    <div class="month-grid">
      {#each Object.values(view.months) as m (m.month)}
        <section class="budget-month" aria-labelledby={`month-${m.month}`}>
          <h2 id={`month-${m.month}`}>{title(m.month)}</h2>
          <dl class="summary">
            {#if view.method === "tracking"}{#each [["Planned income", "total_budget_income"], ["Planned expenses", "total_budgeted"], ["Projected savings", "total_saved"], ["Actual income", "total_income"], ["Actual spending", "total_spent"], ["Final savings", "real_saved"]] as [label, key] (key)}<div
                >
                  <dt>{label}</dt>
                  <dd data-testid={`${m.month}-${key}`}>
                    {formatAmount(m.summary[key])}
                  </dd>
                </div>{/each}{:else}{#each [["Available to budget", "to_budget"], ["Income", "total_income"], ["From previous month", "from_last_month"], ["Previous overspending", "last_month_overspent"], ["Held for next month", "buffered_selected"], ["Category balances", "total_leftover"]] as [label, key] (key)}<div
                >
                  <dt>{label}</dt>
                  <dd data-testid={`${m.month}-${key}`}>
                    {formatAmount(m.summary[key])}
                  </dd>
                </div>{/each}{/if}
          </dl>
          <div class="month-actions">
            <button
              onclick={() => open("copy_previous", m)}
              disabled={!!draft || busy || disconnected || showTemplates}
              >Copy previous month</button
            ><button
              onclick={() => open("zero", m)}
              disabled={!!draft || busy || disconnected || showTemplates}
              >Clear allocations</button
            >{#if view.method === "envelope"}<button
                onclick={() => open("move", m)}
                disabled={!!draft || busy || disconnected || showTemplates}
                >Move money</button
              ><button
                onclick={() => open("cover", m)}
                disabled={!!draft || busy || disconnected || showTemplates}
                >{t("Cover overspending")}</button
              ><button
                onclick={() => open("cover_overbudgeted", m)}
                disabled={!!draft || busy || disconnected || showTemplates}
                >Cover overbudgeting</button
              ><button
                onclick={() => open("hold", m)}
                disabled={!!draft || busy || disconnected || showTemplates}
                >Hold income</button
              ><button
                onclick={() => open("reset_hold", m)}
                disabled={!!draft || busy || disconnected || showTemplates}
                >Release manual hold</button
              >{/if}<button
              onclick={() => open("notes", m)}
              disabled={!!draft || busy || disconnected || showTemplates}
              >Edit month notes</button
            >
          </div>
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial table needs keyboard access at enlarged text.) -->
          <div
            class="budget-table-scroll"
            tabindex="0"
            role="region"
            aria-label={`Budget categories for ${title(m.month)}`}
          >
            <table>
              <caption class="visually-hidden"
                >{title(m.month)} allocations and actuals</caption
              ><thead
                ><tr
                  ><th>{t("Category")}</th><th class="amount"
                    >{view.method === "tracking" ? "Planned" : "Allocated"}</th
                  ><th class="amount">Actual</th><th class="amount"
                    >{t(
                      view.method === "tracking" ? "Remaining" : "Balance",
                    )}</th
                  ><th>{t("Actions")}</th></tr
                ></thead
              ><tbody
                >{#each m.categories.filter((c) => showHidden || (!c.hidden && !c.group_hidden)) as c (c.id)}<tr
                    ><th scope="row"
                      >{c.name}<span class="hint"
                        >{c.group_name} · {c.is_income
                          ? "Income"
                          : "Expense"}{c.hidden || c.group_hidden
                          ? " · Hidden"
                          : ""}</span
                      ></th
                    ><td class="amount"
                      >{view.method === "envelope" && c.is_income
                        ? "—"
                        : formatAmount(c.budgeted)}</td
                    ><td class="amount"
                      ><button
                        class="text-button"
                        onclick={() => void showDetails(m, c)}
                        disabled={busy || disconnected}
                        aria-label={`View ${c.name} transactions for ${title(m.month)}`}
                        >{formatAmount(c.actual)}</button
                      ></td
                    ><td class="amount"
                      >{view.method === "envelope" && c.is_income
                        ? "—"
                        : formatAmount(
                            c.balance,
                          )}{#if preferences.shared.experimental.goalTemplatesEnabled && !(view.method === "envelope" && c.is_income)}<GoalProgress
                          category={c}
                        />{/if}</td
                    ><td
                      ><div class="row-actions">
                        {#if view.method === "tracking" || !c.is_income}<button
                            onclick={() => open("set", m, c)}
                            disabled={!!draft ||
                              busy ||
                              disconnected ||
                              showTemplates}
                            aria-label={`Edit ${c.name} allocation for ${title(m.month)}`}
                            >{t("Edit")}</button
                          ><button
                            onclick={() => open("copy_year", m, c)}
                            disabled={!!draft ||
                              busy ||
                              disconnected ||
                              showTemplates}
                            aria-label={`Copy ${c.name} through year end from ${title(m.month)}`}
                            >Copy year</button
                          ><button
                            onclick={() => open("copy_previous", m, c)}
                            disabled={!!draft ||
                              busy ||
                              disconnected ||
                              showTemplates}
                            aria-label={`Copy previous ${c.name} allocation into ${title(m.month)}`}
                            >Copy previous</button
                          >{/if}<button
                          onclick={() => open("carryover", m, c)}
                          disabled={!!draft ||
                            busy ||
                            disconnected ||
                            showTemplates}
                          aria-label={`Change ${c.name} carryover from ${title(m.month)}`}
                          >{c.carryover
                            ? "Carryover on"
                            : "Carryover off"}</button
                        ><button
                          onclick={() => open("category_notes", m, c)}
                          disabled={!!draft ||
                            busy ||
                            disconnected ||
                            showTemplates}
                          aria-label={`Edit ${c.name} notes`}
                          >{t("Notes")}</button
                        >
                      </div></td
                    ></tr
                  >{/each}</tbody
              >
            </table>
          </div>
          {#if !m.categories.length}<p>
              Create categories to start planning.
            </p>{/if}{#if m.notes}<h3>Month notes</h3>
            <p class="saved-notes">{m.notes}</p>{/if}
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
      {#if detail.category.notes}<p class="saved-notes">
          {detail.category.notes}
        </p>{/if}
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
                ><td>{row.notes}{row.parent_id ? " (split allocation)" : ""}</td
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
  .month-controls,
  .plan-tools,
  .month-actions,
  .row-actions {
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
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 2.75rem;
  }
  .check input {
    width: auto;
  }
  .month-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 36rem), 1fr));
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
    min-width: 40rem;
  }
  .budget-table-scroll th .hint {
    display: block;
    font-weight: 400;
  }
  .row-actions {
    min-width: 13rem;
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
  .saved-notes {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
    .month-controls > label:first-of-type {
      order: -1;
    }
    .month-controls > button {
      flex: 1 1 9rem;
      min-width: 0;
      white-space: normal;
      overflow-wrap: anywhere;
    }
    .month-controls > button:last-child {
      flex-basis: 100%;
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
    .month-grid {
      display: block;
    }
    .row-actions {
      min-width: 12rem;
    }
  }
</style>
