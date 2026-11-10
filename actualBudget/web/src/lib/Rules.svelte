<script lang="ts">
  import { onMount } from "svelte";
  import { request, uuid, errorMessage, type Account } from "./api";
  import {
    references,
    registerPage,
    mutate,
    requestCancellation,
    type Reference,
    type PendingMutation,
    type Transaction,
  } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import { captureAmountFormat, formatAmount } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    rulesKey,
    readRulesDraft,
    conditionFields,
    opLabels,
    operators,
    newCondition,
    type Rule,
    type RuleDraft,
    type RulesView,
    type RulePreview,
    type RuleProjection,
  } from "./rules";
  import RegisterCondition from "./RegisterCondition.svelte";
  import { defaultQuery } from "./register-query";
  import RuleAction from "./RuleAction.svelte";
  import type { SchedulePage } from "./schedules";
  let {
    budgetId,
    disconnected,
    initialPayee = "",
    onschedules,
    onclose,
    onchanged,
  }: {
    budgetId: string;
    disconnected: boolean;
    initialPayee?: string;
    onschedules: () => void;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  let view = $state.raw<RulesView | null>(null),
    draft = $state<RuleDraft | null>(null),
    preview = $state.raw<RulePreview | null>(null);
  let accounts = $state.raw<Account[]>([]),
    payees = $state.raw<Reference[]>([]),
    categories = $state.raw<Reference[]>([]),
    groups = $state.raw<Reference[]>([]);
  let schedules = $state.raw<Reference[]>([]),
    scheduleNext = $state<number | null>(null);
  let selectedRules = $state<string[]>([]),
    selected = $state<string[]>([]),
    payeeFilter = $state(""),
    accountFilter = $state(""),
    scopeSearch = $state(""),
    scopeIds = $state<string[]>([]),
    scopeRows = $state.raw<Transaction[]>([]),
    scopeOffset = $state(0),
    scopeNext = $state<number | null>(null),
    busy = $state(false),
    error = $state(""),
    notice = $state(""),
    storageBroken = $state(false),
    renameNames = $state("");
  let allocationLimit = $state<Record<string, number>>({});
  let checkpoint: string | null = null;
  let locked = $derived(
    busy || disconnected || storageBroken || !!draft?.pending,
  );
  let visible = $derived(
    (view?.data ?? []).filter(
      (r) =>
        !payeeFilter ||
        [...r.conditions, ...r.actions].some(
          (c) =>
            c.field === "payee" &&
            (c.value === payeeFilter ||
              (Array.isArray(c.value) && c.value.includes(payeeFilter))),
        ),
    ),
  );
  function empty(): Rule {
    return {
      id: uuid(),
      stage: null,
      conditionsOp: "and",
      conditions: [newCondition()],
      actions: [{ op: "set", field: "category", value: null }],
    };
  }
  function save(next: RuleDraft | null) {
    if (localStorage.getItem(rulesKey(budgetId)) !== checkpoint)
      throw new Error(
        "Another tab changed this rule draft. Reload to review its saved work.",
      );
    const raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(rulesKey(budgetId));
    else localStorage.setItem(rulesKey(budgetId), raw);
    checkpoint = raw;
    draft = next;
  }
  function edit(rule?: Rule) {
    if (draft) {
      error = "Finish or discard the saved rule work first.";
      return;
    }
    if (!view) return;
    try {
      const r = structuredClone(rule ?? empty());
      save({
        version: 1,
        epoch: view.epoch,
        rule: r,
        existing: !!rule,
        original: rule ? structuredClone(rule) : null,
        format: captureAmountFormat(),
        pending: null,
        intent: "edit",
      });
      preview = null;
      selected = [];
      error = "";
      notice = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function changeRule(update: Partial<Rule>) {
    if (!draft || locked) return;
    try {
      save({
        ...$state.snapshot(draft),
        rule: { ...$state.snapshot(draft.rule), ...update },
      });
      preview = null;
      selected = [];
      error = "";
      notice = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function moreSchedules() {
    if (scheduleNext === null || !view) return;
    busy = true;
    try {
      const s = (
        await request<{ data: SchedulePage }>(
          `/budgets/${budgetId}/schedules?limit=100&offset=${scheduleNext}&snapshot=${view.snapshot}`,
        )
      ).data;
      schedules = [
        ...schedules,
        ...s.schedules.map((row) => ({
          id: row.id,
          name: row.name ?? "Unnamed schedule",
        })),
      ];
      scheduleNext = s.next_offset;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function refresh() {
    if (disconnected) {
      error =
        "Reconnect to load confirmed rules. Saved work stays on this device.";
      return;
    }
    busy = true;
    error = "";
    try {
      const loaded = await request<RulesView>(`/budgets/${budgetId}/rules`);
      const [a, p, c, g, s] = await Promise.all([
        request<{ data: Account[] }>(
          `/budgets/${budgetId}/accounts?snapshot=${loaded.snapshot}`,
        ),
        references(budgetId, "payees", { includeTransfers: true }),
        references(budgetId, "categories"),
        references(budgetId, "category-groups"),
        request<{ data: SchedulePage }>(
          `/budgets/${budgetId}/schedules?limit=100&snapshot=${loaded.snapshot}`,
        ),
      ]);
      view = loaded;
      accounts = a.data;
      payees = p;
      categories = c;
      groups = g;
      schedules = s.data.schedules.map((row) => ({
        id: row.id,
        name: row.name ?? "Unnamed schedule",
      }));
      scheduleNext = s.data.next_offset;
      observeVersions({ rules: loaded.edit_clock });
      preview = null;
      selected = [];
      selectedRules = selectedRules.filter((id) =>
        loaded.data.some((r) => r.id === id),
      );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    payeeFilter = initialPayee;
    try {
      draft = readRulesDraft(budgetId);
      checkpoint = localStorage.getItem(rulesKey(budgetId));
    } catch (e) {
      error = errorMessage(e);
      storageBroken = true;
      return;
    }
    void refresh();
  });
  function discard() {
    if (!draft || draft.pending || busy) return;
    try {
      save(null);
      preview = null;
      selected = [];
      error = "";
      notice = "Rule draft discarded.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function dispatch(
    path?: string,
    body?: unknown,
    method: "POST" | "PATCH" | "DELETE" = "POST",
    intent = "Rule change",
    cancel = false,
  ) {
    if (busy || storageBroken) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (disconnected)
        throw new Error(
          "Reconnect before saving rules. Your work is retained.",
        );
      if (!draft?.pending) {
        if (!view) throw new Error("Refresh the current rules first.");
        if (draft && draft.intent !== "edit")
          throw new Error("Review the retained request first.");
        const p: PendingMutation = {
          path: path!,
          method,
          body,
          key: uuid(),
          edit_clock: nextEditClock(),
          historyOwner: await historyOwner(),
          budgetEpoch: view.epoch,
        };
        save(
          draft
            ? { ...$state.snapshot(draft), pending: p }
            : {
                version: 1,
                epoch: view.epoch,
                rule: empty(),
                original: null,
                existing: false,
                format: captureAmountFormat(),
                pending: p,
                intent,
              },
        );
      }
      if (cancel) requestCancellation(draft!.pending!);
      save($state.snapshot(draft));
      try {
        await mutate(draft!.pending!, cancel);
      } catch (e) {
        if (
          e instanceof Error &&
          "code" in e &&
          e.code === "operation_canceled"
        ) {
          resetCorrectedClock();
          if (draft!.intent === "edit")
            save({ ...$state.snapshot(draft!), pending: null });
          else save(null);
          notice =
            "Saved request canceled. Retained fields can be reviewed before a new request.";
          return;
        }
        throw e;
      }
      save(null);
      notice = cancel
        ? "The original request was already saved."
        : `${intent} saved.`;
      await refresh();
      await onchanged();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function submit() {
    if (!draft || draft.pending || !view) return;
    error = "";
    notice = "";
    try {
      if (view.epoch !== draft.epoch)
        throw new Error(
          "This budget was restored. Keep needed values, discard this draft and reopen current rules.",
        );
      const body: Record<string, unknown> = { snapshot: view.snapshot };
      if (draft.existing) {
        const current = view.data.find((r) => r.id === draft!.rule.id);
        if (!current || !draft.original)
          throw new Error(
            "This rule was removed. Keep needed values and reopen current rules.",
          );
        for (const field of [
          "stage",
          "conditionsOp",
          "conditions",
          "actions",
        ] as const) {
          if (
            JSON.stringify(draft.rule[field]) !==
            JSON.stringify(draft.original[field])
          ) {
            if (
              JSON.stringify(current[field]) !==
              JSON.stringify(draft.original[field])
            )
              throw new Error(
                `The rule ${field} changed elsewhere. Keep needed values, discard this draft and reopen the current rule.`,
              );
            body[field] = draft.rule[field];
          }
        }
        if (Object.keys(body).length === 1)
          throw new Error("Change a rule field before saving.");
      } else Object.assign(body, $state.snapshot(draft.rule));
      await dispatch(
        `/budgets/${budgetId}/rules${draft.existing ? "/" + draft.rule.id : ""}`,
        body,
        draft.existing ? "PATCH" : "POST",
        "Rule",
      );
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function resetScope() {
    scopeIds = [];
    scopeRows = [];
    scopeOffset = 0;
    scopeNext = null;
    preview = null;
    selected = [];
  }
  async function loadScope(offset = 0) {
    if (!view || locked) return;
    busy = true;
    error = "";
    try {
      const query = defaultQuery();
      query.search = scopeSearch;
      const page = await registerPage(
        budgetId,
        accountFilter || null,
        offset,
        "",
        view.snapshot,
        query,
      );
      scopeRows = page.data;
      scopeOffset = offset;
      scopeNext = page.next_offset;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function scopeChoose(id: string, checked: boolean) {
    if (checked && scopeIds.length >= 100) {
      error = "Choose at most 100 transactions for one preview.";
      return;
    }
    scopeIds = checked ? [...scopeIds, id] : scopeIds.filter((v) => v !== id);
    preview = null;
    selected = [];
    error = "";
  }
  function previewAmount(amount: number | null): string {
    if (preferences.shared.privacy_mode) return "••••";
    return amount !== null && Number.isSafeInteger(amount)
      ? formatAmount(amount)
      : amount === null
        ? "Invalid amount"
        : `${amount} minor units`;
  }
  function invalidAmounts(row: RuleProjection): boolean {
    return (
      !row.deleted &&
      (!Number.isSafeInteger(row.amount) ||
        !!row.children?.some(invalidAmounts))
    );
  }
  function diagnostics(row: RuleProjection): string[] {
    return [
      ...(row.rule_errors ?? []),
      ...(row.children ?? []).flatMap((c, i) =>
        (c.rule_errors ?? []).map((error) => `Allocation ${i + 1}: ${error}`),
      ),
    ];
  }
  function fieldValue(
    field: string,
    row: Transaction | RuleProjection | undefined,
  ): string {
    if (!row) return "Not present";
    switch (field) {
      case "amount":
        return previewAmount(row.amount);
      case "account_id":
        return (
          accounts.find((a) => a.id === row.account_id)?.name ??
          row.account_name ??
          "Unavailable account"
        );
      case "payee_id":
        return row.payee_id
          ? (payees.find((p) => p.id === row.payee_id)?.name ??
              (row as Transaction & { new_payee?: Reference }).new_payee
                ?.name ??
              row.payee_name ??
              "Unavailable payee")
          : "None";
      case "category_id":
        return row.category_id
          ? (categories.find((c) => c.id === row.category_id)?.name ??
              row.category_name ??
              "Unavailable category")
          : "None";
      case "cleared":
        return row.cleared ? "Yes" : "No";
      case "notes":
        return row.notes || "None";
      case "date":
        return row.date;
      default:
        return "";
    }
  }
  function changes(
    before: Transaction | undefined,
    after: RuleProjection | undefined,
  ) {
    return Object.entries({
      amount: "Amount",
      date: "Date",
      account_id: "Account",
      payee_id: "Payee",
      category_id: "Category",
      notes: "Notes",
      cleared: "Cleared",
    })
      .filter(
        ([field]) =>
          !before ||
          !after ||
          before[field as keyof Transaction] !==
            after[field as keyof Transaction],
      )
      .map(([field, label]) => ({
        field,
        label,
        before: fieldValue(field, before),
        after: fieldValue(field, after),
      }));
  }
  function allocations(before: Transaction, after: RuleProjection) {
    const old = before.children ?? [],
      next = after.deleted
        ? []
        : (after.children ?? []).map((row) => ({
            ...row,
            account_id: after.account_id,
            date: after.date,
            cleared: after.cleared,
          }));
    const oldById = new Map(old.map((row) => [row.id, row]));
    const nextIds = new Set(next.map((row) => row.id));
    return [
      ...next.map((row, index) => ({
        id: row.id,
        index,
        before: oldById.get(row.id),
        after: row,
      })),
      ...old
        .filter((row) => !nextIds.has(row.id))
        .map((row, index) => ({
          id: row.id,
          index: next.length + index,
          before: row,
          after: undefined,
        })),
    ];
  }
  async function runPreview(offset = 0) {
    if (!view || locked) return;
    busy = true;
    error = "";
    notice = "";
    try {
      const body = {
        snapshot: view.snapshot,
        offset,
        limit: 100,
        ...(scopeIds.length
          ? { ids: $state.snapshot(scopeIds) }
          : accountFilter
            ? { account_id: accountFilter }
            : {}),
        ...(draft?.intent === "edit"
          ? { rule: $state.snapshot(draft.rule) }
          : selectedRules.length
            ? { rule_ids: $state.snapshot(selectedRules) }
            : {}),
      };
      const result = await request<{ data: RulePreview }>(
        `/budgets/${budgetId}/rules/preview`,
        body,
      );
      if (offset && preview && preview.snapshot === result.data.snapshot)
        preview = {
          ...result.data,
          transactions: [...preview.transactions, ...result.data.transactions],
        };
      else {
        preview = result.data;
        selected = [];
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function choose(id: string, checked: boolean) {
    if (checked && selected.length >= 100) {
      error = "Apply at most 100 selected transactions per command.";
      return;
    }
    selected = checked ? [...selected, id] : selected.filter((v) => v !== id);
    error = "";
  }
  async function apply() {
    if (!preview || !selected.length || draft) return;
    await dispatch(
      `/budgets/${budgetId}/rules/apply`,
      {
        snapshot: preview.snapshot,
        ids: $state.snapshot(selected),
        ...(selectedRules.length
          ? { rule_ids: $state.snapshot(selectedRules) }
          : {}),
      },
      "POST",
      "Selected rule actions",
    );
  }
  function settings(enabled: boolean, payee_id?: string) {
    if (!view || draft) return;
    void dispatch(
      `/budgets/${budgetId}/rules/learning`,
      { snapshot: view.snapshot, enabled, ...(payee_id ? { payee_id } : {}) },
      "POST",
      "Learning setting",
    );
  }
  function remove(rule: Rule) {
    if (!view || draft) return;
    void dispatch(
      `/budgets/${budgetId}/rules/${rule.id}`,
      { snapshot: view.snapshot },
      "DELETE",
      "Rule deletion",
    );
  }
  function rename() {
    if (!view || draft || !payeeFilter) return;
    void dispatch(
      `/budgets/${budgetId}/rules/rename-payee`,
      {
        snapshot: view.snapshot,
        payee_id: payeeFilter,
        imported_payees: renameNames.split("\n"),
      },
      "POST",
      "Rename rule",
    );
  }
</script>

<section class="rules">
  <header>
    <div>
      <h1>Rules and category learning</h1>
      <p class="hint">
        Run pre rules, default rules, then post rules. More specific rules run
        later within each stage.
      </p>
    </div>
    <button onclick={onclose} disabled={busy}>Close</button>
  </header>
  {#if error}<p role="alert" class="error">{error}</p>{/if}{#if notice}<p
      role="status"
    >
      {notice}
    </p>{/if}
  <div class="toolbar">
    <button onclick={refresh} disabled={busy || disconnected}
      >Refresh rules</button
    ><button
      class="primary"
      onclick={() => edit()}
      disabled={locked || !!draft || !view}>Create rule</button
    >
  </div>
  {#if draft?.pending}<section class="retained" aria-label="Saved rule request">
      <h2>Unconfirmed request</h2>
      <p>
        {draft.intent === "edit" ? "Rule change" : draft.intent}. Its original
        payload, clock and history owner are saved on this device.
      </p>
      <div class="toolbar">
        <button
          class="primary"
          onclick={() =>
            dispatch(
              undefined,
              undefined,
              "POST",
              draft?.intent === "edit"
                ? "Rule"
                : (draft?.intent ?? "Rule change"),
            )}
          disabled={busy || disconnected}>Retry saved request</button
        ><button
          onclick={() =>
            dispatch(undefined, undefined, "POST", "Rule change", true)}
          disabled={busy || disconnected}>Cancel if not saved</button
        >
      </div>
    </section>{/if}
  {#if view}
    <section aria-labelledby="learning-heading">
      <h2 id="learning-heading">Category learning</h2>
      <label class="inline"
        ><input
          type="checkbox"
          checked={view.learning_enabled}
          disabled={locked || !!draft}
          onchange={(e) => settings(e.currentTarget.checked)}
        />Learn categories from explicit manual categorization</label
      >
      <p class="hint">
        Uses recent transactions when a category has a clear majority. Existing
        defaults remain when learning is off. Rule-generated categories never
        train defaults. Expenses recorded in actualExpenses do not change these
        defaults.
      </p>
      <label
        >Associated payee<select bind:value={payeeFilter} disabled={busy}
          ><option value="">All payees</option
          >{#each payees.filter((p) => !p.transfer_account_id) as payee (payee.id)}<option
              value={payee.id}>{payee.name}</option
            >{/each}</select
        ></label
      >
      {#if payeeFilter}{@const payee = payees.find(
          (p) => p.id === payeeFilter,
        )}<label class="inline"
          ><input
            type="checkbox"
            checked={payee?.learn_categories !== false}
            disabled={locked || !!draft}
            onchange={(e) => settings(e.currentTarget.checked, payeeFilter)}
          />Allow category learning for {payee?.name ?? "this payee"}</label
        >
        <details>
          <summary>Automatically rename imported payees</summary><label
            >Imported payee names<textarea
              bind:value={renameNames}
              disabled={locked || !!draft}></textarea><span class="hint"
              >One exact imported name per line. Adds names to this payee's
              existing pre-stage rename rule.</span
            ></label
          ><button
            onclick={rename}
            disabled={locked || !!draft || !renameNames.trim()}
            >Save rename rule</button
          >
        </details>{/if}
    </section>
    {#if draft?.intent === "edit"}<form
        onsubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <fieldset disabled={locked}>
          <legend>{draft.existing ? "Edit rule" : "Create rule"}</legend>
          <div class="builder">
            <label
              >Stage<select
                value={draft.rule.stage ?? ""}
                onchange={(e) =>
                  changeRule({
                    stage: (e.currentTarget.value || null) as Rule["stage"],
                  })}
                ><option value="pre">Pre</option><option value=""
                  >Default</option
                ><option value="post">Post</option></select
              ></label
            ><label
              >Match conditions<select
                value={draft.rule.conditionsOp}
                onchange={(e) =>
                  changeRule({
                    conditionsOp: e.currentTarget.value as "and" | "or",
                  })}
                ><option value="and">All conditions</option><option value="or"
                  >Any condition</option
                ></select
              ></label
            >
          </div>
          <h3>Conditions</h3>
          {#each draft.rule.conditions as condition, index (index)}<div
              class="builder condition"
            >
              <label
                >Field {index + 1}<select
                  value={condition.field}
                  onchange={(e) =>
                    changeRule({
                      conditions: draft!.rule.conditions.map((c, i) =>
                        i === index ? newCondition(e.currentTarget.value) : c,
                      ),
                    })}
                  >{#each Object.entries(conditionFields) as [value, label] (value)}<option
                      {value}>{label}</option
                    >{/each}</select
                ></label
              ><label
                >Operator {index + 1}<select
                  value={condition.op}
                  onchange={(e) =>
                    changeRule({
                      conditions: draft!.rule.conditions.map((c, i) =>
                        i === index
                          ? newCondition(c.field, e.currentTarget.value)
                          : c,
                      ),
                    })}
                  >{#each operators(condition.field) as op (op)}<option
                      value={op}>{opLabels[op]}</option
                    >{/each}</select
                ></label
              ><RegisterCondition
                {condition}
                {index}
                {accounts}
                {payees}
                {categories}
                {groups}
                rules
                amountFormat={draft.format}
                onchange={(c) =>
                  changeRule({
                    conditions: draft!.rule.conditions.map((old, i) =>
                      i === index ? c : old,
                    ),
                  })}
              /><button
                type="button"
                onclick={() =>
                  changeRule({
                    conditions: draft!.rule.conditions.filter(
                      (_, i) => i !== index,
                    ),
                  })}>Remove condition {index + 1}</button
              >
            </div>{/each}<button
            type="button"
            onclick={() =>
              changeRule({
                conditions: [...draft!.rule.conditions, newCondition()],
              })}
            disabled={draft.rule.conditions.length >= 32}>Add condition</button
          >
          <p class="hint">A rule with no conditions never matches.</p>
          <h3>Actions</h3>
          {#each draft.rule.actions as action, index (index)}<div
              class="builder action"
            >
              <RuleAction
                {action}
                {index}
                {accounts}
                {payees}
                {categories}
                {schedules}
                onmoreschedules={scheduleNext === null
                  ? undefined
                  : moreSchedules}
                format={draft.format}
                onchange={(a) =>
                  changeRule({
                    actions: draft!.rule.actions.map((old, i) =>
                      i === index ? a : old,
                    ),
                  })}
              /><button
                type="button"
                onclick={() =>
                  changeRule({
                    actions: draft!.rule.actions.filter((_, i) => i !== index),
                  })}>Remove action {index + 1}</button
              >
            </div>{/each}<button
            type="button"
            onclick={() =>
              changeRule({
                actions: [
                  ...draft!.rule.actions,
                  { op: "set", field: "category", value: null },
                ],
              })}
            disabled={draft.rule.actions.length >= 64}>Add action</button
          >
          <div class="toolbar">
            <button class="primary">Save rule</button><button
              type="button"
              onclick={() => runPreview()}>Preview draft rule</button
            ><button type="button" onclick={discard}>Discard draft</button>
          </div>
        </fieldset>
      </form>{/if}
    <section aria-labelledby="scope-heading">
      <h2 id="scope-heading">Transactions to preview</h2>
      <label
        >Preview account<select
          bind:value={accountFilter}
          disabled={locked}
          onchange={resetScope}
          ><option value="">All accounts</option
          >{#each accounts as account (account.id)}<option value={account.id}
              >{account.name}</option
            >{/each}</select
        ></label
      >
      <p class="hint">
        Preview up to 10,000 candidates in an account, or choose up to 100
        transactions below. Search and page through the register for larger
        accounts.
      </p>
      <div class="toolbar">
        <label
          >Find transactions<input
            bind:value={scopeSearch}
            disabled={locked}
          /></label
        ><button onclick={() => loadScope()} disabled={locked}
          >Find transactions</button
        ><button onclick={resetScope} disabled={locked || !scopeIds.length}
          >Clear transaction selection</button
        >
      </div>
      <p role="status">
        {scopeIds.length
          ? `${scopeIds.length} transactions chosen for preview`
          : "Preview uses all candidates in the chosen account."}
      </p>
      {#if scopeRows.length}<ul class="scope-list">
          {#each scopeRows as row (row.id)}<li>
              <label class="inline"
                ><input
                  type="checkbox"
                  checked={scopeIds.includes(row.id)}
                  disabled={locked}
                  onchange={(e) => scopeChoose(row.id, e.currentTarget.checked)}
                /><span
                  >{row.date} · {row.payee_name ?? row.account_name} · {formatAmount(
                    row.amount,
                  )}<small>{row.notes || "No notes"}</small></span
                ></label
              >
            </li>{/each}
        </ul>{/if}
      {#if scopeRows.length || scopeOffset}<div class="toolbar">
          <button
            onclick={() => loadScope(Math.max(0, scopeOffset - 100))}
            disabled={locked || !scopeOffset}>Previous transactions</button
          ><button
            onclick={() => loadScope(scopeNext!)}
            disabled={locked || scopeNext === null}>Next transactions</button
          >
        </div>{/if}
    </section>
    <section aria-labelledby="saved-rules">
      <h2 id="saved-rules">
        {payeeFilter ? "Associated rules" : "Saved rules"} ({visible.length})
      </h2>
      <p>
        Rules attached to a recurring transaction are edited through its
        schedule.
      </p>
      <button onclick={onschedules}>Manage scheduled rules</button>
      {#if visible.length}<ul class="rule-list">
          {#each visible as rule (rule.id)}<li data-rule-id={rule.id}>
              <label class="inline"
                ><input
                  type="checkbox"
                  checked={selectedRules.includes(rule.id)}
                  disabled={locked || !!draft}
                  onchange={(e) => {
                    selectedRules = e.currentTarget.checked
                      ? [...selectedRules, rule.id]
                      : selectedRules.filter((id) => id !== rule.id);
                    preview = null;
                    selected = [];
                  }}
                /><span
                  ><strong
                    >{rule.stage === "pre"
                      ? "Pre"
                      : rule.stage === "post"
                        ? "Post"
                        : "Default"}</strong
                  >
                  · {rule.conditions.length}
                  {rule.conditions.length === 1 ? "condition" : "conditions"} · {rule
                    .actions.length}
                  {rule.actions.length === 1 ? "action" : "actions"}<small
                    >{rule.conditions
                      .map(
                        (c) =>
                          (conditionFields[c.field] ?? c.field) +
                          " " +
                          (opLabels[c.op] ?? c.op),
                      )
                      .join(
                        rule.conditionsOp === "and" ? " and " : " or ",
                      )}</small
                  ></span
                ></label
              >
              <div class="toolbar">
                <button onclick={() => edit(rule)} disabled={locked || !!draft}
                  >Edit rule</button
                ><button
                  onclick={() => remove(rule)}
                  disabled={locked || !!draft}>Delete rule</button
                >
              </div>
            </li>{/each}
        </ul>{:else}<p>No saved rules match this view.</p>{/if}<button
        onclick={() => runPreview()}
        disabled={locked || !!draft || !view.data.length}
        >Preview {selectedRules.length ? "selected" : "all"} saved rules</button
      >
    </section>
  {/if}
  {#if preview}<section aria-labelledby="preview-heading">
      <h2 id="preview-heading">Rule preview</h2>
      <p>
        {preview.total_count} matching transactions from {preview.candidate_count}
        candidates. No changes saved.
      </p>
      {#if draft}<p class="hint">
          Save the rule, then preview saved rules to apply selected
          transactions.
        </p>{/if}{#if preview.protected_count}<p role="status">
          {preview.protected_count} affected reconciled entries. Unlock them, then
          preview again before applying those entries. You can select unrelated unlocked
          entries.
        </p>{/if}
      <ul class="preview-list">
        {#each preview.transactions as entry (entry.before.id)}<li>
            <label class="inline"
              ><input
                type="checkbox"
                checked={selected.includes(entry.before.id)}
                disabled={locked ||
                  !!draft ||
                  entry.before.reconciled ||
                  invalidAmounts(entry.after)}
                onchange={(e) =>
                  choose(entry.before.id, e.currentTarget.checked)}
              /><span
                ><strong
                  >{entry.before.date} · {entry.before.payee_name ??
                    entry.before.account_name}</strong
                >
                <small
                  >Current amount: {formatAmount(
                    entry.before.amount,
                  )}{#if entry.before.notes}
                    · {entry.before.notes}{/if}</small
                >
                {#if entry.after.deleted}<strong>Delete transaction</strong
                  >{/if}
                {#if invalidAmounts(entry.after)}<strong
                    >Cannot save: amounts must be whole minor units.</strong
                  >{/if}
                {#if diagnostics(entry.after).length}
                  <strong
                    >Some calculated actions failed. Their previous values
                    remain; later actions may still apply.</strong
                  >
                  {#each [...new Set(diagnostics(entry.after))] as message (message)}<small
                      class="field-change">{message}</small
                    >{/each}
                {/if}
                {#if !!entry.before.is_parent !== !!entry.after.is_parent}<small
                    >Structure: {entry.before.is_parent
                      ? "Split transaction"
                      : "Ordinary transaction"} → {entry.after.is_parent
                      ? "Split transaction"
                      : "Ordinary transaction"}</small
                  >{/if}
                {#each changes(entry.before, entry.after) as change (change.field)}<small
                    class="field-change"
                    >{change.label}: {change.before} → {change.after}</small
                  >{/each}
                {#if !entry.after.deleted && !changes(entry.before, entry.after).length && !allocations(entry.before, entry.after).length}<small
                    >No field changes</small
                  >{/if}
                {#if entry.before.reconciled}<small
                    >Reconciled: unlock before applying.</small
                  >{/if}
              </span></label
            >
            {#if entry.before.children?.length || entry.after.children?.length}<section
                class="allocations"
                aria-label="Split allocation changes"
              >
                <h3>Split allocations</h3>
                <p>
                  Showing {Math.min(
                    allocationLimit[entry.before.id] ?? 20,
                    allocations(entry.before, entry.after).length,
                  )} of {allocations(entry.before, entry.after).length} allocations.
                </p>
                <ol>
                  {#each allocations(entry.before, entry.after).slice(0, allocationLimit[entry.before.id] ?? 20) as allocation (allocation.id)}<li
                    >
                      <strong
                        >Allocation {allocation.index + 1} · {allocation.before
                          ? allocation.after
                            ? "Retained"
                            : "Removed"
                          : "Added"}</strong
                      >{#each changes(allocation.before, allocation.after) as change (change.field)}<small
                          class="field-change"
                          >{change.label}: {change.before} → {change.after}</small
                        >{/each}{#if allocation.before && allocation.after && !changes(allocation.before, allocation.after).length}<small
                          >Unchanged · {previewAmount(
                            allocation.after.amount,
                          )}</small
                        >{/if}
                    </li>{/each}
                </ol>
                {#if (allocationLimit[entry.before.id] ?? 20) < allocations(entry.before, entry.after).length}<button
                    onclick={() =>
                      (allocationLimit[entry.before.id] =
                        (allocationLimit[entry.before.id] ?? 20) + 20)}
                    >Show more allocation changes</button
                  >{/if}
              </section>{/if}
          </li>{/each}
      </ul>
      <div class="toolbar">
        {#if preview.next_offset !== null}<button
            onclick={() => runPreview(preview!.next_offset!)}
            disabled={locked}>Load more matches</button
          >{/if}<button
          class="primary"
          onclick={apply}
          disabled={locked || !!draft || !selected.length}
          >Apply rules to {selected.length} selected</button
        >
      </div>
    </section>{/if}
</section>

<style>
  .rules {
    max-width: 70rem;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    justify-content: space-between;
    gap: 1rem;
  }
  header > div {
    flex: 1 1 24rem;
    min-width: 0;
  }
  @media (max-width: 40rem) {
    header {
      flex-direction: column;
    }
    header > div {
      flex-basis: auto;
      width: 100%;
    }
  }
  .field-change {
    white-space: pre-wrap;
  }
  .scope-list {
    list-style: none;
    padding: 0;
  }
  .scope-list li {
    padding-block: 0.75rem;
    border-bottom: 1px solid var(--line);
  }
  .allocations {
    margin-block: 0.75rem;
    padding-inline-start: 2rem;
  }
  .allocations li {
    padding-block: 0.5rem;
  }
  section,
  form {
    margin-block: 1.5rem;
  }
  label {
    display: grid;
    gap: 0.3rem;
    min-width: 0;
  }
  .toolbar,
  .builder {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: start;
  }
  .builder > button {
    align-self: center;
  }
  .builder > label {
    flex: 1 1 10rem;
  }
  .condition,
  .action {
    padding-block: 1rem;
    border-bottom: 1px solid var(--line);
  }
  .toolbar {
    margin-block: 1rem;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .inline input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
  }
  .rule-list,
  .preview-list {
    list-style: none;
    padding: 0;
  }
  .rule-list li,
  .preview-list li {
    border-bottom: 1px solid var(--line);
    padding-block: 1rem;
  }
  .rule-list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  small {
    display: block;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .retained {
    border-inline-start: 3px solid var(--accent);
    padding-inline-start: 1rem;
  }
  fieldset {
    min-width: 0;
  }
  input,
  select,
  textarea {
    max-width: 100%;
    min-width: 0;
  }
  summary {
    cursor: pointer;
    padding-block: 1rem;
  }
</style>
