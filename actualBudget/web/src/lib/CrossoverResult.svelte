<script lang="ts">
  import { t, languageState } from "./i18n.svelte";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import { dateObject } from "./dates";
  import type { CrossoverProjection } from "./crossover";
  let {
    budgetId,
    projection,
  }: { budgetId: string; projection: CrossoverProjection } = $props();
  let page = $state(0),
    projectionOnly = $state(false);
  let result = $derived(projection.result),
    rows = $derived(result.graphData.data),
    visible = $derived(rows.slice(page * 50, page * 50 + 50));
  let privateAmounts = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== projection.epoch ||
      preferences.shared.privacy_mode,
  );
  const money = (value: number | null) =>
    value === null
      ? t("Unavailable")
      : privateAmounts
        ? "••••"
        : formatAmount(value);
  const month = (value: string) =>
    new Intl.DateTimeFormat(languageState.language, {
      month: "short",
      year: "numeric",
      timeZone: "UTC",
    }).format(dateObject(value + "-01"));
  const percentage = (value: number | null) =>
    value === null
      ? t("Unavailable")
      : privateAmounts
        ? "••••"
        : new Intl.NumberFormat(languageState.language, {
            style: "percent",
            maximumFractionDigits: 2,
          }).format(value);
  let plot = $derived(
    rows
      .map((row, index) => ({ row, month: projection.months[index] }))
      .filter((item) => !projectionOnly || item.row.isProjection),
  );
  let series = $derived.by(() => {
    const columns = [
      {
        key: "investmentIncome" as const,
        label: t("Monthly investment income:"),
        color: "var(--accent)",
      },
      { key: "expenses" as const, label: t("Expenses"), color: "var(--error)" },
      {
        key: "adjustedExpenses" as const,
        label: t("Target income:"),
        color: "var(--text)",
      },
    ];
    const values = plot.flatMap(({ row }) =>
      columns.flatMap((column) =>
        typeof row[column.key] === "number" ? [row[column.key]!] : [],
      ),
    );
    const min = Math.min(0, ...values),
      max = Math.max(1, ...values),
      y = (value: number) => 180 - (160 * (value - min)) / (max - min),
      x = (i: number) => 20 + (560 * i) / Math.max(1, plot.length - 1);
    return {
      columns: columns.map((column) => ({
        ...column,
        points: plot
          .flatMap(({ row }, index) =>
            typeof row[column.key] === "number"
              ? [`${x(index)},${y(row[column.key]!)}`]
              : [],
          )
          .join(" "),
      })),
      boundary: x(
        Math.max(
          0,
          plot.findIndex(({ row }) => row.isProjection),
        ),
      ),
      zero: y(0),
      min,
      max,
      crossover:
        plot.findIndex(
          ({ row }) => row.x === result.graphData.crossoverXLabel,
        ) >= 0
          ? x(
              plot.findIndex(
                ({ row }) => row.x === result.graphData.crossoverXLabel,
              ),
            )
          : null,
    };
  });
  let crossoverMonth = $derived.by(() => {
    const index = rows.findIndex(
      (row) => row.x === result.graphData.crossoverXLabel,
    );
    return index >= 0 ? projection.months[index] : null;
  });
</script>

