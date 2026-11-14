<script lang="ts">
  import Notifications from "./Notifications.svelte";
  import ReorderHandle from "./ReorderHandle.svelte";
  import type { PendingMutation } from "./register-api";
  import RefreshIcon from "./RefreshIcon.svelte";
  import { outsideClick } from "./outside-click";
  import Categories from "./Categories.svelte";
  import type { CategoryLaunch } from "./category-colors";
  import CategoryColor from "./CategoryColor.svelte";
  import { t } from "./i18n.svelte";
  import { onMount, onDestroy, tick } from "svelte";
  import { SvelteDate } from "svelte/reactivity";
  import { request, uuid, errorMessage } from "./api";
  import { mutate, requestCancellation } from "./register-api";
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
    onchanged,
    ontransactions,
  }: {
    budgetId: string;
    disconnected: boolean;
    onchanged: (method: "tracking") => Promise<void>;
    ontransactions: (category: string, month: string) => void;
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
    group_id?: string | null;
    field_versions: Record<string, string>;
  };
  let categoryRows = $state.raw<Group[]>([]);
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
    const rows: Group[] = [];
    offset = 0;
    do {
      const result: { data: Group[]; next_offset: number | null } =
        await request(
          `/budgets/${budgetId}/categories?limit=1000&offset=${offset}`,
        );
      rows.push(...result.data);
      offset = result.next_offset;
    } while (offset !== null);
    categoryRows = rows;
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
      (a, b) => a.order - b.order || a.name.localeCompare(b.name),
    );
  }
  type ItemKind = "category-groups" | "categories";
  type DragItem = { kind: ItemKind; id: string; month: string };
  let dragging = $state<DragItem | null>(null);
  let dropTarget = $state<{
    kind: ItemKind;
    id: string;
    after: boolean;
  } | null>(null);
  let arrangement = $state.raw<PendingMutation[]>([]);
  let arrangementError = $state("");
  let arranging = $state(false);
  const arrangementKey = $derived(`actualBudget.arrangement.v1.${budgetId}`);
  const arrangementLocked = $derived(
    busy ||
      loading ||
      disconnected ||
      !!draft ||
      !!categoryEditor ||
      arranging ||
      arrangement.length > 0 ||
      !!arrangementError,
  );
  onMount(() => {
    try {
      const raw = localStorage.getItem(arrangementKey);
      if (raw) arrangement = JSON.parse(raw);
    } catch (e) {
      arrangementError = errorMessage(e);
    }
  });
  async function sendArrangement() {
    if (arranging || disconnected || !arrangement.length) return;
    arranging = true;
    arrangementError = "";
    try {
      while (arrangement.length) {
        if (
          localStorage.getItem(arrangementKey) !== JSON.stringify(arrangement)
        )
          throw new Error(
            "Another tab changed this saved edit. Reload before retrying.",
          );
        const result = await mutate(arrangement[0]);
        const remaining = arrangement.slice(1);
        if (remaining.length)
          localStorage.setItem(arrangementKey, JSON.stringify(remaining));
        else localStorage.removeItem(arrangementKey);
        arrangement = remaining;
        if (result.conflicts?.length) {
          arrangementError =
            "A newer edit was kept. Review the current order or name.";
          break;
        }
      }
      await refresh();
    } catch (e) {
      arrangementError = errorMessage(e);
    } finally {
      arranging = false;
    }
  }
  async function saveArrangement(
    kind: ItemKind,
    edits: { row: Group; body: Record<string, unknown> }[],
  ) {
    if (arrangementLocked || !view || !edits.length) return;
    const epoch = view.epoch;
    arranging = true;
    arrangementError = "";
    try {
      if (localStorage.getItem(arrangementKey))
        throw new Error("Another tab has a saved edit. Reload to recover it.");
      const owner = await historyOwner();
      if (localStorage.getItem(arrangementKey))
        throw new Error("Another tab has a saved edit. Reload to recover it.");
      const operations: PendingMutation[] = edits.map(({ row, body }) => ({
        path: `/budgets/${budgetId}/${kind}/${row.id}`,
        method: "PATCH",
        key: uuid(),
        body,
        budgetEpoch: epoch,
        edit_clock: nextEditClock(row.field_versions),
        historyOwner: owner,
      }));
      localStorage.setItem(arrangementKey, JSON.stringify(operations));
      arrangement = operations;
    } catch (e) {
      arrangementError = errorMessage(e);
    } finally {
      arranging = false;
    }
    if (arrangement.length) await sendArrangement();
  }
  async function renameItem(
    row: Group,
    input: HTMLInputElement,
    kind: ItemKind = "category-groups",
  ) {
    const name = input.value.trim();
    if (!name) {
      input.value = row.name;
      return;
    }
    if (name === row.name) return;
    await saveArrangement(kind, [{ row, body: { name } }]);
  }
  function cancelDrag() {
    dragging = null;
    dropTarget = null;
  }
  function moveDrag(x: number, y: number) {
    if (!dragging) return;
    const row = document
      .elementFromPoint(x, y)
      ?.closest<HTMLElement>("[data-budget-kind]");
    const kind = row?.dataset.budgetKind as ItemKind | undefined;
    if (
      !row ||
      !kind ||
      row.dataset.month !== dragging.month ||
      (dragging.kind === "category-groups" && kind !== "category-groups")
    ) {
      dropTarget = null;
      return;
    }
    const rect = row.getBoundingClientRect();
    dropTarget = {
      kind,
      id: row.dataset.id!,
      after: y > rect.top + rect.height / 2,
    };
    const scroll = row.closest(".budget-table-scroll");
    if (scroll && x > innerWidth - 35) scroll.scrollBy({ left: 20 });
    if (y > innerHeight - 45) window.scrollBy(0, 15);
    else if (y < 60) window.scrollBy(0, -15);
  }
  async function reorder(
    source: DragItem,
    target: { kind: ItemKind; id: string; after: boolean },
  ) {
    const rows =
      source.kind === "category-groups" ? categoryGroups : categoryRows;
    const row = rows.find((r) => r.id === source.id);
    if (!row || (source.kind === target.kind && source.id === target.id))
      return;
    const destination =
      target.kind === "categories"
        ? categoryRows.find((r) => r.id === target.id)
        : categoryGroups.find((r) => r.id === target.id);
    const groupId =
      target.kind === "categories"
        ? (destination?.group_id ?? null)
        : (destination?.id ?? null);
    if (
      source.kind === "categories" &&
      destination &&
      destination.is_income !== row.is_income
    ) {
      notice = "Keep income and expense categories in their matching groups.";
      return;
    }
    // Preserve the displayed order when imported records share the same order number.
    const displayed =
      source.kind === "category-groups"
        ? grouped([]).map((g) => g.id)
        : (Object.values(view?.months ?? {})[0]?.categories.map((c) => c.id) ??
          []);
    const siblings = displayed
      .map((id) => rows.find((r) => r.id === id))
      .filter(
        (r): r is Group =>
          !!r &&
          r.id !== row.id &&
          (source.kind === "category-groups" ||
            (r.group_id ?? null) === groupId),
      );
    let index =
      source.kind === target.kind
        ? siblings.findIndex((r) => r.id === target.id)
        : siblings.length;
    if (index < 0) return;
    if (source.kind === target.kind && target.after) index++;
    const before = siblings[index - 1]?.sort_order ?? (index === 0 ? null : 0);
    const after =
      siblings[index]?.sort_order ?? (index === siblings.length ? null : 0);
    const order =
      before === null
        ? (after ?? 0) - 1024
        : after === null
          ? before + 1024
          : before + (after - before) / 2;
    const groupPatch =
      source.kind === "categories" && (row.group_id ?? null) !== groupId
        ? { group_id: groupId }
        : {};
    if (
      Number.isFinite(order) &&
      Math.abs(order) < Number.MAX_SAFE_INTEGER &&
      (before === null || order > before) &&
      (after === null || order < after)
    ) {
      await saveArrangement(source.kind, [
        { row, body: { ...groupPatch, sort_order: order } },
      ]);
    } else {
      siblings.splice(index, 0, row);
      await saveArrangement(
        source.kind,
        siblings.map((r, i) => ({
          row: r,
          body: {
            ...(r.id === row.id ? groupPatch : {}),
            sort_order: i * 1024,
          },
        })),
      );
    }
    if (!arrangementError && !arrangement.length) notice = "Order saved.";
  }
  function drop() {
    const source = dragging,
      target = dropTarget;
    cancelDrag();
    if (source && target) void reorder(source, target);
  }
  function stepItem(source: DragItem, direction: number) {
    const groups = grouped(
      Object.values(view?.months ?? {})[0]?.categories ?? [],
    );
    const ids =
      source.kind === "category-groups"
        ? groups
            .filter((g) => categoryGroups.some((r) => r.id === g.id))
            .map((g) => g.id)
        : (groups
            .find((g) => g.categories.some((c) => c.id === source.id))
            ?.categories.map((c) => c.id) ?? []);
    const target = ids[ids.indexOf(source.id) + direction];
    if (target)
      void reorder(source, {
        kind: source.kind,
        id: target,
        after: direction > 0,
      });
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
  let selectedCategory = $state<{ id: string; month: string } | null>(null);
  let checkpoint: string | null = null;
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
  const categoryActions = ["set", "carryover"];
  const labels: Record<string, string> = {
    set: "Edit allocation",
    carryover: "Carryover setting",
    copy_previous: "Copy previous month",
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
  function historyAction(direction: "undo" | "redo") {
    if (current) {
      open(direction, current);
      void send();
    }
  }
  function title(m: string) {
    return new Intl.DateTimeFormat("en-US", {
      month: "long",
      year: "numeric",
      timeZone: "UTC",
    }).format(new Date(m + "-01T12:00:00Z"));
  }
  function editRow(m: PlanMonth, c: PlanCategory) {
    if (busy || disconnected || categoryEditor || draft) return;
    selectedCategory = { id: c.id, month: m.month };
  }
  async function categoryAction(
    action: "carryover" | "copy_previous",
    m: PlanMonth,
    c: PlanCategory,
    enabled?: boolean,
  ) {
    if (draft || busy || disconnected) return;
    open(action, m, c);
    const created = draft as PlanningDraft | null;
    if (!created) return;
    try {
      if (action === "carryover") {
        // Actual's stored carryover flag belongs to the outgoing month.
        const previous = new SvelteDate(m.month + "-01T12:00:00Z");
        previous.setUTCMonth(previous.getUTCMonth() - 1);
        save({
          ...$state.snapshot(created),
          month: previous.toISOString().slice(0, 7),
          enabled: !!enabled,
        });
      }
      await send();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

{#snippet categoryControls(m: PlanMonth, c: PlanCategory)}
  <section
    class="category-controls"
    aria-label={`Budget options for ${c.name}`}
    {@attach outsideClick(() => {
      if (!busy && !draft) selectedCategory = null;
    })}
  >
    <label class="carryover-toggle"
      ><input
        type="checkbox"
        role="switch"
        checked={c.carryover_from_previous ?? true}
        disabled={busy || !!draft || disconnected || m.month === "0001-01"}
        onchange={(e) =>
          void categoryAction("carryover", m, c, e.currentTarget.checked)}
      /><span class="carryover-track" aria-hidden="true"></span>Carryover excess
      from previous month</label
    >
    <div class="editor-tools">
      <button
        disabled={busy || !!draft || disconnected}
        onclick={() => void categoryAction("copy_previous", m, c)}
        >Copy previous</button
      >
      <button
        disabled={busy || !!draft || disconnected}
        onclick={() => ontransactions(c.id, m.month)}>Transactions</button
      >
      <button
        class="add-icon delete-category-icon"
        aria-label={`Delete category ${c.name}`}
        title="Delete category"
        disabled={busy || !!draft || disconnected || !!categoryEditor}
        onclick={() => {
          selectedCategory = null;
          categoryEditor = { kind: "categories", id: c.id, delete: true };
        }}
        ><svg
          aria-hidden="true"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"
          ><path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 10v8m4-8v8" /></svg
        ></button
      >
    </div>
  </section>
{/snippet}

{#snippet planEditor()}{#if draft && view}
    <section class="plan-form" aria-label="Saved budget change">
      <p>
        {labels[draft.action] ?? "Retired saved action"} · {title(draft.month)}
      </p>
      {#if draft.pending}
        <p role="status">
          The outcome is unconfirmed. Retry the original saved request.
        </p>
        <button onclick={() => void send()} disabled={busy || disconnected}
          >Retry saved budget request</button
        >
        <button onclick={() => void send(true)} disabled={busy || disconnected}
          >Cancel saved budget request safely</button
        >
      {:else}
        <p role="status">
          A saved change needs attention before editing can continue.
        </p>
        <button onclick={() => void send()} disabled={busy || disconnected}
          >Retry saved budget change</button
        >
        <button onclick={discard} disabled={busy}>Discard saved change</button>
      {/if}
    </section>
  {/if}{/snippet}

{#snippet reorderHandle(
  kind: ItemKind,
  id: string,
  name: string,
  month: string,
)}
  <ReorderHandle
    label={`Move ${kind === "categories" ? "category" : "group"} ${name}`}
    disabled={arrangementLocked}
    onstart={() => {
      dragging = { kind, id, month };
    }}
    onmove={moveDrag}
    onend={drop}
    oncancel={cancelDrag}
    onstep={(direction) => stepItem({ kind, id, month }, direction)}
  />
{/snippet}
<section aria-labelledby="planning-title">
  <div class="page-heading">
    <div>
      <h1 id="planning-title">{t("Budget")}</h1>
    </div>
  </div>
  {#if arrangement.length}<div role="status">
      A group or category edit is saved for retry.
      <button
        onclick={() => void sendArrangement()}
        disabled={arranging || disconnected}
        >Retry saved group/category edit</button
      >
    </div>{/if}
  {#if arrangementError}<p role="alert">{arrangementError}</p>
    {#if !arrangement.length}<button
        onclick={() => {
          arrangementError = "";
          void refresh();
        }}>Dismiss and refresh</button
      >{/if}
  {/if}
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
  <Notifications bind:notice bind:error />
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
    {#if draft && !busy && !draft.quick}{@render planEditor()}{/if}
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
              onclick={() => {
                open("copy_previous", m);
                void send();
              }}
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
                  {@const record = categoryGroups.find(
                    (g) => g.id === group.id,
                  )}
                  <tr
                    class="budget-group"
                    style:--group-color={group.color}
                    data-budget-kind="category-groups"
                    data-id={group.id}
                    data-month={m.month}
                    class:drop-before={dragging?.month === m.month &&
                      dropTarget?.id === group.id &&
                      !dropTarget.after}
                    class:drop-after={dragging?.month === m.month &&
                      dropTarget?.id === group.id &&
                      dropTarget.after}
                  >
                    <th scope="row"
                      ><div class="group-label">
                        {#if categoryGroups.find((g) => g.id === group.id)}
                          {@render reorderHandle(
                            "category-groups",
                            group.id,
                            group.name,
                            m.month,
                          )}
                        {/if}
                        <button
                          aria-label={`Toggle ${group.name}`}
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
                          >
                        </button>
                        {#if record}<input
                            class="group-name"
                            aria-label={`Group name ${group.name}`}
                            maxlength="120"
                            value={record.name}
                            disabled={arrangementLocked}
                            onblur={(e) =>
                              void renameItem(record, e.currentTarget)}
                            onkeydown={(e) => {
                              if (e.key === "Enter") e.currentTarget.blur();
                              if (e.key === "Escape") {
                                e.currentTarget.value = record.name;
                                e.currentTarget.blur();
                              }
                            }}
                          />
                        {:else}<span>{group.name}</span>{/if}
                        {#if categoryGroups.some((g) => g.id === group.id)}<CategoryColor
                            {budgetId}
                            id={group.id}
                            kind="category-groups"
                            name={group.name}
                            color={group.color}
                            disabled={busy || disconnected || !!draft}
                            onchanged={refresh}
                          />{/if}
                        {#if record}<button
                            class="add-icon"
                            aria-label={`Edit group ${group.name}`}
                            title={`Edit group ${group.name}`}
                            disabled={arrangementLocked}
                            onclick={() => {
                              selectedCategory = null;
                              categoryEditor = {
                                kind: "category-groups",
                                id: group.id,
                              };
                            }}
                            ><svg
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
                            ></button
                          >{/if}
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
                      {@const categoryRecord = categoryRows.find(
                        (row) => row.id === c.id,
                      )}
                      <tr
                        class="budget-category"
                        data-budget-kind="categories"
                        data-id={c.id}
                        data-month={m.month}
                        class:drop-before={dragging?.month === m.month &&
                          dropTarget?.id === c.id &&
                          !dropTarget.after}
                        class:drop-after={dragging?.month === m.month &&
                          dropTarget?.id === c.id &&
                          dropTarget.after}
                        class:editing={draft?.category === c.id &&
                          draft.month === m.month}
                        style:--category-color={c.category_color ??
                          "transparent"}
                      >
                        <th scope="row"
                          ><div class="category-label">
                            {@render reorderHandle(
                              "categories",
                              c.id,
                              c.name,
                              m.month,
                            )}
                            <CategoryColor
                              {budgetId}
                              id={c.id}
                              kind="categories"
                              name={c.name}
                              color={c.category_color ?? group.color}
                              compact
                              disabled={busy ||
                                disconnected ||
                                !!draft?.pending}
                              onchanged={refresh}
                            />
                            {#if categoryRecord}<input
                                class="category-name group-name"
                                aria-label={`Category name ${c.name}`}
                                title={c.name}
                                maxlength="120"
                                value={categoryRecord.name}
                                disabled={arrangementLocked}
                                onblur={(e) =>
                                  void renameItem(
                                    categoryRecord,
                                    e.currentTarget,
                                    "categories",
                                  )}
                                onkeydown={(e) => {
                                  if (e.key === "Enter") e.currentTarget.blur();
                                  if (e.key === "Escape") {
                                    e.currentTarget.value = categoryRecord.name;
                                    e.currentTarget.blur();
                                  }
                                }}
                              />{:else}<span class="category-name"
                                >{c.name}</span
                              >{/if}
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
                      {#if selectedCategory?.id === c.id && selectedCategory.month === m.month}<tr
                          class="inline-plan-row"
                          ><td colspan="4">{@render categoryControls(m, c)}</td
                          ></tr
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
</section>

<style>
  .delete-category-icon {
    margin-left: auto;
    color: var(--muted);
  }
  .delete-category-icon:hover {
    color: var(--error-text);
  }
  .category-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem 1.5rem;
    padding-block: 0.5rem;
  }
  .carryover-toggle {
    position: relative;
    display: flex;
    gap: 0.6rem;
    align-items: center;
    min-height: 44px;
    margin: 0;
    cursor: pointer;
  }
  .carryover-toggle input {
    position: absolute;
    width: 100%;
    height: 100%;
    opacity: 0;
    margin: 0;
    cursor: inherit;
  }
  .carryover-track {
    flex: 0 0 40px;
    height: 24px;
    border-radius: 16px;
    background: var(--muted);
    padding: 3px;
    box-sizing: border-box;
  }
  .carryover-track::after {
    content: "";
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--surface);
  }
  .carryover-toggle input:checked + .carryover-track {
    background: var(--accent);
  }
  .carryover-toggle input:checked + .carryover-track::after {
    transform: translateX(16px);
  }
  .carryover-toggle input:focus-visible + .carryover-track {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  .carryover-toggle input:disabled + .carryover-track {
    opacity: 0.5;
  }
  .group-name {
    min-width: 4rem;
    width: 100%;
    max-width: 16rem;
    border: 1px solid transparent;
    background: transparent;
    font: inherit;
    color: inherit;
    padding: 0.25rem;
  }
  .group-name:hover,
  .group-name:focus {
    border-color: var(--line);
    background: var(--surface);
  }
  .drop-before > th,
  .drop-before > td {
    box-shadow: inset 0 2px var(--accent, #397c8c);
  }
  .drop-after > th,
  .drop-after > td {
    box-shadow: inset 0 -2px var(--accent, #397c8c);
  }

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
  .category-name {
    flex: 1;
    text-overflow: ellipsis;
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
    flex: 0 0 24px;
    justify-content: center;
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
  .group-toggle svg {
    flex-shrink: 0;
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
  .month-controls {
    align-items: center;
  }
  .month-controls label {
    display: flex;
    margin: 0;
  }
  .months-count select {
    margin: 0;
  }
  .page-heading,
  .page-heading h1 {
    margin-bottom: 0;
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
  .editor-tools {
    gap: 0.4rem;
  }
  .plan-form {
    border-top: 1px solid var(--border);
    padding: 1.5rem 0;
    margin-top: 1rem;
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
