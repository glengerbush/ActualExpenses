<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { RuleAction } from "./rules";
  import type { Reference } from "./register-api";
  import { formatAmountInput, parseAmount, type AmountFormat } from "./money";
  import DateInput from "./DateInput.svelte";
  let {
    action,
    index,
    accounts,
    payees,
    categories,
    format,
    schedules = [],
    onmoreschedules,
    onchange,
  }: {
    action: RuleAction;
    index: number;
    accounts: Reference[];
    payees: Reference[];
    categories: Reference[];
    format: AmountFormat;
    schedules?: Reference[];
    onmoreschedules?: () => Promise<void>;
    onchange: (a: RuleAction) => void;
  } = $props();
  const fields: Record<string, string> = {
    category: "Category",
    payee: "Payee",
    payee_name: "Payee name",
    account: "Account",
    date: "Date",
    amount: "Amount",
  };
  const operations: Record<string, string> = {
    set: "Set field",
    "delete-transaction": "Delete transaction",
    "link-schedule": "Link schedule",
    "set-split-amount": "Allocate split",
  };
  let choices = $derived(
    action.field === "account"
      ? accounts
      : action.field === "payee"
        ? payees
        : categories,
  );
  function field(name: string) {
    onchange({
      ...action,
      field: name,
      value: ["payee", "category", "account"].includes(name)
        ? null
        : name === "amount"
          ? 0
          : name === "cleared"
            ? false
            : name === "date"
              ? "2026-01-01"
              : "",
    });
  }
  function op(value: string) {
    onchange({
      op: value,
      ...(value === "set" ? { field: "category" } : {}),
      value: value === "set-split-amount" ? 0 : null,
      ...(value === "set-split-amount"
        ? { options: { splitIndex: 1, method: "fixed-amount" } }
        : {}),
    });
  }
  function amount(value: string) {
    try {
      return parseAmount(value, format);
    } catch {
      return value;
    }
  }
  function option(key: string, value: string | number) {
    const options = { ...action.options, [key]: value };
    onchange({ ...action, options });
  }
</script>

<label
  >Action {index + 1}<select
    value={action.op}
    onchange={(e) => op(e.currentTarget.value)}
    >{#each Object.entries(operations) as [value, label] (value)}<option {value}
        >{label}</option
      >{/each}</select
  ></label
>
{#if action.op === "set"}
  <label
    >Set field {index + 1}<select
      value={action.field}
      onchange={(e) => field(e.currentTarget.value)}
      >{#each Object.entries(fields).filter(([name]) => name !== "account" || action.field === "account") as [value, label] (value)}<option
          {value}>{label}</option
        >{/each}</select
    ></label
  >
  {#if ["category", "payee", "account"].includes(action.field ?? "")}
    <label
      >Set value {index + 1}<select
        value={String(action.value ?? "")}
        onchange={(e) =>
          onchange({ ...action, value: e.currentTarget.value || null })}
        ><option value=""
          >{t(action.field === "account" ? "Choose account" : "None")}</option
        >{#each choices as row (row.id)}<option value={row.id}
            >{row.name}</option
          >{/each}{#if action.value && !choices.some((r) => r.id === action.value)}<option
            value={String(action.value)}
            >Unavailable reference ({String(action.value)})</option
          >{/if}</select
      ></label
    >
  {:else if action.field === "cleared"}<label
      >Set value {index + 1}<select
        value={String(action.value)}
        onchange={(e) =>
          onchange({ ...action, value: e.currentTarget.value === "true" })}
        ><option value="true">{t("Yes")}</option><option value="false"
          >{t("No")}</option
        ></select
      ></label
    >
  {:else if action.field === "amount"}<label
      >Set amount {index + 1}<input
        inputmode="decimal"
        value={typeof action.value === "number"
          ? formatAmountInput(action.value, format)
          : String(action.value ?? "")}
        onchange={(e) =>
          onchange({ ...action, value: amount(e.currentTarget.value) })}
      /></label
    >
  {:else if action.field === "date"}<DateInput
      id={`rule-action-date-${index}`}
      label={`Set date ${index + 1}`}
      format="yyyy-MM-dd"
      bind:value={
        () => String(action.value), (value) => onchange({ ...action, value })
      }
      required
    />
  {:else}<label
      >Set value {index + 1}<input
        value={String(action.value ?? "")}
        oninput={(e) => onchange({ ...action, value: e.currentTarget.value })}
      /></label
    >{/if}
{:else if action.op === "link-schedule"}<label
    >Linked schedule {index + 1}<select
      value={String(action.value ?? "")}
      onchange={(e) => onchange({ ...action, value: e.currentTarget.value })}
      ><option value="">Choose schedule</option
      >{#each schedules as schedule (schedule.id)}<option value={schedule.id}
          >{schedule.name}</option
        >{/each}{#if action.value && !schedules.some((s) => s.id === action.value)}<option
          value={String(action.value)}
          >Saved schedule ({String(action.value)})</option
        >{/if}</select
    ></label
  >{#if onmoreschedules}<button onclick={onmoreschedules}
      >Load more schedules</button
    >{/if}
{:else if action.op === "set-split-amount"}
  <label
    >Allocation method {index + 1}<select
      value={action.options?.method}
      onchange={(e) => option("method", e.currentTarget.value)}
      ><option value="fixed-amount">{t("Fixed amount")}</option><option
        value="fixed-percent">{t("Percentage")}</option
      ><option value="remainder">Remaining amount</option></select
    ></label
  >
  {#if action.options?.method !== "remainder"}<label
      >{action.options?.method === "fixed-percent"
        ? "Percent"
        : "Allocation amount"}
      {index + 1}<input
        inputmode="decimal"
        value={action.options?.method === "fixed-percent"
          ? String(action.value)
          : typeof action.value === "number"
            ? formatAmountInput(action.value, format)
            : String(action.value ?? "")}
        onchange={(e) =>
          onchange({
            ...action,
            value:
              action.options?.method === "fixed-percent"
                ? Number(e.currentTarget.value)
                : amount(e.currentTarget.value),
          })}
      /></label
    >{/if}
{:else if action.op === "delete-transaction"}<p class="hint">
    Matching transactions will be deleted only after you select and apply them.
  </p>{/if}
{#if action.op !== "delete-transaction"}<label
    >Split target {index + 1}<input
      type="number"
      min={action.op === "set-split-amount" ? 1 : 0}
      max="100"
      value={action.options?.splitIndex ?? 0}
      onchange={(e) => option("splitIndex", Number(e.currentTarget.value))}
    /><span class="hint"
      >0: parent and applicable children. 1–100: allocation number. Amount,
      account and date belong to the parent.</span
    ></label
  >{/if}
