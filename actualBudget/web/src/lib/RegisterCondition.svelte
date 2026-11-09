<script lang="ts">
  import { captureAmountFormat, parseAmount, formatAmountInput } from "./money";
  import type { Reference } from "./register-api";
  import type { Condition } from "./register-query";
  let {
    condition,
    index,
    accounts,
    payees,
    categories,
    groups,
    onchange,
  }: {
    condition: Condition;
    index: number;
    accounts: Reference[];
    payees: Reference[];
    categories: Reference[];
    groups: Reference[];
    onchange: (value: Condition) => void;
  } = $props();
  let choices = $derived(
    condition.field === "account"
      ? accounts
      : condition.field === "payee"
        ? payees
        : condition.field === "category"
          ? categories
          : groups,
  );
  let selectedIds = $derived(
    (Array.isArray(condition.value) ? condition.value : [condition.value]).map(
      (value) => String(value ?? ""),
    ),
  );
  function referenceLabel(row: Reference) {
    const group =
      condition.field === "category"
        ? groups.find((group) => group.id === row.group_id)?.name
        : null;
    let label = group ? `${group} / ${row.name}` : row.name;
    if (
      choices.some(
        (other) =>
          other.id !== row.id &&
          other.name === row.name &&
          (condition.field !== "category" || other.group_id === row.group_id),
      )
    )
      label += ` (${row.id})`;
    if (row.deleted) label += " · deleted";
    else if (row.hidden || row.group_hidden) label += " · hidden";
    return label;
  }
  let object = $derived(
    typeof condition.value === "object" &&
      !Array.isArray(condition.value) &&
      condition.value !== null
      ? condition.value
      : {},
  );
  function displayAmount(value: unknown) {
    return typeof value === "number"
      ? formatAmountInput(value)
      : String(value ?? "");
  }
  function readAmount(value: string) {
    try {
      return parseAmount(value, captureAmountFormat());
    } catch {
      return value;
    }
  }
  let amountError = $derived.by(() => {
    if (condition.field !== "amount") return "";
    for (const value of condition.op === "isbetween"
      ? [object.num1, object.num2]
      : [condition.value]) {
      if (typeof value !== "number") {
        try {
          parseAmount(String(value ?? ""), captureAmountFormat());
        } catch (e) {
          return (e as Error).message;
        }
      }
    }
    return "";
  });
  function set(value: Condition["value"]) {
    onchange({ ...condition, value });
  }
  function setObject(key: string, value: unknown) {
    set({ ...object, [key]: value });
  }
  function mode() {
    if (condition.field !== "date") return "";
    if (typeof condition.value === "object") return "recurring";
    return typeof condition.value === "string" && condition.value.length === 7
      ? "month"
      : typeof condition.value === "string" && condition.value.length === 4
        ? "year"
        : "day";
  }
  function dateMode(value: string) {
    set(
      value === "recurring"
        ? { frequency: "monthly", start: "2026-01-01", interval: 1 }
        : value === "month"
          ? "2026-01"
          : value === "year"
            ? "2026"
            : "2026-01-01",
    );
  }
</script>

