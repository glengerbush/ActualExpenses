<script lang="ts">
  import { t, languageState } from "./i18n.svelte";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import {
    rankSimulationsWorstFirst,
    buildMonteCarloYearStory,
    buildMonteCarloCashflowChart,
  } from "./monte-carlo-vendor/configuration.js";
  import type {
    MonteCarloProjection,
    PlotSeries,
    RunDetail,
  } from "./monte-carlo";
  import MonteCarloPlot from "./MonteCarloPlot.svelte";
  let {
    budgetId,
    projection,
    selected,
    availableRuns,
    busy,
    disconnected,
    oninspect,
  }: {
    budgetId: string;
    projection: MonteCarloProjection;
    selected: number;
    availableRuns: number[];
    busy: boolean;
    disconnected: boolean;
    oninspect: (index: number) => Promise<void>;
  } = $props();
  let page = $state(0),
    order = $state("worst"),
    requested = $state<number | undefined>();
  let runNumber = $derived(requested ?? selected + 1);
  let result = $derived(projection.result),
    config = $derived(projection.config);
  let ranked = $derived(
    rankSimulationsWorstFirst(
      result.endingBalances,
      result.depletionYearBySimulation,
    ),
  );
  let pages = $derived(Math.max(1, Math.ceil(ranked.length / 20)));
  let currentPage = $derived(Math.min(page, pages - 1));
  let visible = $derived(
    Array.from(
      { length: Math.min(20, ranked.length - currentPage * 20) },
      (_, offset) => {
        const rank = currentPage * 20 + offset;
        return {
          rank,
          index: ranked[order === "best" ? ranked.length - 1 - rank : rank],
        };
      },
    ),
  );
  let awaitingPreferences = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== projection.epoch,
  );
  const money = (n: number) =>
    awaitingPreferences ? "••••" : formatAmount(Math.round(n));
  const percent = (n: number | null) =>
    awaitingPreferences
      ? "••••"
      : n === null
        ? t("None")
        : new Intl.NumberFormat(languageState.language, {
            style: "percent",
            maximumFractionDigits: 4,
          }).format(n);
  const outcome = (index: number) =>
    awaitingPreferences
      ? "••••"
      : result.depletionYearBySimulation[index] === -1
        ? t("Lasted the full horizon")
        : t("Depleted at age {{age}}", {
            age: config.currentAge + result.depletionYearBySimulation[index],
          });
  let rows = $derived(result.runDetail ?? []);
  let explanationsOpen = $state(false),
    expandedYears = $state<Record<number, boolean>>({});
  let cashflowData = $derived(
    buildMonteCarloCashflowChart({
      rows,
      pots: config.pots,
      contributions: config.contributions,
      incomeStreams: config.incomeStreams,
      spendingPhases: config.spendingPhases,
      startAge: config.currentAge,
      translate: t,
    }),
  );
  let cashflow = $derived<PlotSeries[]>(
    [...cashflowData.inflowSeries, ...cashflowData.outflowSeries].map(
      (series) => ({
        ...series,
        values: cashflowData.data.map(
          (point) => point.amounts[series.key] ?? 0,
        ),
      }),
    ),
  );
  function story(row: RunDetail) {
    return buildMonteCarloYearStory({
      row,
      withdrawalRule: config.withdrawalRule,
      surplusPotName: String(
        config.pots.find((pot) => pot.isSurplus)?.name || t("Surplus cash"),
      ),
      hasSurplusPot: config.pots.some((pot) => pot.isSurplus),
      format: money,
      translate: t,
    });
  }
  const ruleValues: [string, string, "money" | "percent" | "number"][] = [
    ["rate", "Anchored withdrawal rate", "percent"],
    ["unclamped", "Rate-based spending", "money"],
    ["floor", "Spending floor", "money"],
    ["ceiling", "Spending ceiling", "money"],
    ["factor", "Cumulative spending multiplier", "number"],
    ["planned", "Spending before rule", "money"],
    ["adjusted", "Spending after rule", "money"],
    ["currentRate", "Current withdrawal rate", "percent"],
    ["referenceRate", "Planned withdrawal rate", "percent"],
    ["ratchetStreak", "Consecutive qualifying years", "number"],
  ];
  const potValues: [keyof RunDetail, string, boolean?][] = [
    ["potStartBalances", "Starting balance"],
    ["potContributions", "Contributions"],
    ["potWithdrawals", "Gross withdrawals"],
    ["potTaxables", "Taxable withdrawal"],
    ["potTaxes", "Withdrawal tax"],
    ["potFees", "Fees"],
    ["potReturns", "Return", true],
    ["potBalances", "Ending balance"],
  ];
</script>