{#if !rows.length}<p>
    {t("Select at least one account to calculate a crossover projection.")}
  </p>{:else}
  <dl>
    <div>
      <dt>{t("Last known balance")}</dt>
      <dd>{money(result.lastKnownBalance)}</dd>
    </div>
    <div>
      <dt>{t("Monthly investment income:")}</dt>
      <dd>{money(result.lastKnownMonthlyIncome)}</dd>
    </div>
    <div>
      <dt>{t("Monthly expenses:")}</dt>
      <dd>{money(result.lastKnownMonthlyExpenses)}</dd>
    </div>
    <div>
      <dt>{t("Historical annual growth")}</dt>
      <dd>{percentage(result.historicalReturn)}</dd>
    </div>
    <div>
      <dt>{t("Years to Retire")}</dt>
      <dd>
        {result.yearsToRetire === null
          ? t("No crossover within 600 projected months")
          : privateAmounts
            ? "••••"
            : new Intl.NumberFormat(languageState.language, {
                maximumFractionDigits: 2,
              }).format(result.yearsToRetire)}
      </dd>
    </div>
    <div>
      <dt>{t("Target Monthly Income")}</dt>
      <dd>{money(result.targetMonthlyIncome)}</dd>
    </div>
    <div>
      <dt>{t("Target Life Savings")}</dt>
      <dd>{money(result.targetNestEgg)}</dd>
    </div>
  </dl>
  {#if crossoverMonth}<p>
      {t("Projected crossover:")}
      {privateAmounts ? "••••" : month(crossoverMonth)}
    </p>{/if}
  <p>
    {t(
      "Income here is a withdrawal-rate estimate, including the historical line. It is not recorded earnings or a guaranteed investment return. Account balances remain separate from this estimate.",
    )}
  </p>
  {#if privateAmounts}<p>{t("Financial plot hidden for privacy.")}</p>{:else}
    <label class="check"
      ><input type="checkbox" bind:checked={projectionOnly} />{t(
        "Show projected months only",
      )}</label
    >
    {#if projectionOnly}<p>
        {t(
          "Only projected months are drawn. All historical and estimated points remain in the table.",
        )}
      </p>{/if}
    <svg
      viewBox="0 0 600 190"
      role="img"
      aria-label={t(
        "Crossover projection. The exact monthly data follows in the table.",
      )}
    >
      <rect
        x={series.boundary}
        y="20"
        width={580 - series.boundary}
        height="160"
        fill="var(--accent)"
        opacity=".06"
      />
      <line
        x1="20"
        x2="580"
        y1={series.zero}
        y2={series.zero}
        stroke="var(--line)"
      />
      <line
        x1={series.boundary}
        x2={series.boundary}
        y1="20"
        y2="180"
        stroke="var(--muted)"
        stroke-dasharray="3 4"
      />
      {#if series.crossover !== null}<line
          x1={series.crossover}
          x2={series.crossover}
          y1="20"
          y2="180"
          stroke="var(--accent)"
          stroke-width="2"
          ><title
            >{t("Projected crossover:")}
            {crossoverMonth ? month(crossoverMonth) : ""}</title
          ></line
        >{/if}
      {#each series.columns as column, index (column.key)}<polyline
          points={column.points}
          stroke={column.color}
          stroke-width="2"
          vector-effect="non-scaling-stroke"
          fill="none"
          stroke-dasharray={index === 1
            ? "6 3"
            : index === 2
              ? "2 3"
              : undefined}
        />{/each}
    </svg>
    <div class="axis">
      <span>{month(plot[0]?.month ?? projection.months[0])}</span><span
        >{month(plot.at(-1)?.month ?? projection.months.at(-1)!)}</span
      >
    </div>
    <p>
      {t("Displayed vertical range:")}
      {money(series.min)}–{money(series.max)}
    </p>
    <ul class="legend">
      {#each series.columns as column, index (column.key)}<li>
          <svg viewBox="0 0 24 6" aria-hidden="true"
            ><line
              x1="0"
              x2="24"
              y1="3"
              y2="3"
              stroke={column.color}
              stroke-width="2"
              stroke-dasharray={index === 1
                ? "6 3"
                : index === 2
                  ? "2 3"
                  : undefined}
            /></svg
          >{column.label}
        </li>{/each}
    </ul>
    <p>
      {t(
        "The shaded area and dashed boundary mark projected months. Expenses show net spending; net refunds can be negative. Adjusted expenses apply only to projections.",
      )}
    </p>
  {/if}
{/if}
<p>{t("Scroll horizontally for all columns on smaller screens.")}</p>
<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial table needs keyboard access.) -->
<div
  class="scroll"
  role="region"
  tabindex="0"
  aria-label={t("Crossover data table")}
>
  <table aria-label={t("Crossover values")}>
    <thead
      ><tr
        ><th>{t("Month")}</th><th>{t("Basis")}</th><th
          >{t("Monthly investment income:")}</th
        ><th>{t("Expenses")}</th><th>{t("Target income:")}</th><th
          >{t("Life savings:")}</th
        ></tr
      ></thead
    ><tbody>
      {#each visible as row, index (projection.months[page * 50 + index])}<tr
          ><th scope="row">{month(projection.months[page * 50 + index])}</th><td
            >{t(row.isProjection ? "(projected)" : "Historical")}</td
          ><td>{money(row.investmentIncome)}</td><td>{money(row.expenses)}</td
          ><td
            >{row.adjustedExpenses === undefined
              ? t("Not applicable")
              : money(row.adjustedExpenses)}</td
          ><td>{money(row.nestEgg)}</td></tr
        >{/each}
    </tbody>
  </table>
</div>
<div class="pager">
  <button disabled={!page} onclick={() => page--}>{t("Previous months")}</button
  ><span
    >{rows.length ? page * 50 + 1 : 0}–{Math.min((page + 1) * 50, rows.length)} /
    {rows.length}</span
  ><button disabled={(page + 1) * 50 >= rows.length} onclick={() => page++}
    >{t("Next months")}</button
  >
</div>
<details>
  <summary>{t("Assumptions and limits")}</summary
  >{#each projection.assumptions as assumption (assumption.code)}<p>
      {t(assumption.message)}
    </p>{/each}
</details>

<style>
  .axis {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    color: var(--muted);
    font-size: 0.875rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .check input {
    width: auto;
    flex: none;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr));
    gap: 1rem;
    margin: 1.2rem 0;
  }
  dt {
    color: var(--muted);
  }
  dl > div {
    min-width: 0;
  }
  dd {
    overflow-wrap: anywhere;
    margin: 0.25rem 0 0;
    font-variant-numeric: tabular-nums;
  }
  p {
    max-width: 70ch;
  }
  svg {
    width: 100%;
    height: auto;
  }
  .legend {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
    padding: 0;
    list-style: none;
  }
  .legend li {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .legend svg {
    width: 24px;
    height: 6px;
    flex: none;
  }
  .scroll {
    overflow: auto;
    max-width: 100%;
    margin-top: 1rem;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    min-width: 48rem;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    padding: 0.65rem;
    border-bottom: 1px solid var(--line);
    text-align: left;
    white-space: nowrap;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin: 1rem 0;
  }
  button,
  summary {
    min-height: 44px;
  }
  button {
    max-width: 100%;
  }
</style>
