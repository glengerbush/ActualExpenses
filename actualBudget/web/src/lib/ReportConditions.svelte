<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { errorMessage, type Account } from "./api";
  import { references, type Reference } from "./register-api";
  import type { Condition } from "./register-query";
  import RegisterCondition from "./RegisterCondition.svelte";
  import { array } from "./reports";
  let {
    budgetId,
    accounts,
    conditions = [],
    conditionsOp = "and",
    namedConditionsIgnored = true,
    onchange,
  }: {
    budgetId: string;
    accounts: Account[];
    conditions?: unknown;
    conditionsOp?: unknown;
    namedConditionsIgnored?: boolean;
    onchange: (conditions: unknown[], op: string) => void;
  } = $props();
  let payees = $state.raw<Reference[]>([]),
    categories = $state.raw<Reference[]>([]),
    groups = $state.raw<Reference[]>([]),
    error = $state(""),
    busy = $state(true);
  let rows = $derived(array(conditions));
  let fields = $derived([
    "amount",
    "date",
    "cleared",
    "reconciled",
    "payee",
    "category",
    "category_group",
    "payee_name",
    "category_name",
    "category_group_name",
    "transfer",
    "parent",
  ]);
  const operators = [
    ["eq", "is"],
    ["ne", "is not"],
    ["gt", "greater than / after"],
    ["gte", "at least / from"],
    ["lt", "less than / before"],
    ["lte", "at most / through"],
    ["in", "is one of"],
    ["not_in", "is not one of"],
    ["contains", "contains"],
    ["not_contains", "does not contain"],
    ["matches", "matches expression"],
    ["isapprox", "approximately"],
    ["isbetween", "between"],
    ["onBudget", "on budget"],
    ["offBudget", "off budget"],
  ];
  async function load() {
    busy = true;
    error = "";
    try {
      [payees, categories, groups] = await Promise.all([
        references(budgetId, "payees", { includeTransfers: true }),
        references(budgetId, "categories"),
        references(budgetId, "category-groups"),
      ]);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void load();
  });
  function allowed(field: string) {
    const names =
      field === "amount"
        ? ["eq", "ne", "gt", "gte", "lt", "lte", "isapprox", "isbetween"]
        : field === "date"
          ? ["eq", "ne", "gt", "gte", "lt", "lte", "isapprox"]
          : ["cleared", "reconciled", "transfer", "parent"].includes(field)
            ? ["eq", "ne"]
            : ["account", "payee", "category", "category_group"].includes(field)
              ? [
                  "eq",
                  "ne",
                  "in",
                  "not_in",
                  ...(field === "account" ? ["onBudget", "offBudget"] : []),
                ]
              : [
                  "eq",
                  "ne",
                  "contains",
                  "not_contains",
                  "matches",
                  "in",
                  "not_in",
                ];
    return operators.filter((op) => names.includes(op[0]));
  }
  function canonical(row: Record<string, unknown>): Condition {
    const map: Record<string, string> = {
      is: "eq",
      isNot: "ne",
      oneOf: "in",
      notOneOf: "not_in",
      doesNotContain: "not_contains",
    };
    return {
      ...row,
      field:
        ["account", "payee", "category", "category_group"].includes(
          String(row.field),
        ) &&
        ["contains", "doesNotContain", "not_contains", "matches"].includes(
          String(row.op),
        )
          ? String(row.field) + "_name"
          : String(row.field),
      op: map[String(row.op)] || String(row.op),
      value: row.value as Condition["value"],
    };
  }
  function update(index: number, value: Condition) {
    onchange(
      rows.map((row, i) => (i === index ? value : row)),
      String(conditionsOp || "and"),
    );
  }
  function field(index: number, name: string) {
    const numeric = name === "amount",
      boolean = ["cleared", "reconciled", "transfer", "parent"].includes(name),
      reference = ["account", "payee", "category", "category_group"].includes(
        name,
      );
    update(index, {
      field: name,
      op:
        numeric || name === "date"
          ? "gte"
          : boolean || reference
            ? "eq"
            : "contains",
      value: numeric ? 0 : boolean ? true : reference ? null : "",
    });
  }
  function operator(index: number, op: string) {
    const c = canonical(rows[index]);
    update(index, {
      ...c,
      op,
      value:
        op === "isbetween"
          ? { num1: 0, num2: 0 }
          : c.field === "amount"
            ? 0
            : ["in", "not_in"].includes(op)
              ? Array.isArray(c.value)
                ? c.value
                : [c.value as string]
              : Array.isArray(c.value)
                ? (c.value[0] ?? null)
                : c.value,
    });
  }
</script>

<fieldset>
  <legend>{t("Report conditions")}</legend>
  {#if error}<p role="alert">{t(error)}</p>
    <button type="button" onclick={load} disabled={busy}
      >{t("Reload filter choices")}</button
    >{/if}
  <label
    >{t("Match report conditions")}<select
      value={String(conditionsOp || "and")}
      onchange={(e) => onchange(rows, e.currentTarget.value)}
      ><option value="and">{t("All conditions")}</option><option value="or"
        >{t("Any condition")}</option
      ></select
    ></label
  >
  {#each rows as raw, index (raw)}{@const condition = canonical(raw)}
    <div class="condition">
      <label
        >{t("Report field")}
        {index + 1}<select
          value={condition.field}
          onchange={(e) => field(index, e.currentTarget.value)}
          >{#if !fields.includes(condition.field)}<option
              value={condition.field}
              >{t("Preserved:")} {condition.field}</option
            >{/if}{#each fields as name (name)}<option value={name}
              >{t(
                name.replaceAll("_", " ").replace(/^./, (s) => s.toUpperCase()),
              )}</option
            >{/each}</select
        ></label
      >
      <label
        >{t("Report operator")}
        {index + 1}<select
          value={condition.op}
          onchange={(e) => operator(index, e.currentTarget.value)}
          >{#if !allowed(condition.field).some((o) => o[0] === condition.op)}<option
              value={condition.op}>{t("Preserved:")} {condition.op}</option
            >{/if}{#each allowed(condition.field) as op (op[0])}<option
              value={op[0]}>{t(op[1])}</option
            >{/each}</select
        ></label
      >
      <RegisterCondition
        {condition}
        {index}
        {accounts}
        {payees}
        {categories}
        {groups}
        onchange={(value) => update(index, value)}
      /><button
        type="button"
        onclick={() =>
          onchange(
            rows.filter((_, i) => i !== index),
            String(conditionsOp || "and"),
          )}>{t("Remove report condition")} {index + 1}</button
      >
    </div>{/each}
  <button
    type="button"
    disabled={busy || rows.length >= 20}
    onclick={() =>
      onchange(
        [...rows, { field: "payee", op: "is", value: null }],
        String(conditionsOp || "and"),
      )}>{t("Add report condition")}</button
  >
</fieldset>

<style>
  fieldset {
    min-width: 0;
    border: 1px solid var(--line);
    padding: 1rem;
    display: grid;
    gap: 1rem;
  }
  .condition {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    align-items: end;
  }
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
    max-width: 100%;
  }
  select {
    max-width: 100%;
  }
</style>