<section aria-label={t("Run inspection")}>
  <details>
    <summary>{t("Individual simulation runs")}</summary>
    <p>
      {t(
        "Outcomes are ranked by ending balance, then depletion timing. Inspect a run to see how its spending was funded.",
      )}
    </p>
    <div class="controls">
      <label
        >{t("Run order")}<select bind:value={order} onchange={() => (page = 0)}
          ><option value="worst">{t("Worst outcomes first")}</option><option
            value="best">{t("Best outcomes first")}</option
          ></select
        ></label
      >
      <label
        >{t("Choose scenario")}<select
          value=""
          onchange={(event) => {
            requested =
              ranked[
                Math.round(
                  Number(event.currentTarget.value) * (ranked.length - 1),
                )
              ] + 1;
          }}
        >
          <option value="" disabled>{t("Choose by outcome")}</option>
          {#each [["0", "Worst run"], ["0.25", "25th percentile"], ["0.5", "Median run"], ["0.75", "75th percentile"], ["1", "Best run"]] as [value, label] (value)}<option
              {value}>{t(label)}</option
            >{/each}
        </select></label
      >
    </div>
    <form
      aria-label={t("Inspect a simulation run")}
      onsubmit={(event) => {
        event.preventDefault();
        void oninspect(runNumber - 1);
      }}
    >
      <label
        >{t("Run number")}<input
          type="number"
          min="1"
          max={result.simulationCount}
          step="1"
          required
          value={runNumber}
          oninput={(event) => (requested = event.currentTarget.valueAsNumber)}
        /></label
      >
      <button
        disabled={busy ||
          (disconnected && !availableRuns.includes(runNumber - 1))}
        >{t("Inspect run")}</button
      >
    </form>
    <p>
      {t(
        "Recent runs can open without recalculating. Offline, only retained runs are available. The last inspected run is saved across reloads.",
      )}
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard access to horizontally scrolling data.) -->
    <div
      class="table-scroll"
      role="region"
      aria-label={t("Simulation runs")}
      tabindex="0"
    >
      <table aria-label={t("Simulation runs")}>
        <thead
          ><tr
            >{#each ["Rank", "Run", "Outcome", "Ending balance", "Total withdrawn", "Details"] as label (label)}<th
                scope="col">{t(label)}</th
              >{/each}</tr
          ></thead
        >
        <tbody
          >{#each visible as item (item.index)}<tr>
              <td>{item.rank + 1}</td><th scope="row">{item.index + 1}</th><td
                >{outcome(item.index)}</td
              ><td>{money(result.endingBalances[item.index])}</td><td
                >{money(result.totalWithdrawnBySimulation[item.index])}</td
              >
              <td
                ><button
                  disabled={busy ||
                    (disconnected && !availableRuns.includes(item.index))}
                  onclick={() => {
                    requested = item.index + 1;
                    void oninspect(item.index);
                  }}
                  >{t("Inspect run {{number}}", {
                    number: item.index + 1,
                  })}</button
                ></td
              >
            </tr>{/each}</tbody
        >
      </table>
    </div>
    <div class="controls">
      <button
        disabled={currentPage === 0}
        onclick={() => (page = currentPage - 1)}>{t("Previous runs")}</button
      >
      <p>
        {t("Page {{page}} of {{count}}", {
          page: currentPage + 1,
          count: pages,
        })}
      </p>
      <button
        disabled={currentPage + 1 >= pages}
        onclick={() => (page = currentPage + 1)}>{t("Next runs")}</button
      >
    </div>
  </details>
  {#if rows.length}
    <h4>{t("Run {{number}} cash flow", { number: selected + 1 })}</h4>
    <p>
      {outcome(selected)} · {t("Ending balance:")}
      {money(result.endingBalances[selected])} · {t("Total withdrawn:")}
      {money(result.totalWithdrawnBySimulation[selected])}
    </p>
    <p>
      {t(
        "This scenario uses the balances captured when the plan ran. Money in is above zero; planned spending, tax, contributions and saved surplus are below. Fees reduce investments, so they are shown in the table, not the cash-flow bars. Years after depletion show unfunded spending.",
      )}
    </p>
    <MonteCarloPlot
      title="Annual cash flow"
      series={cashflow}
      ages={rows.map((row) => config.currentAge + row.year - 1)}
      {awaitingPreferences}
      stacked
    />
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard access to horizontally scrolling data.) -->
    <div
      class="table-scroll"
      role="region"
      aria-label={t("Annual cash flow")}
      tabindex="0"
    >
      <table aria-label={t("Annual cash flow")}>
        <thead
          ><tr
            >{#each ["Age", "Gross income", "Gross withdrawals", "Planned spending", "Actual spending", "Tax", "Contributions", "Saved surplus", "Fees", "Ending balance", "Funding"] as label (label)}<th
                scope="col">{t(label)}</th
              >{/each}</tr
          ></thead
        >
        <tbody
          >{#each rows as row (row.year)}<tr
              ><th scope="row">{config.currentAge + row.year - 1}</th>
              <td>{money(row.income)}</td><td>{money(row.withdrawal)}</td><td
                >{money(row.plannedSpending)}</td
              ><td>{money(row.spent)}</td><td
                >{money(row.incomeTax + row.taxPaid)}</td
              ><td>{money(row.contributions)}</td><td
                >{money(row.surplusSaved)}</td
              ><td>{money(row.feesPaid)}</td><td>{money(row.endBalance)}</td><td
                >{awaitingPreferences
                  ? "••••"
                  : t(
                      row.afterDepletion
                        ? "After depletion"
                        : row.spent < row.plannedSpending
                          ? "Spending shortfall"
                          : "Funded year",
                    )}</td
              >
            </tr>{/each}</tbody
        >
      </table>
    </div>
    <details bind:open={explanationsOpen}>
      <summary>{t("Year-by-year explanation")}</summary>
      {#if awaitingPreferences}<p>
          {t("Loading display preferences…")}
        </p>{:else if explanationsOpen}
        {#each rows.filter((row) => !row.afterDepletion) as row (row.year)}
          <details class="year" bind:open={expandedYears[row.year]}>
            <summary
              >{t("Age")}
              {config.currentAge + row.year - 1} · {t("Spent:")}
              {money(row.spent)}</summary
            >
            {#if expandedYears[row.year]}
              {#each story(row) as sentence (sentence)}<p>{sentence}</p>{/each}
              <dl>
                {#each [["Starting balance", row.startBalance], ["Contributions", row.contributions], ["Saved surplus", row.surplusSaved], ["Gross withdrawals", row.withdrawal], ["Investment growth", row.growth], ["Fees", row.feesPaid], ["Ending balance", row.endBalance], ["Planned spending", row.plannedSpending], ["Actual spending", row.spent], ["Gross income", row.income], ["Income tax", row.incomeTax], ["Withdrawal tax", row.taxPaid], ["Unspent income", row.unspentIncome], ["Inaccessible balance", row.inaccessibleBalance ?? 0]] as [label, value] (label)}<div
                  >
                    <dt>{t(String(label))}</dt>
                    <dd>{money(Number(value))}</dd>
                  </div>{/each}
                <div>
                  <dt>{t("Inflation")}</dt>
                  <dd>{percent(row.inflation)}</dd>
                </div>
              </dl>
              {#if row.ruleExplanation}<h5>{t("Withdrawal decision")}</h5>
                <dl>
                  {#each ruleValues.filter(([key]) => typeof row.ruleExplanation?.[key] === "number") as [key, label, kind] (key)}<div
                    >
                      <dt>{t(label)}</dt>
                      <dd>
                        {kind === "money"
                          ? money(Number(row.ruleExplanation[key]))
                          : kind === "percent"
                            ? percent(Number(row.ruleExplanation[key]))
                            : row.ruleExplanation[key]}
                      </dd>
                    </div>{/each}
                </dl>{/if}
              {#each config.pots as pot, index (pot.id)}<h5>
                  {String(pot.name || `${t("Pot")} ${index + 1}`)}
                </h5>
                <dl>
                  {#each potValues as [key, label, rate] (key)}{@const value =
                      (row[key] as (number | null)[])[index] ?? null}
                    <div>
                      <dt>{t(label)}</dt>
                      <dd>{rate ? percent(value) : money(value ?? 0)}</dd>
                    </div>{/each}
                </dl>{/each}
              {#if config.incomeStreams.length}<h5>{t("Income streams")}</h5>
                <dl>
                  {#each config.incomeStreams as income, index (income.id)}<div>
                      <dt>
                        {String(income.name || `${t("Income")} ${index + 1}`)}
                      </dt>
                      <dd>{money(row.incomeAmounts[index] ?? 0)}</dd>
                    </div>{/each}
                </dl>{/if}
              {#if config.contributions.length}<h5>
                  {t("Contribution deposits")}
                </h5>
                <dl>
                  {#each config.contributions as contribution, index (contribution.id)}<div
                    >
                      <dt>
                        {String(
                          contribution.name ||
                            `${t("Contribution")} ${index + 1}`,
                        )}
                      </dt>
                      <dd>{money(row.contributionAmounts[index] ?? 0)}</dd>
                    </div>{/each}
                </dl>{/if}
            {/if}
          </details>
        {/each}
      {/if}
    </details>
  {:else}<p>{t("Choose a run to inspect its cash flow.")}</p>{/if}
</section>

<style>
  section {
    min-width: 0;
  }
  .controls,
  form {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.75rem;
    margin: 1rem 0;
  }
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }
  input,
  select,
  button {
    min-height: 44px;
    max-width: 100%;
  }
  .table-scroll {
    overflow-x: auto;
    max-width: 100%;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    padding: 0.6rem;
    border-bottom: 1px solid var(--line);
    text-align: right;
    white-space: nowrap;
  }
  th:first-child {
    text-align: left;
  }
  summary {
    min-height: 44px;
    cursor: pointer;
    padding-top: 0.8rem;
  }
  h4 {
    font-size: 1.15rem;
    margin: 1.5rem 0 0.5rem;
  }
  h5 {
    font-size: 1rem;
    margin: 1.2rem 0 0.4rem;
  }
  .year {
    border-bottom: 1px solid var(--line);
    padding: 0.5rem 0;
  }
  dl {
    margin: 0.5rem 0 1rem;
  }
  dl > div {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 0.5rem 1rem;
    padding: 0.3rem 0;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  p {
    max-width: 70ch;
  }
</style>
