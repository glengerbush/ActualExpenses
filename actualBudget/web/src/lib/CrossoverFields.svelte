<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import { list, errorMessage, type Account } from "./api";
  import { references, type Reference } from "./register-api";
  import { captureAmountFormat, formatAmountInput, parseAmount } from "./money";
  let {
    budgetId,
    meta,
    onchange,
  }: {
    budgetId: string;
    meta: Record<string, unknown>;
    onchange: (patch: Record<string, unknown>) => void;
  } = $props();
  let accounts = $state.raw<Account[]>([]),
    categories = $state.raw<Reference[]>([]),
    busy = $state(true),
    error = $state("");
  let contributionDraft = $state.raw<{
    value: unknown;
    text: string;
    format: string;
  } | null>(null);
  let contributionText = $derived.by(() => {
    const value = meta.expectedContribution,
      format = captureAmountFormat(),
      signature = JSON.stringify(format);
    if (
      contributionDraft &&
      Object.is(contributionDraft.value, value) &&
      contributionDraft.format === signature
    )
      return contributionDraft.text;
    return typeof value === "number" && Number.isSafeInteger(value)
      ? formatAmountInput(value, format)
      : String(value ?? "0");
  });
  function contributionValidity(input: HTMLInputElement) {
    const value = meta.expectedContribution;
    input.setCustomValidity(
      value != null && !Number.isSafeInteger(value)
        ? t("Enter a valid contribution before saving.")
        : "",
    );
  }
  async function load() {
    busy = true;
    error = "";
    try {
      [accounts, categories] = await Promise.all([
        list<Account>(`/budgets/${budgetId}/accounts`),
        references(budgetId, "categories"),
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
  let custom = $derived(
    meta.estimatedReturn != null || meta.expectedContribution != null,
  );
  let selectedAccounts = $derived(
    Array.isArray(meta.incomeAccountIds)
      ? (meta.incomeAccountIds as string[])
      : accounts.map((a) => a.id),
  );
  let selectedCategories = $derived(
    Array.isArray(meta.expenseCategoryIds)
      ? (meta.expenseCategoryIds as string[])
      : categories.filter((c) => !c.is_income).map((c) => c.id),
  );
  function contribution(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    let value: unknown;
    try {
      value = parseAmount(input.value, captureAmountFormat());
      input.setCustomValidity("");
    } catch (e) {
      value = input.value;
      input.setCustomValidity(errorMessage(e));
    }
    contributionDraft = {
      value,
      text: input.value,
      format: JSON.stringify(captureAmountFormat()),
    };
    onchange({ expectedContribution: value });
  }
  // Remove binary display noise without rounding assumptions to whole percentages.
  function percentText(value: unknown, fallback: number) {
    const resolved = value ?? fallback;
    return typeof resolved === "number"
      ? String(Number((resolved * 100).toPrecision(15)))
      : String(resolved);
  }
  function percentInput(input: HTMLInputElement) {
    // Empty drafts must survive reload, rather than becoming a default via JSON null.
    return Number.isFinite(input.valueAsNumber)
      ? input.valueAsNumber / 100
      : "";
  }
</script>

<div class="fields">
  <p>
    {t(
      "This projection estimates when withdrawal-based investment income could cover adjusted expenses. Historical growth includes contributions and other balance changes; it is not an isolated investment return.",
    )}
  </p>
  <p>
    {t(
      "Historical windows end at the previous completed month. Entire history ignores saved bounds; other bounds are clamped to available completed months.",
    )}
  </p>
  {#if busy}<p role="status">{t("Loading selections…")}</p>{/if}
  {#if error}<p role="alert">{error}</p>
    <button type="button" onclick={load}>{t("Retry selections")}</button>{/if}
  <label class="check"
    ><input
      type="checkbox"
      disabled={busy || !!error}
      checked={meta.expenseCategoryIds === undefined}
      onchange={(e) =>
        onchange({
          expenseCategoryIds: e.currentTarget.checked
            ? undefined
            : categories.filter((c) => !c.is_income).map((c) => c.id),
        })}
    />{t("Use default expense categories")}</label
  >
  <label
    >{t("Expenses categories")}<select
      multiple
      disabled={busy || !!error}
      value={selectedCategories}
      onchange={(e) =>
        onchange({
          expenseCategoryIds: Array.from(e.currentTarget.selectedOptions).map(
            (o) => o.value,
          ),
        })}
    >
      {#each categories.filter((c) => !c.is_income || selectedCategories.includes(c.id)) as category (category.id)}<option
          value={category.id}>{category.name}</option
        >{/each}
      {#each selectedCategories.filter((id) => !categories.some((c) => c.id === id)) as id (id)}<option
          value={id}
          >{t("Unavailable category (reference preserved)")} {id}</option
        >{/each}
    </select></label
  >
  <p>
    {t(
      "Hidden selections are retained while excluded. No selected expense categories means zero projected expenses. Use Ctrl or Command to change individual items in these lists.",
    )}
  </p>
  <div class="grid">
    <label
      >{t("Safe withdrawal rate (%)")}<input
        type="number"
        min="0"
        required
        step="any"
        value={percentText(meta.safeWithdrawalRate, 0.04)}
        oninput={(e) =>
          onchange({ safeWithdrawalRate: percentInput(e.currentTarget) })}
      /></label
    >
    <label
      >{t("Expense Projection Type")}<select
        value={String(meta.projectionType ?? "hampel")}
        onchange={(e) => onchange({ projectionType: e.currentTarget.value })}
        ><option value="hampel">{t("Hampel Filtered Median")}</option><option
          value="median">{t("Median")}</option
        ><option value="mean">{t("Mean")}</option
        >{#if meta.projectionType && !["hampel", "median", "mean"].includes(String(meta.projectionType))}<option
            value={String(meta.projectionType)}
            >{t("Preserved:")} {String(meta.projectionType)}</option
          >{/if}</select
      ></label
    >
    <label
      >{t("Target Income (% of expenses)")}<input
        type="number"
        min="0"
        required
        step="any"
        value={percentText(meta.expenseAdjustmentFactor, 1)}
        oninput={(e) =>
          onchange({
            expenseAdjustmentFactor: percentInput(e.currentTarget),
          })}
      /></label
    >
  </div>
  <p>
    {t(
      "Mean includes all months, including zero months. Median reduces outliers; Hampel filtering removes values outside three scaled median absolute deviations before taking the median.",
    )}
  </p>
  <label class="check"
    ><input
      type="checkbox"
      checked={custom}
      onchange={(e) =>
        onchange(
          e.currentTarget.checked
            ? {
                estimatedReturn: meta.estimatedReturn ?? 0.06,
                expectedContribution: meta.expectedContribution ?? 0,
              }
            : { estimatedReturn: null, expectedContribution: null },
        )}
    />{t("Use custom growth projections")}</label
  >
  {#if custom}<div class="grid">
      <label
        >{t("Expected return (annual %)")}<input
          type="number"
          min="-100"
          required
          step="any"
          value={percentText(meta.estimatedReturn, 0)}
          oninput={(e) =>
            onchange({ estimatedReturn: percentInput(e.currentTarget) })}
        /></label
      >
      <label
        >{t("Expected monthly contribution")}<input
          {@attach contributionValidity}
          inputmode="decimal"
          required
          value={contributionText}
          oninput={contribution}
        /></label
      >
    </div>{:else}<p>
      {t(
        "Growth uses the historical balance CAGR, including contributions. The default additional monthly contribution is zero.",
      )}
    </p>{/if}
  <p>
    {t(
      "Contributions are added before monthly growth. A zero withdrawal rate may make the target nest egg unavailable; unsafe or nonfinite calculations are reported explicitly. Projections stop at crossover or 600 months.",
    )}
  </p>
</div>

<style>
  .fields {
    display: grid;
    gap: 0.8rem;
    min-width: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }
  input,
  select {
    max-width: 100%;
    min-height: 44px;
  }
  select[multiple] {
    min-height: 9rem;
  }
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .check input {
    width: auto;
    flex: none;
    margin: 0;
  }
  p {
    max-width: 70ch;
    margin: 0.2rem 0;
  }
</style>
