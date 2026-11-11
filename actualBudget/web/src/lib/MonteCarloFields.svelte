<script lang="ts">
  import { t } from "./i18n.svelte";
  import { uuid, type Account } from "./api";
  import {
    fields,
    planFields,
    planInputs,
    type Collection,
    type Field,
    type PlanEntry,
  } from "./monte-carlo";
  import {
    createMonteCarloPot,
    createMonteCarloSurplusPot,
    createMonteCarloIncomeStream,
    createMonteCarloContribution,
    createMonteCarloSpendingPhase,
    createMonteCarloTaxBand,
    ALLOCATION_PRESETS,
    getPotAssetWeights,
  } from "./monte-carlo-vendor/configuration.js";
  import MonteCarloInput from "./MonteCarloInput.svelte";
  let {
    meta,
    accounts,
    onchange,
  }: {
    meta: Record<string, unknown>;
    accounts: Account[];
    onchange: (patch: Record<string, unknown>) => void;
  } = $props();
  let plan = $derived(planInputs(meta));
  let expanded = $state<Record<Collection, boolean>>({
    pots: true,
    incomeStreams: false,
    contributions: false,
    spendingPhases: true,
    taxBands: false,
  });
  const sections: [Collection, string, string][] = [
    ["pots", "Investment pots", "Add pot"],
    ["incomeStreams", "Income", "Add income"],
    ["contributions", "Contributions", "Add contribution"],
    ["spendingPhases", "Spending phases", "Add spending phase"],
    ["taxBands", "Tax bands", "Add tax band"],
  ];
  function add(collection: Collection) {
    const id = uuid();
    const row =
      collection === "pots"
        ? createMonteCarloPot(id)
        : collection === "incomeStreams"
          ? createMonteCarloIncomeStream(id)
          : collection === "contributions"
            ? createMonteCarloContribution(
                id,
                plan.pots.find((p) => !p.isSurplus)?.id ?? "",
              )
            : collection === "spendingPhases"
              ? createMonteCarloSpendingPhase(id, plan.currentAge)
              : createMonteCarloTaxBand(id);
    onchange({ [collection]: [...plan[collection], row] });
  }
  function update(
    collection: Collection,
    id: string,
    key: string,
    value: unknown,
  ) {
    onchange({
      [collection]: plan[collection].map((row) => {
        if (row.id !== id) return row;
        const next = { ...row, [key]: value };
        if (collection === "pots" && key === "allocationPreset") {
          if (value === "custom-mix") {
            const weights =
              row.allocationPreset !== "custom-mix"
                ? getPotAssetWeights(row)
                : null;
            if (weights)
              Object.assign(next, {
                allocationStocks: weights.stocks,
                allocationBonds: weights.bonds,
                allocationCash: weights.cash,
              });
          } else if (typeof value === "string" && ALLOCATION_PRESETS[value]) {
            const preset = ALLOCATION_PRESETS[value];
            Object.assign(next, {
              expectedReturnMean: preset.mean,
              returnStdDev: preset.stdDev,
            });
          }
        }
        return next;
      }),
    });
  }
  function selection(field: Field): Field {
    if (field.key === "accountId")
      return {
        ...field,
        choices: accounts.filter((a) => !a.deleted).map((a) => [a.id, a.name]),
      };
    if (field.key === "potId")
      return {
        ...field,
        choices: plan.pots
          .filter((p) => !p.isSurplus)
          .map((p, i) => [p.id, String(p.name || `${t("Pot")} ${i + 1}`)]),
      };
    if (field.key === "sourceIncomeStreamId")
      return {
        ...field,
        choices: plan.incomeStreams.map((p, i) => [
          p.id,
          String(p.name || `${t("Income")} ${i + 1}`),
        ]),
      };
    return field;
  }
  function visible(field: Field, row: PlanEntry) {
    if (row.isSurplus)
      return (
        [
          "name",
          "allocationPreset",
          "allocationStocks",
          "allocationBonds",
          "allocationCash",
          "expectedReturnMean",
          "returnStdDev",
        ].includes(field.key) &&
        (!field.key.startsWith("allocation") ||
          field.key === "allocationPreset" ||
          row.allocationPreset === "custom-mix")
      );
    if (
      ["allocationStocks", "allocationBonds", "allocationCash"].includes(
        field.key,
      )
    )
      return row.allocationPreset === "custom-mix";
    if (["taxableFraction"].includes(field.key))
      return plan.taxModel === "bands";
    if (["withdrawalTaxRate", "taxRate"].includes(field.key))
      return plan.taxModel !== "bands";
    return true;
  }
  function remove(collection: Collection, id: string) {
    const patch: Record<string, unknown> = {
      [collection]: plan[collection].filter((p) => p.id !== id),
    };
    // Keep references visible rather than silently reassigning contributed money.
    onchange(patch);
  }