{#if ["cleared", "reconciled", "transfer", "parent"].includes(condition.field)}
  <label
    >Value {index + 1}<select
      value={String(condition.value)}
      onchange={(e) => set(e.currentTarget.value === "true")}
      ><option value="true">Yes</option><option value="false">No</option
      ></select
    ></label
  >
{:else if condition.field === "amount"}
  {#if condition.op === "isbetween"}
    <label
      >From amount {index + 1}<input
        inputmode="decimal"
        value={displayAmount(object.num1 ?? 0)}
        onchange={(e) => setObject("num1", readAmount(e.currentTarget.value))}
      /></label
    >
    <label
      >To amount {index + 1}<input
        inputmode="decimal"
        value={displayAmount(object.num2 ?? 0)}
        onchange={(e) => setObject("num2", readAmount(e.currentTarget.value))}
      /></label
    >
  {:else}
    <label
      >Amount value {index + 1}<input
        inputmode="decimal"
        value={displayAmount(condition.value)}
        onchange={(e) => set(readAmount(e.currentTarget.value))}
      /></label
    >
    <label
      >Payment or deposit {index + 1}<select
        value={condition.options?.outflow
          ? "outflow"
          : condition.options?.inflow
            ? "inflow"
            : "either"}
        onchange={(e) =>
          onchange({
            ...condition,
            options: {
              outflow: e.currentTarget.value === "outflow",
              inflow: e.currentTarget.value === "inflow",
            },
          })}
        ><option value="either">Signed amount</option><option value="outflow"
          >Payment</option
        ><option value="inflow">Deposit</option></select
      ></label
    >
  {/if}
  {#if amountError}<p class="hint" role="alert">{amountError}</p>{/if}
{:else if condition.field === "date"}
  {#if ["eq", "isapprox"].includes(condition.op)}<label
      >Date matching {index + 1}<select
        value={mode()}
        onchange={(e) => dateMode(e.currentTarget.value)}
        ><option value="day">Calendar day</option
        >{#if condition.op === "eq"}<option value="month">Month</option><option
            value="year">Year</option
          >{/if}<option value="recurring">Recurring dates</option></select
      ></label
    >{/if}
  {#if mode() === "recurring"}
    <label
      >Frequency {index + 1}<select
        value={String(object.frequency)}
        onchange={(e) => setObject("frequency", e.currentTarget.value)}
        >{#each ["daily", "weekly", "monthly", "yearly"] as frequency (frequency)}<option
            value={frequency}>{frequency}</option
          >{/each}</select
      ></label
    >
    <label
      >Starts {index + 1}<input
        type="date"
        value={String(object.start ?? "")}
        oninput={(e) => setObject("start", e.currentTarget.value)}
      /></label
    >
    <label
      >Every {index + 1}<input
        type="number"
        min="1"
        max="10000"
        value={Number(object.interval ?? 1)}
        oninput={(e) => setObject("interval", Number(e.currentTarget.value))}
      /></label
    >
    <label
      >Ends {index + 1}<select
        value={String(object.endMode ?? "never")}
        onchange={(e) =>
          set({
            ...object,
            endMode: e.currentTarget.value,
            endOccurrences: 10,
            endDate: String(object.start),
          })}
        ><option value="never">No end</option><option
          value="after_n_occurrences">After occurrences</option
        ><option value="on_date">On date</option></select
      ></label
    >
    {#if object.endMode === "after_n_occurrences"}<label
        >Occurrences {index + 1}<input
          type="number"
          min="1"
          value={Number(object.endOccurrences ?? 10)}
          oninput={(e) =>
            setObject("endOccurrences", Number(e.currentTarget.value))}
        /></label
      >{:else if object.endMode === "on_date"}<label
        >End date {index + 1}<input
          type="date"
          value={String(object.endDate ?? "")}
          oninput={(e) => setObject("endDate", e.currentTarget.value)}
        /></label
      >{/if}
    <p class="hint">
      Matches the first 100 dates. Monthly dates follow the start day; imported
      weekday patterns are preserved.
    </p>
  {:else}<label
      >Date value {index + 1}<input
        type={mode() === "month"
          ? "month"
          : mode() === "year"
            ? "text"
            : "date"}
        value={String(condition.value)}
        oninput={(e) => set(e.currentTarget.value)}
      /></label
    >{/if}
{:else if ["account", "payee", "category", "category_group"].includes(condition.field)}
  {#if !["onBudget", "offBudget"].includes(condition.op)}
    <label
      >Value {index + 1}<select
        multiple={["in", "not_in"].includes(condition.op)}
        onchange={(e) => {
          const values = Array.from(
            e.currentTarget.selectedOptions,
            (option) => option.value || null,
          );
          set(
            ["in", "not_in"].includes(condition.op)
              ? values
              : (values[0] ?? null),
          );
        }}
      >
        <option value="" selected={selectedIds.includes("")}
          >{condition.field === "category"
            ? "Uncategorized"
            : condition.field === "payee"
              ? "No payee"
              : condition.field === "category_group"
                ? "No category group"
                : "No account"}</option
        >
        {#each choices as row (row.id)}<option
            value={row.id}
            selected={selectedIds.includes(row.id)}
            >{referenceLabel(row)}</option
          >{/each}
        {#each selectedIds.filter((id) => id && !choices.some((row) => row.id === id)) as id (id)}<option
            value={id}
            selected>Unavailable reference ({id})</option
          >{/each}
      </select>{#if ["in", "not_in"].includes(condition.op)}<span class="hint"
          >Select one or more entries.</span
        >{/if}</label
    >
  {/if}
{:else if ["in", "not_in"].includes(condition.op)}
  <label
    >Values {index + 1}<textarea
      value={Array.isArray(condition.value)
        ? condition.value.join("\n")
        : String(condition.value)}
      oninput={(e) => set(e.currentTarget.value.split("\n"))}></textarea><span
      class="hint">One value per line.</span
    ></label
  >
{:else}<label
    >Value {index + 1}<input
      value={typeof condition.value === "string" ? condition.value : ""}
      oninput={(e) => set(e.currentTarget.value)}
    /></label
  >{/if}

<style>
  label {
    display: grid;
    gap: 0.25rem;
    flex: 1 1 8rem;
    min-width: 0;
  }
  input,
  select,
  textarea {
    max-width: 100%;
    min-width: 0;
  }
  .hint {
    flex-basis: 100%;
  }
</style>
