<script lang="ts">
  import { references, type Reference } from "./register-api";
  import RegisterCondition from "./RegisterCondition.svelte";
  import { onMount } from "svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import {
    defaultQuery,
    defaultColumns,
    columnLabels,
    type RegisterQuery,
    type RegisterSettings,
    type SettingsResponse,
    type Condition,
  } from "./register-query";
  let {
    budgetId,
    accountId,
    disconnected,
    accounts,
    payees,
    categories,
    cutoff,
    onapply,
  }: {
    budgetId: string;
    accountId: string | null;
    disconnected: boolean;
    accounts: Reference[];
    payees: Reference[];
    categories: Reference[];
    cutoff: string;
    onapply: (
      query: RegisterQuery,
      columns: string[],
      date: string,
      extraBalances?: boolean,
    ) => Promise<unknown>;
  } = $props();
  let groups = $state.raw<Reference[]>([]);
  let groupsLoaded = false;
  async function loadGroups() {
    if (disconnected || groupsLoaded) return;
    try {
      groups = await references(budgetId, "category-groups");
      groupsLoaded = true;
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let query = $state<RegisterQuery>(defaultQuery());
  let columns = $state<string[]>([...defaultColumns]);
  let extraBalances = $state(true);
  let settings = $state.raw<SettingsResponse | null>(null);
  let busy = $state(false),
    error = $state(""),
    notice = $state(""),
    filterName = $state(""),
    selected = $state("");
  type Pending = {
    key: string;
    budgetId: string;
    epoch: string;
    body: { revision: number; settings: RegisterSettings };
  };
  let pending = $state.raw<Pending | null>(null),
    rejected = $state.raw<Pending | null>(null);
  const prefix = "actualBudget.register-settings.v1.";
  let alive = true;
  function recover() {
    const found = Object.keys(localStorage)
      .filter((k) => k.startsWith(prefix + budgetId + "."))
      .sort();
    pending = null;
    if (found[0]) {
      const value = JSON.parse(localStorage.getItem(found[0])!);
      if (
        value.budgetId !== budgetId ||
        typeof value.key !== "string" ||
        found[0] !== prefix + budgetId + "." + value.key ||
        !/^[a-f0-9]{32}$/.test(value.epoch) ||
        !Number.isSafeInteger(value.body?.revision) ||
        !value.body?.settings?.views ||
        !Array.isArray(value.body?.settings?.saved_filters)
      )
        throw new Error(
          "Saved register request could not be validated. Keep device data and retry recovery.",
        );
      pending = value;
    }
  }
  async function load(apply = true, preserveDraft = false) {
    busy = true;
    error = "";
    try {
      recover();
      if (disconnected) return;
      const result = await request<{ data: SettingsResponse }>(
        `/budgets/${budgetId}/register-settings`,
      );
      if (!alive) return;
      settings = result.data;
      const view = result.data.settings.views[scope()];
      if (view && !preserveDraft) {
        query = structuredClone(view.query);
        columns = [...view.columns];
        extraBalances = view.show_extra_balances ?? true;
      }
      if (apply && view && !preserveDraft)
        await onapply(
          $state.snapshot(query),
          [...columns],
          view?.as_of ?? cutoff,
          extraBalances,
        );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      if (alive) busy = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      alive = false;
    };
  });
  async function apply() {
    await onapply($state.snapshot(query), [...columns], cutoff, extraBalances);
  }
  function removePending(operation: Pending) {
    const key = prefix + budgetId + "." + operation.key;
    if (localStorage.getItem(key) === JSON.stringify(operation))
      localStorage.removeItem(key);
    recover();
  }
  function scope() {
    return accountId ?? query.account_scope ?? "all";
  }
  function changeScope() {
    const selectedScope = query.account_scope ?? "all";
    const view = settings?.settings.views[selectedScope];
    query = view ? structuredClone(view.query) : defaultQuery();
    query.account_scope = selectedScope;
    columns = view ? [...view.columns] : [...defaultColumns];
    extraBalances = view?.show_extra_balances ?? true;
    void onapply(
      $state.snapshot(query),
      [...columns],
      view?.as_of ?? "",
      extraBalances,
    );
  }
  async function save(kind: "view" | "filter" | "update" | "delete" = "view") {
    if (disconnected || !settings || busy) return;
    busy = true;
    error = "";
    notice = "";
    let attempted: Pending | null = null;
    try {
      recover();
      if (!pending) {
        const next = structuredClone(settings.settings);
        next.views[scope()] = {
          query: $state.snapshot(query),
          columns: [...columns],
          as_of: cutoff,
          show_extra_balances: extraBalances,
        };
        if (kind === "filter") {
          if (!filterName.trim())
            throw new Error("Name this filter before saving.");
          next.saved_filters.push({
            id: uuid(),
            name: filterName.trim(),
            query: $state.snapshot(query),
          });
        }
        if (kind === "update") {
          const filter = next.saved_filters.find((f) => f.id === selected);
          if (filter) {
            filter.name = filterName.trim() || filter.name;
            filter.query = $state.snapshot(query);
          }
        }
        if (kind === "delete")
          next.saved_filters = next.saved_filters.filter(
            (f) => f.id !== selected,
          );
        const operation = {
          key: uuid(),
          budgetId,
          epoch: settings.epoch,
          body: { revision: settings.revision, settings: next },
        };
        localStorage.setItem(
          prefix + budgetId + "." + operation.key,
          JSON.stringify(operation),
        );
        pending = operation;
      }
      const operation = pending;
      attempted = operation;
      const response = await request<{ data: SettingsResponse }>(
        `/budgets/${budgetId}/register-settings`,
        operation.body,
        operation.key,
        "PATCH",
        undefined,
        false,
        operation.epoch,
      );
      removePending(operation);
      if (!alive) return;
      settings = response.data;
      rejected = null;
      filterName = "";
      notice = "Register settings saved.";
      const view = response.data.settings.views[scope()];
      if (view) {
        query = structuredClone(view.query);
        columns = [...view.columns];
        extraBalances = view.show_extra_balances ?? true;
        await onapply(
          $state.snapshot(query),
          [...columns],
          view.as_of,
          extraBalances,
        );
      }
    } catch (e) {
      error = errorMessage(e);
      rejected = e instanceof RequestError && !e.uncertain ? attempted : null;
    } finally {
      if (alive) busy = false;
    }
  }
  async function reloadRejected() {
    if (rejected) {
      removePending(rejected);
      rejected = null;
    }
    await load(false, true);
  }
  function choose() {
    const filter = settings?.settings.saved_filters.find(
      (f) => f.id === selected,
    );
    if (filter) {
      query = structuredClone(filter.query);
      filterName = filter.name;
      void apply();
    }
  }
  function addCondition() {
    query.filter.conditions.push({ field: "notes", op: "contains", value: "" });
  }
  function changeField(condition: Condition) {
    condition.op = ["amount", "date"].includes(condition.field)
      ? "gte"
      : [
            "cleared",
            "reconciled",
            "transfer",
            "parent",
            "account",
            "payee",
            "category",
            "category_group",
          ].includes(condition.field)
        ? "eq"
        : "contains";
    condition.value = ["cleared", "reconciled", "transfer", "parent"].includes(
      condition.field,
    )
      ? true
      : condition.field === "amount"
        ? 0
        : ["account", "payee", "category", "category_group"].includes(
              condition.field,
            )
          ? null
          : "";
  }
</script>

<svelte:window
  onstorage={(event) => {
    if (event.key?.startsWith(prefix + budgetId + ".")) {
      try {
        recover();
      } catch (e) {
        error = errorMessage(e);
      }
    }
  }}
/>
<section class="register-tools" aria-label="Find transactions">
  <form
    class="search-row"
    onsubmit={(event) => {
      event.preventDefault();
      void apply();
    }}
  >
    {#if !accountId}<label
        >Account scope<select
          bind:value={query.account_scope}
          disabled={busy || disconnected || !!pending}
          onchange={changeScope}
          ><option value="all">All accounts</option><option value="onbudget"
            >On budget</option
          ><option value="offbudget">Off budget</option><option value="closed"
            >Closed accounts</option
          ><option value="uncategorized">Uncategorized</option></select
        ></label
      >{/if}
    <label
      >Search transactions<input
        type="search"
        bind:value={query.search}
        maxlength="500"
        disabled={busy || disconnected || !!pending}
      /></label
    >
    <label
      >Sort by<select
        bind:value={query.sort}
        disabled={busy || disconnected || !!pending}
        ><option value="date">Date</option><option value="amount">Amount</option
        ><option value="payee">Payee</option><option value="category"
          >Category</option
        ><option value="account">Account</option><option value="notes"
          >Notes</option
        ><option value="cleared">Cleared</option><option value="reconciled"
          >Reconciled</option
        ></select
      ></label
    >
    <label
      >Order<select
        bind:value={query.direction}
        disabled={busy || disconnected || !!pending}
        ><option value="desc">Descending</option><option value="asc"
          >Ascending</option
        ></select
      ></label
    >
    <button disabled={busy || disconnected || !!pending}
      >Find transactions</button
    >
  </form>
  <details
    ontoggle={(event) => {
      if (event.currentTarget.open) void loadGroups();
    }}
  >
    <summary>Filters and columns</summary>
    <fieldset disabled={busy || disconnected || !!pending}>
      <legend>Visibility</legend>
      <label class="check"
        ><input type="checkbox" bind:checked={extraBalances} />Show cleared and
        uncleared totals</label
      >
      <label class="check"
        ><input type="checkbox" bind:checked={query.hide_cleared} />Hide cleared
        transactions</label
      >
      <label class="check"
        ><input type="checkbox" bind:checked={query.hide_reconciled} />Hide
        reconciled transactions</label
      >
    </fieldset>
    <fieldset disabled={busy || disconnected || !!pending}>
      <legend>Compound filter</legend>
      <label
        >Match<select bind:value={query.filter.op}
          ><option value="and">All conditions</option><option value="or"
            >Any condition</option
          ></select
        ></label
      >
      {#each query.filter.conditions as condition, index (condition)}
        <div class="condition">
          <label
            >Field {index + 1}<select
              bind:value={condition.field}
              onchange={() => changeField(condition)}
              >{#each ["notes", "amount", "date", "cleared", "reconciled", "account", "payee", "category", "category_group", "account_name", "payee_name", "category_name", "category_group_name", "transfer", "parent"] as field (field)}<option
                  value={field}>{field.replaceAll("_", " ")}</option
                >{/each}</select
            ></label
          >
          <label
            >Operator {index + 1}<select
              bind:value={condition.op}
              onchange={() => {
                if (condition.op === "isbetween")
                  condition.value = { num1: 0, num2: 0 };
                else if (condition.field === "amount") condition.value = 0;
                else if (["in", "not_in"].includes(condition.op))
                  condition.value = Array.isArray(condition.value)
                    ? condition.value
                    : [condition.value as string | null];
                else if (Array.isArray(condition.value))
                  condition.value = condition.value[0] ?? null;
              }}
              ><option value="eq">is</option><option value="ne">is not</option
              >{#if ["amount", "date"].includes(condition.field)}<option
                  value="lt">less than / before</option
                ><option value="lte">at most / through</option><option
                  value="gt">greater than / after</option
                ><option value="gte">at least / from</option><option
                  value="isapprox">approximately</option
                >{#if condition.field === "amount"}<option value="isbetween"
                    >between</option
                  >{/if}{:else if ["account", "payee", "category", "category_group"].includes(condition.field)}<option
                  value="in">is one of</option
                ><option value="not_in">is not one of</option
                >{#if condition.field === "account"}<option value="onBudget"
                    >is on budget</option
                  ><option value="offBudget">is off budget</option>{/if}
              {:else if !["cleared", "reconciled", "transfer", "parent"].includes(condition.field)}<option
                  value="contains">contains</option
                ><option value="not_contains">does not contain</option><option
                  value="matches">matches regular expression</option
                ><option value="in">is one of</option><option value="not_in"
                  >is not one of</option
                >{#if condition.field === "notes"}<option value="hasTags"
                    >has all tags</option
                  ><option value="hasAnyTag">has any tag</option
                  >{/if}{/if}</select
            ></label
          >
          <RegisterCondition
            {condition}
            {index}
            {accounts}
            {payees}
            {categories}
            {groups}
            onchange={(value) => Object.assign(condition, value)}
          />
          <button
            type="button"
            onclick={() => query.filter.conditions.splice(index, 1)}
            >Remove condition {index + 1}</button
          >
        </div>
      {/each}
      <button
        type="button"
        onclick={addCondition}
        disabled={query.filter.conditions.length >= 20}>Add condition</button
      >
    </fieldset>
    <fieldset disabled={busy || disconnected || !!pending}>
      <legend>Columns</legend>
      <div class="columns">
        {#each Object.entries(columnLabels) as [id, label] (id)}<label
            class="check"
            ><input
              type="checkbox"
              value={id}
              aria-label={`Show ${label} column`}
              bind:group={columns}
            />{label}</label
          >{/each}
      </div>
    </fieldset>
    <details>
      <summary>Column order</summary>
      <ol>
        {#each columns as column, index (column)}<li>
            {columnLabels[column]}
            <button
              disabled={busy || disconnected || !!pending || index === 0}
              onclick={() => {
                [columns[index - 1], columns[index]] = [
                  columns[index],
                  columns[index - 1],
                ];
              }}>Move {columnLabels[column]} earlier</button
            >
          </li>{/each}
      </ol>
    </details>
    <div class="actions">
      <button onclick={apply} disabled={busy || disconnected || !!pending}
        >Apply filters and columns</button
      ><button
        onclick={() => void save()}
        disabled={busy || disconnected || !!pending || !settings}
        >Save view</button
      ><button
        onclick={() => {
          query = defaultQuery();
          columns = [...defaultColumns];
          void apply();
        }}
        disabled={busy || disconnected || !!pending}>Reset view</button
      >
    </div>
    <fieldset disabled={busy || disconnected || !!pending || !settings}>
      <legend>Saved filters</legend>
      <div class="search-row">
        <label
          >Saved filter<select bind:value={selected} onchange={choose}
            ><option value="">Choose a filter</option
            >{#each settings?.settings.saved_filters ?? [] as filter (filter.id)}<option
                value={filter.id}>{filter.name}</option
              >{/each}</select
          ></label
        ><button onclick={() => void save("delete")} disabled={!selected}
          >Delete saved filter</button
        ><button onclick={() => void save("update")} disabled={!selected}
          >Update saved filter</button
        ><label
          >Filter name<input bind:value={filterName} maxlength="120" /></label
        ><button
          onclick={() => void save("filter")}
          disabled={!filterName.trim()}>Save filter</button
        >
      </div>
    </fieldset>
  </details>
  {#if busy}<p role="status">Loading register settings…</p>{/if}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if pending}<p role="status">
      Register settings request saved on this device. Retry before making
      another change.
    </p>
    <button disabled={busy || disconnected} onclick={() => void save()}
      >Retry register settings</button
    >{/if}
  {#if rejected || !settings}<button
      disabled={busy || disconnected}
      onclick={reloadRejected}>Reload register settings</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if disconnected}<p class="hint">
      Connect to search or change register settings.
    </p>{/if}
</section>

<style>
  .register-tools {
    margin-block: 1rem;
  }
  .search-row,
  .condition,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: end;
  }
  .search-row label {
    flex: 1 1 10rem;
  }
  label {
    display: grid;
    gap: 0.25rem;
    min-width: 0;
  }
  input,
  select {
    max-width: 100%;
    min-width: 0;
  }
  fieldset {
    margin-block: 1rem;
    border: 1px solid var(--line);
    padding: 0.75rem;
    min-width: 0;
  }
  .condition {
    margin-block: 0.75rem;
  }
  .condition label {
    flex: 1 1 8rem;
  }
  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }
  .check input {
    width: 1.125rem;
    height: 1.125rem;
    min-height: 0;
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  summary {
    cursor: pointer;
    padding-block: 0.75rem;
  }
  @media (max-width: 700px) {
    .search-row,
    .condition {
      align-items: stretch;
    }
    .condition {
      border-bottom: 1px solid var(--line);
      padding-bottom: 0.75rem;
    }
    .search-row label,
    .condition label {
      flex-basis: 100%;
    }
  }
</style>
