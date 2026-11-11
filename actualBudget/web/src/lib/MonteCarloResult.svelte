<script lang="ts">
  import { t, languageState } from "./i18n.svelte";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import type { MonteCarloProjection } from "./monte-carlo";
  import { getPotAssetWeights } from "./monte-carlo-vendor/configuration.js";
  let {
    budgetId,
    projection,
  }: { budgetId: string; projection: MonteCarloProjection } = $props();
  let result = $derived(projection.result);
  let masked = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== projection.epoch ||
      preferences.shared.privacy_mode,
  );
  const money = (value: number) =>
    masked ? "••••" : formatAmount(Math.round(value));
  const percent = (value: number) =>
    masked
      ? "••••"
      : new Intl.NumberFormat(languageState.language, {
          style: "percent",
          maximumFractionDigits: 2,
        }).format(value);
  const age = (year: number | null) =>
    year === null ? t("None") : String(projection.config.currentAge + year);
  const percentiles = [
    "p5",
    "p10",
    "p25",
    "p30",
    "p50",
    "p70",
    "p75",
    "p90",
  ] as const;
</script>

<section aria-label={t("Monte Carlo result")}>
  {#if projection.config.returnModel !== "normal"}
    {#each projection.config.pots as pot, index (pot.id)}
      {#if !getPotAssetWeights(pot)}<p role="status">
          {String(pot.name || `${t("Pot")} ${index + 1}`)}: {t(
            "Uses normal return assumptions because this pot has no complete historical asset mix.",
          )}
        </p>{/if}
    {/each}
  {/if}
  <dl>
    <div>
      <dt>{t("Plans lasting the full horizon")}</dt>
      <dd aria-label={t("Plan success rate")}>{percent(result.successRate)}</dd>
    </div>
    <div>
      <dt>{t("Median ending balance")}</dt>
      <dd aria-label={t("Median ending balance")}>
        {money(result.medianEndingBalance)}
      </dd>
    </div>
    <div>
      <dt>{t("Median total withdrawn")}</dt>
      <dd>{money(result.medianTotalWithdrawn)}</dd>
    </div>
    <div>
      <dt>{t("Simulation runs")}</dt>
      <dd>{result.simulationCount}</dd>
    </div>
    <div>
      <dt>{t("Simulation years")}</dt>
      <dd aria-label={t("Simulation years")}>{result.horizonYears}</dd>
    </div>
    <div>
      <dt>{t("Median depletion age among failed runs")}</dt>
      <dd>{masked ? "••••" : age(result.medianDepletionYear)}</dd>
    </div>
    <div>
      <dt>{t("Earliest / latest depletion age")}</dt>
      <dd>
        {masked
          ? "••••"
          : `${age(result.earliestDepletionYear)} / ${age(result.latestDepletionYear)}`}
      </dd>
    </div>
  </dl>
  <p>
    {t(
      "Percentiles compare simulated outcomes, not a guaranteed range. The median is the 50th percentile. Values are rounded only for display.",
    )}
  </p>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard access to horizontally scrolling data.) -->
  <div
    class="table-scroll"
    role="region"
    aria-label={t("Balance distribution")}
    tabindex="0"
  >
    <table aria-label={t("Balance percentiles by age")}>
      <thead
        ><tr
          ><th scope="col">{t("Age")}</th
          >{#each [5, 10, 25, 30, 50, 70, 75, 90] as percentile (percentile)}<th
              scope="col">{percentile}%</th
            >{/each}</tr
        ></thead
      >
      <tbody
        >{#each result.percentileBands as row (row.year)}<tr
            ><th scope="row">{age(row.year)}</th
            >{#each percentiles as key (key)}<td>{money(row[key])}</td
              >{/each}</tr
          >{/each}</tbody
      >
    </table>
  </div>
  <details>
    <summary>{t("Depletion by age")}</summary>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard access to horizontally scrolling data.) -->
    <div
      class="table-scroll"
      role="region"
      aria-label={t("Depletion distribution")}
      tabindex="0"
    >
      <table>
        <thead
          ><tr
            ><th scope="col">{t("Age")}</th><th scope="col"
              >{t("Runs depleted this year")}</th
            ><th scope="col">{t("Cumulative depletion probability")}</th></tr
          ></thead
        >
        <tbody
          >{#each result.depletionHistogram as row (row.year)}<tr
              ><th scope="row">{age(row.year)}</th><td
                >{masked ? "••••" : row.count}</td
              ><td>{percent(result.depletionProbabilityByYear[row.year])}</td
              ></tr
            >{/each}</tbody
        >
      </table>
    </div>
  </details>
  <details>
    <summary>{t("Assumptions and limitations")}</summary>
    <ul>
      {#each projection.assumptions as assumption (assumption)}<li>
          {t(assumption)}
        </li>{/each}
    </ul>
  </details>
</section>

<style>
  section {
    min-width: 0;
  }
  dl {
    margin: 1rem 0;
  }
  dl > div {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid var(--line);
    padding: 0.5rem 0;
    flex-wrap: wrap;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
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
  p,
  li {
    max-width: 70ch;
  }
</style>
