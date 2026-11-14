<script lang="ts">
  import { t } from "./i18n.svelte";
  import { references, type Reference } from "./register-api";
  import RegisterCondition from "./RegisterCondition.svelte";
  import { onMount, untrack, type Snippet } from "svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import {
    defaultQuery,
    manualQuery,
    defaultColumns,
    transactionColumns,
    columnLabels,
    type RegisterQuery,
    type RegisterSettings,
    type SettingsResponse,
    type Condition,
  } from "./register-query";
  let {
    budgetId,
    initialQuery,
    accountId,
    disconnected,
    locked = false,
    accounts,
    payees,
    categories,
    cutoff,
    onapply,
    dateFilter,
  }: {
    dateFilter?: Snippet;
    initialQuery?: RegisterQuery;
    budgetId: string;
    accountId: string | null;
    disconnected: boolean;
    locked?: boolean;
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
  let panel = $state<"filters" | "columns" | "saved">("filters");
  let groups = $state.raw<Reference[]>([]);
  let transferPayees = $state.raw<Reference[]>([]);
  let filterPayees = $derived([
    ...payees,
    ...transferPayees.map((payee) => ({
      ...payee,
      name:
        accounts.find((account) => account.id === payee.transfer_account_id)
          ?.name ?? payee.name,
    })),
  ]);
  async function loadGroups() {
    if (disconnected) return;
    try {
      const [loadedGroups, loadedPayees] = await Promise.all([
        references(budgetId, "category-groups"),
        references(budgetId, "payees", { includeTransfers: true }),
      ]);
      if (!alive) return;
      groups = loadedGroups;
      transferPayees = loadedPayees.filter(
        (payee) => payee.transfer_account_id,
      );
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let query = $state<RegisterQuery>(
    untrack(() =>
      initialQuery ? structuredClone(initialQuery) : defaultQuery(),
    ),
  );
  let applyInitialQuery = untrack(() => !!initialQuery);
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
        if (!applyInitialQuery)
          query = manualQuery(structuredClone(view.query));
        columns = transactionColumns(view.columns);
        extraBalances = view.show_extra_balances ?? true;
      }
      if (apply && (view || applyInitialQuery) && !preserveDraft)
        await onapply(
          $state.snapshot(query),
          [...columns],
          view?.as_of ?? cutoff,
          extraBalances,
        );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      applyInitialQuery = false;
      if (alive) busy = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      alive = false;
    };
  });
  export async function sortBy(field: string) {
    if (busy || locked || disconnected || pending) return;
    query.direction =
      query.sort === field
        ? query.direction === "asc"
          ? "desc"
          : "asc"
        : field === "date"
          ? "desc"
          : "asc";
    query.sort = field;
    await apply();
  }
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
    query = view ? manualQuery(structuredClone(view.query)) : defaultQuery();
    query.account_scope = selectedScope;
    columns = transactionColumns(view?.columns ?? defaultColumns);
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
        query = manualQuery(structuredClone(view.query));
        columns = transactionColumns(view.columns);
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
      query = manualQuery(structuredClone(filter.query));
      filterName = filter.name;
      void apply();
    }
  }
  function addCondition() {
    query.filter.conditions.push({
      field: "payee_name",
      op: "contains",
      value: "",
    });
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
  ononline={() => {
    if (error) void load(false, true);
  }}
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
    class="search-row quick-search"
    onsubmit={(event) => {
      event.preventDefault();
      void apply();
    }}
  >
    <input
      type="search"
      aria-label="Search transactions"
      placeholder="Search transactions"
      bind:value={query.search}
      maxlength="500"
      disabled={busy || locked || disconnected || !!pending}
    />
    <button
      class="search-submit"
      aria-label="Find transactions"
      title="Search"
      disabled={busy || locked || disconnected || !!pending}
      ><svg
        aria-hidden="true"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        ><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4 4" /></svg
      ></button
    >
  </form>
  <details
    ontoggle={(event) => {
      if (event.currentTarget.open) void loadGroups();
    }}
  >
    <summary aria-label="Filters and columns">Filters</summary>
    <div class="filter-panel">
      <div class="panel-switcher" role="group" aria-label="Filter settings">
        {#each [["filters", "Conditions"], ["columns", "Columns"], ["saved", "Saved filters"]] as [id, label] (id)}
          <button
            type="button"
            aria-pressed={panel === id}
            onclick={() => (panel = id as typeof panel)}
            >{label}{#if id === "filters" && query.filter.conditions.length}<span
                class="condition-count">{query.filter.conditions.length}</span
              >{/if}</button
          >
        {/each}
      </div>
      {#if panel === "filters"}
        <div class="filter-date">{@render dateFilter?.()}</div>

        <fieldset disabled={busy || locked || disconnected || !!pending}>
          <legend class="visually-hidden">Filter conditions</legend>
          <div class="condition-toolbar">
            <label
              >Match<select bind:value={query.filter.op}
                ><option value="and">All conditions</option><option value="or"
                  >Any condition</option
                ></select
              ></label
            >
            <button
              type="button"
              class="add-condition"
              onclick={addCondition}
              disabled={query.filter.conditions.length >= 20}
            >
              <svg
                aria-hidden="true"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.75"><path d="M12 5v14M5 12h14" /></svg
              >{t("Add condition")}
            </button>
          </div>
          {#each query.filter.conditions as condition, index (condition)}
            <div class="condition">
              <label
                >Field {index + 1}<select
                  aria-label={`Field ${index + 1}`}
                  bind:value={condition.field}
                  onchange={() => changeField(condition)}
                  >{#if condition.field === "account"}<option value="account"
                      >Historical account</option
                    >{/if}{#each ["amount", "date", "payee", "category", "category_group", "payee_name", "category_name", "category_group_name", "transfer", "parent"] as field (field)}<option
                      value={field}>{field.replaceAll("_", " ")}</option
                    >{/each}</select
                ></label
              >
              <label
                >Operator {index + 1}<select
                  aria-label={`Operator ${index + 1}`}
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
                  ><option value="eq">{t("is")}</option><option value="ne"
                    >{t("is not")}</option
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
                        >{t("is on budget")}</option
                      ><option value="offBudget">{t("is off budget")}</option
                      >{/if}
                  {:else if !["cleared", "reconciled", "transfer", "parent"].includes(condition.field)}<option
                      value="contains">{t("contains")}</option
                    ><option value="not_contains"
                      >{t("does not contain")}</option
                    ><option value="matches">matches regular expression</option
                    ><option value="in">is one of</option><option value="not_in"
                      >is not one of</option
                    >{/if}</select
                ></label
              >
              <div class="condition-values">
                <RegisterCondition
                  {condition}
                  {index}
                  {accounts}
                  payees={filterPayees}
                  {categories}
                  {groups}
                  onchange={(value) => Object.assign(condition, value)}
                />
              </div>
              <button
                type="button"
                onclick={() => query.filter.conditions.splice(index, 1)}
                class="remove-condition"
                aria-label={`Remove condition ${index + 1}`}
                title="Remove condition"
                ><svg
                  aria-hidden="true"
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.75"><path d="m6 6 12 12M6 18 18 6" /></svg
                ></button
              >
            </div>
          {/each}
        </fieldset>
      {:else if panel === "columns"}
        <fieldset disabled={busy || locked || disconnected || !!pending}>
          <legend class="visually-hidden">Visible columns</legend>
          <div class="columns">
            {#each Object.entries(columnLabels).filter(([key]) => !["account", "payment", "deposit"].includes(key)) as [id, label] (id)}<label
                class="check"
                ><input
                  type="checkbox"
                  value={id}
                  aria-label={`Show ${label} column`}
                  disabled={id === "amount"}
                  bind:group={columns}
                />{label}</label
              >{/each}
          </div>
        </fieldset>
        <details class="column-order">
          <summary>Column order</summary>
          <ol>
            {#each columns as column, index (column)}<li>
                {columnLabels[column]}
                <button
                  disabled={busy ||
                    locked ||
                    disconnected ||
                    !!pending ||
                    index === 0}
                  onclick={() => {
                    [columns[index - 1], columns[index]] = [
                      columns[index],
                      columns[index - 1],
                    ];
                  }}
                  aria-label={`Move ${columnLabels[column]} earlier`}
                  title={`Move ${columnLabels[column]} earlier`}
                  class="remove-condition"
                  ><svg
                    aria-hidden="true"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.75"><path d="m6 14 6-6 6 6" /></svg
                  ></button
                >
              </li>{/each}
          </ol>
        </details>
      {:else}
        <fieldset
          disabled={busy || locked || disconnected || !!pending || !settings}
        >
          <legend class="visually-hidden">Saved filters</legend>
          <div class="saved-filter-grid">
            <label
              >Saved filter<select
                aria-label="Saved filter"
                bind:value={selected}
                onchange={choose}
                ><option value="">Choose a filter</option
                >{#each settings?.settings.saved_filters ?? [] as filter (filter.id)}<option
                    value={filter.id}>{filter.name}</option
                  >{/each}</select
              ></label
            ><button
              onclick={() => void save("delete")}
              disabled={!selected}
              aria-label="Delete saved filter"
              title="Delete saved filter"
              class="remove-condition"
              ><svg
                aria-hidden="true"
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.75"
                ><path
                  d="M3 6h18M9 6V4h6v2M5 6l1 14h12l1-14M10 10v6M14 10v6"
                /></svg
              ></button
            ><button
              onclick={() => void save("update")}
              disabled={!selected}
              aria-label="Update saved filter">Update</button
            ><label
              >{t("Filter name")}<input
                bind:value={filterName}
                maxlength="120"
              /></label
            ><button
              onclick={() => void save("filter")}
              disabled={!filterName.trim()}>Save filter</button
            >
          </div>
        </fieldset>
      {/if}
      <div class="actions">
        <button
          onclick={apply}
          disabled={busy || locked || disconnected || !!pending}
          class="primary"
          aria-label="Apply filters and columns"
          >{panel === "columns" ? "Apply columns" : "Apply filters"}</button
        ><button
          onclick={() => void save()}
          disabled={busy || locked || disconnected || !!pending || !settings}
          >Save view</button
        ><button
          onclick={() => {
            query = defaultQuery();
            columns = [...defaultColumns];
            void apply();
          }}
          disabled={busy || locked || disconnected || !!pending}
          >Reset view</button
        >
      </div>
    </div>
  </details>
  {#if busy}<p role="status">Loading register settings…</p>{/if}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if pending}<p role="status">
      Register settings request saved on this device. Retry before making
      another change.
    </p>
    <button
      disabled={busy || locked || disconnected}
      onclick={() => void save()}>Retry register settings</button
    >{/if}
  {#if rejected || !settings}<button
      disabled={busy || locked || disconnected}
      onclick={reloadRejected}>Reload register settings</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if disconnected}<p class="hint">
      Connect to search or change register settings.
    </p>{/if}
</section>

<style>
  .register-tools {
    display: contents;
  }
  .register-tools > details {
    grid-column: 2;
    grid-row: 1;
  }
  .register-tools > details[open] {
    grid-column: 1 / -1;
    grid-row: 2;
  }
  .register-tools > :is(p, button) {
    grid-column: 1 / -1;
  }
  .quick-search {
    grid-column: 1;
    grid-row: 1;
    position: relative;
  }
  .quick-search input {
    width: 100%;
    padding-right: 44px;
  }
  .search-submit {
    position: absolute;
    right: 0;
    top: 0;
    width: 44px;
    height: 44px;
    padding: 0;
    display: grid;
    place-items: center;
    background: transparent;
    border-color: transparent;
  }

  .filter-panel {
    border-block: 1px solid var(--line);
    padding: 0.5rem 0 0.75rem;
  }
  .panel-switcher {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
  }
  .panel-switcher button {
    border: 0;
    background: transparent;
    padding: 0.45rem 0.75rem;
    font-size: 0.875rem;
  }
  .panel-switcher button[aria-pressed="true"] {
    background: var(--hover);
    color: var(--accent);
  }
  .condition-count {
    margin-left: 0.4rem;
    font-variant-numeric: tabular-nums;
  }
  label {
    display: grid;
    gap: 0.25rem;
    min-width: 0;
    margin: 0;
    font-size: 0.875rem;
  }
  input,
  select {
    max-width: 100%;
    min-width: 0;
  }
  fieldset {
    margin: 0;
    border: 0;
    padding: 0;
    min-width: 0;
  }
  .filter-date {
    margin-bottom: 0.75rem;
  }
  .filter-date :global(.date-filter) {
    margin: 0;
    align-items: end;
    gap: 0.5rem;
  }
  .filter-date :global(.date-field) {
    min-width: 0;
    max-width: 100%;
  }
  .filter-date :global(label) {
    margin: 0 0 0.25rem;
    font-size: 0.875rem;
  }
  .filter-date :global(#register-as-of-format) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .filter-date :global(#register-as-of) {
    width: 10rem;
  }
  .condition-toolbar {
    display: flex;
    align-items: end;
    gap: 0.75rem;
  }
  .condition-toolbar label {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .condition-toolbar select {
    width: 11rem;
  }
  .add-condition {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border-color: transparent;
    background: transparent;
  }
  .condition {
    display: grid;
    grid-template-columns:
      minmax(8rem, 1fr) minmax(9rem, 1fr) minmax(12rem, 2fr)
      44px;
    gap: 0.5rem;
    align-items: end;
    margin-top: 0.65rem;
  }
  .condition-values {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    min-width: 0;
    align-items: end;
  }
  .condition-values :global(label) {
    margin: 0;
    font-size: 0.875rem;
  }
  .condition-values :global(.hint) {
    margin: 0;
  }
  .condition-values :global(input),
  .condition-values :global(select) {
    width: 100%;
  }
  .remove-condition {
    display: grid;
    place-items: center;
    width: 44px;
    min-height: 44px;
    padding: 0;
    background: transparent;
    border-color: transparent;
  }
  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 1.5rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .check input {
    width: 1.125rem;
    height: 1.125rem;
    min-height: 0;
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .column-order {
    margin-top: 0.25rem;
  }
  .column-order ol {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.25rem;
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .column-order li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .saved-filter-grid {
    display: grid;
    grid-template-columns: minmax(10rem, 1fr) 44px auto minmax(10rem, 1fr) auto;
    gap: 0.5rem;
    align-items: end;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.85rem;
  }
  summary {
    cursor: pointer;
    padding: 0.65rem 0.5rem;
    white-space: nowrap;
  }
  @media (max-width: 900px) {
    .condition {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 44px;
    }
    .condition-values {
      grid-column: 1 / 3;
      grid-row: 2;
    }
    .condition > .remove-condition {
      grid-column: 3;
      grid-row: 2;
    }
    .saved-filter-grid {
      grid-template-columns: minmax(0, 1fr) 44px auto;
    }
    .saved-filter-grid > label:nth-of-type(2) {
      grid-column: 1 / 3;
    }
  }
  @media (max-width: 500px) {
    .panel-switcher {
      gap: 0;
    }
    .panel-switcher button {
      padding-inline: 0.5rem;
    }
    .condition-toolbar {
      flex-wrap: wrap;
      gap: 0.25rem;
    }
    .condition {
      padding-bottom: 0.65rem;
      border-bottom: 1px solid var(--line);
    }
  }
</style>