</script>

<section aria-label={t("Monte Carlo plan inputs")}>
  <p>
    {t(
      "Estimates use your assumptions and do not change recorded balances. Save the plan, then choose Run simulation. Presets are illustrative, not forecasts.",
    )}
  </p>
  <div class="fields">
    {#each planFields as field (field.key)}<div>
        <MonteCarloInput
          {field}
          value={plan[field.key]}
          onchange={(value) => onchange({ [field.key]: value })}
        />
      </div>{/each}
  </div>
  <p>
    {t("Withdrawals use the saved strategy:")}
    {String(plan.withdrawalStrategy)}. {t("Withdrawal rule:")}
    {String(plan.withdrawalRule.type)}.
  </p>
  {#each sections as [collection, title, addLabel] (collection)}
    {#if collection !== "taxBands" || plan.taxModel === "bands"}
      <details bind:open={expanded[collection]}>
        <summary>{t(title)} ({plan[collection].length})</summary>
        {#if collection === "pots"}
          <p>
            {t(
              "Surplus cash starts at zero, keeps unspent income and is spent before other pots. It is always accessible, untaxed and fee-free. Without a surplus pot, unspent income leaves the plan.",
            )}
          </p>
          <label class="check"
            ><input
              type="checkbox"
              checked={plan.pots.some((p) => p.isSurplus)}
              disabled={plan.pots.length === 1 &&
                plan.pots[0].isSurplus === true}
              onchange={(e) =>
                onchange({
                  pots: e.currentTarget.checked
                    ? [createMonteCarloSurplusPot(uuid()), ...plan.pots]
                    : plan.pots.filter((p) => !p.isSurplus),
                })}
            />{t("Keep surplus income")}</label
          >
        {:else if collection === "incomeStreams" || collection === "contributions"}<p
          >
            {t(
              "Blank ages mean the start or end of the plan. Age windows include both endpoints.",
            )}
          </p>{/if}
        {#each plan[collection] as row, index (row.id)}
          <fieldset>
            <legend
              >{row.isSurplus
                ? t("Surplus cash")
                : `${t(title)} ${index + 1}`}</legend
            >
            <div class="fields">
              {#each fields[collection].filter( (field) => visible(field, row) ) as field (field.key)}<div
                >
                  <MonteCarloInput
                    field={selection(field)}
                    value={row[field.key]}
                    onchange={(value) =>
                      update(collection, row.id, field.key, value)}
                  />
                </div>{/each}
            </div>
            {#if collection === "pots" && plan.returnModel !== "normal" && !getPotAssetWeights(row)}<p
                role="status"
              >
                {t(
                  "This pot has no complete historical asset mix. It uses its normal return assumptions instead. Custom asset shares must total 100%.",
                )}
              </p>{/if}
            <button
              type="button"
              onclick={() => remove(collection, row.id)}
              disabled={(collection === "pots" ||
                collection === "spendingPhases" ||
                collection === "taxBands") &&
                plan[collection].length <= 1}
              >{t("Remove")}
              {row.isSurplus
                ? t("Surplus cash")
                : `${t(title)} ${index + 1}`}</button
            >
          </fieldset>
        {/each}
        {#if !plan[collection].length}<p>
            {t("No entries in this plan.")}
          </p>{/if}
        <button
          type="button"
          onclick={() => add(collection)}
          disabled={plan[collection].length >= 256}>{t(addLabel)}</button
        >
      </details>
    {/if}
  {/each}
</section>

<style>
  section,
  details,
  fieldset {
    min-width: 0;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
  }
  details {
    border-top: 1px solid var(--line);
    padding: 1rem 0;
    margin-top: 1rem;
  }
  summary {
    cursor: pointer;
    font-weight: 600;
    min-height: 44px;
  }
  fieldset {
    margin: 1rem 0;
    padding: 1rem;
    border: 1px solid var(--line);
  }
  fieldset button {
    margin-top: 1rem;
  }
  p {
    max-width: 70ch;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
    min-height: 44px;
  }
  button {
    min-height: 44px;
  }
</style>
