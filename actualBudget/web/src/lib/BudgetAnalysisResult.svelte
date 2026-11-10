<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import { errorMessage } from "./api";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import { displayDate } from "./dates";
  import {
    inspectAnalysis,
    analysisCsv,
    type AnalysisProjection,
    type AnalysisQuery,
    type AnalysisDetails,
    type AnalysisRow,
  } from "./budget-analysis";
  let {
    budgetId,
    query,
    projection,
    meta,
    cached = false,
    disconnected = false,
  }: {
    budgetId: string;
    query: AnalysisQuery;
    projection: AnalysisProjection;
    meta: Record<string, unknown>;
    cached?: boolean;
    disconnected?: boolean;
  } = $props();
  let page = $state(0),
    details = $state.raw<AnalysisDetails | null>(null),
    busy = $state(false),
    error = $state(""),
    month = $state(""),
    metric = $state<"spent" | "budgeted">("spent");
  let alive = true,
    generation = 0;
  let privateAmounts = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== projection.epoch ||
      preferences.shared.privacy_mode,
  );
  let rows = $derived(projection.result.intervalData),
    visible = $derived(rows.slice(page * 50, page * 50 + 50));
  let bars = $derived(
      (meta.graphType === undefined || meta.graphType === "Bar") &&
        meta.balanceOnly !== true,
    ),
    knownStyle = $derived(
      meta.graphType === undefined ||
        ["Line", "Bar"].includes(String(meta.graphType)),
    ),
    draw = $derived(knownStyle && (!bars || rows.length * 3 <= 1000));
  let columns = $derived([
    { key: "budgeted", label: t("Budgeted"), color: "var(--accent)" },
    { key: "spent", label: t("Spent"), color: "var(--error)" },
    {
      key: "overspendingAdjustment",
      label: t("Overspending Adjustment"),
      color: "var(--muted)",
    },
    { key: "balance", label: t("Balance"), color: "var(--text)" },
  ]);
  let series = $derived.by(() => {
    const selected =
      meta.balanceOnly === true
        ? columns.slice(3)
        : columns.filter(
            (column) => column.key !== "balance" || meta.showBalance !== false,
          );
    const values = rows.flatMap((row) =>
        selected.map(
          (column) => row[column.key as keyof AnalysisRow] as number,
        ),
      ),
      min = Math.min(0, ...values),
      max = Math.max(1, ...values),
      y = (value: number) => 180 - (160 * (value - min)) / (max - min),
      width = 560 / Math.max(1, rows.length);
    return selected.map((column, index) => ({
      ...column,
      firstY: y((rows[0]?.[column.key as keyof AnalysisRow] as number) ?? 0),
      dash: index ? `${index * 3 + 2} 3` : undefined,
      path: rows
        .map(
          (row, i) =>
            `${i ? "L" : "M"}${20 + (560 * i) / Math.max(1, rows.length - 1)},${y(row[column.key as keyof AnalysisRow] as number)}`,
        )
        .join(" "),
      rects:
        bars && column.key !== "balance" && draw
          ? rows.map((row, i) => {
              const value = row[column.key as keyof AnalysisRow] as number;
              return {
                id: row.date,
                x: 20 + i * width + ((index % 3) * width) / 3,
                y: Math.min(y(0), y(value)),
                width: Math.max(0.5, width / 3 - 1),
                height: Math.abs(y(value) - y(0)),
              };
            })
          : [],
    }));
  });
  const money = (value: number) =>
    privateAmounts ? "••••" : formatAmount(value);
  async function inspect(
    value: string,
    kind: "spent" | "budgeted",
    offset = 0,
  ) {
    if (cached || disconnected) return;
    const current = ++generation;
    month = value;
    metric = kind;
    busy = true;
    error = "";
    details = null;
    try {
      const response = await inspectAnalysis(
        budgetId,
        query,
        projection.snapshot,
        value,
        kind,
        offset,
      );
      if (alive && current === generation) details = response;
    } catch (e) {
      if (alive && current === generation)
        error =
          errorMessage(e) +
          " " +
          t("Recalculate the analysis for current details.");
    } finally {
      if (alive && current === generation) busy = false;
    }
  }
  function exportCsv() {
    try {
      const csv = analysisCsv(projection.result, [
          t("Month"),
          t("Budgeted"),
          t("Spent"),
          t("Overspending Adjustment"),
          t("Balance"),
        ]),
        url = URL.createObjectURL(
          new Blob([csv], { type: "text/csv;charset=utf-8" }),
        ),
        anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `budget-analysis-${projection.result.startDate}-${projection.result.endDate}${cached ? "-cached" : ""}.csv`;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

<div class="actions">
  <button onclick={exportCsv}>{t("Export as CSV")}</button
  >{#if privateAmounts}<p>{t("CSV includes unmasked amounts.")}</p>{/if}
</div>
<dl>
  <div>
    <dt>{t("Budgeted")}</dt>
    <dd>{money(projection.result.totalBudgeted)}</dd>
  </div>
  <div>
    <dt>{t("Spent")}</dt>
    <dd>{money(projection.result.totalSpent)}</dd>
  </div>
  <div>
    <dt>{t("Overspending Adjustment")}</dt>
    <dd>{money(projection.result.totalOverspendingAdjustment)}</dd>
  </div>
  <div>
    <dt>{t("Ending balance:")}</dt>
    <dd>{money(rows.at(-1)?.balance ?? 0)}</dd>
  </div>
</dl>
{#if projection.result.finalOverspendingAdjustment < 0}<p>
    {t("Uncarried overspending at period end:")}
    {money(projection.result.finalOverspendingAdjustment)}
  </p>{/if}
{#if privateAmounts}<p>
    {t("Financial plot hidden for privacy.")}
  </p>{:else if !knownStyle}<p>
    {t(
      "The saved chart style is unavailable. Exact monthly values and CSV remain available.",
    )}
  </p>{:else if !draw}<p>
    {t(
      "This bar chart exceeds 1,000 bars. Choose a line chart or a shorter range; exact values and CSV remain available.",
    )}
  </p>{:else if rows.length}
  <svg
    viewBox="0 0 600 200"
    role="img"
    aria-label={t("Monthly budget analysis. Exact values follow in the table.")}
  >
    {#each series as column (column.key)}{#if column.rects.length}{#each column.rects as rect (rect.id)}<rect
            x={rect.x}
            y={rect.y}
            width={rect.width}
            height={rect.height}
            fill={column.color}
          />{/each}{:else}<path
          d={column.path}
          fill="none"
          stroke={column.color}
          stroke-width="2.5"
          stroke-dasharray={column.dash}
        />{#if rows.length === 1}<circle
            cx="20"
            cy={column.firstY}
            r="4"
            fill={column.color}
          />{/if}{/if}{/each}
  </svg>
  <ul class="legend" aria-label={t("Chart series")}>
    {#each series as column (column.key)}<li>
        <svg viewBox="0 0 30 12" aria-hidden="true"
          ><path
            d="M1 6H29"
            stroke={column.color}
            stroke-width="3"
            stroke-dasharray={column.dash}
          /></svg
        >{column.label}
      </li>{/each}
  </ul>
{/if}
<p class="muted">
  {t(
    "Chart options affect the plot. Exact values and CSV retain all columns. Select Budgeted or Spent for the underlying monthly records.",
  )}
</p>
<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
<div
  class="scroll"
  tabindex="0"
  role="region"
  aria-label={t("Monthly budget analysis table")}
>
  <table aria-label={t("Budget analysis values")}>
    <thead
      ><tr
        ><th>{t("Month")}</th>{#each columns as column (column.key)}<th
            >{column.label}</th
          >{/each}</tr
      ></thead
    ><tbody
      >{#each visible as row (row.date)}<tr
          ><th>{row.date}</th><td
            ><button
              disabled={disconnected || cached || busy}
              onclick={() => inspect(row.date, "budgeted")}
              aria-label={t("Inspect planned allocations") + " " + row.date}
              >{money(row.budgeted)}</button
            ></td
          ><td
            ><button
              disabled={disconnected || cached || busy}
              onclick={() => inspect(row.date, "spent")}
              aria-label={t("Inspect recorded spending") + " " + row.date}
              >{money(row.spent)}</button
            ></td
          ><td>{money(row.overspendingAdjustment)}</td><td
            >{money(row.balance)}</td
          ></tr
        >{/each}</tbody
    >
  </table>
</div>
<div class="pager">
  <button disabled={!page} onclick={() => page--}>{t("Previous months")}</button
  ><span
    >{page * 50 + 1}–{Math.min((page + 1) * 50, rows.length)} / {rows.length}</span
  ><button disabled={(page + 1) * 50 >= rows.length} onclick={() => page++}
    >{t("Next months")}</button
  >
</div>
{#if error}<p role="alert">{t(error)}</p>{/if}
{#if month}<section aria-label={t("Budget analysis details")}>
    <header>
      <h4>
        {metric === "spent" ? t("Recorded spending") : t("Planned allocations")} ·
        {month}
      </h4>
      <button
        onclick={() => {
          month = "";
          details = null;
          generation++;
          busy = false;
        }}>{t("Close details")}</button
      >
    </header>
    {#if busy}<p role="status">{t("Loading details…")}</p>{/if}
    {#if details}<p>
        {details.basis === "budget_allocations"
          ? t(
              "Planned category allocations contributing to this month’s Budgeted total.",
            )
          : t(
              "Recorded leaf entries contributing to this month’s Spent total. Transfers and splits follow the budget’s actual category amounts.",
            )}
      </p>
      <p>
        {t("Total")}: {money(details.total_amount)} · {details.total_count}
        {t("records")}
      </p>
      {#if !details.rows.length}<p>
          {t("No contributing records for this selection.")}
        </p>{:else}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
        <div
          class="scroll"
          tabindex="0"
          role="region"
          aria-label={t("Analysis record table")}
        >
          <table>
            <thead
              ><tr
                ><th>{t("Date")}</th
                >{#if details.basis === "recorded_entries"}<th
                    >{t("Account")}</th
                  ><th>{t("Payee")}</th>{/if}<th>{t("Category")}</th><th
                  >{t("Amount")}</th
                >{#if details.basis === "recorded_entries"}<th>{t("Notes")}</th
                  >{/if}</tr
              ></thead
            ><tbody
              >{#each details.rows as row (row.id)}<tr
                  ><td
                    >{row.date
                      ? displayDate(row.date, preferences.shared.date_format)
                      : row.month}</td
                  >{#if details.basis === "recorded_entries"}<td
                      >{row.account_name}</td
                    ><td>{row.payee_name || t("No payee")}</td>{/if}<td
                    >{row.category_name}</td
                  ><td>{money(row.amount)}</td
                  >{#if details.basis === "recorded_entries"}<td>{row.notes}</td
                    >{/if}</tr
                >{/each}</tbody
            >
          </table>
        </div>
      {/if}
      <div class="pager">
        <button
          disabled={!details.offset || busy || disconnected || cached}
          onclick={() =>
            inspect(
              month,
              metric,
              Math.max(0, details!.offset - details!.limit),
            )}>{t("Previous details")}</button
        ><span
          >{details.offset + 1}–{details.offset + details.rows.length} / {details.total_count}</span
        ><button
          disabled={!details.has_more || busy || disconnected || cached}
          onclick={() =>
            inspect(month, metric, details!.offset + details!.limit)}
          >{t("Next details")}</button
        >
      </div>
    {/if}
  </section>{/if}
<details>
  <summary>{t("Assumptions and limits")}</summary
  >{#each projection.assumptions as assumption (assumption.code)}<p>
      {t(assumption.message)}
    </p>{/each}
  <p>
    {t(
      "Uncategorized entries are excluded. Overspending adjustments explain category resets and do not create entries.",
    )}
  </p>
</details>

<style>
  dl {
    display: flex;
    gap: 1rem 2rem;
    flex-wrap: wrap;
    margin: 1.2rem 0;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0.35rem 0;
    font-variant-numeric: tabular-nums;
  }
  svg {
    display: block;
    width: 100%;
    max-height: 260px;
    margin: 1rem 0;
  }
  .legend {
    display: flex;
    gap: 0.6rem 1rem;
    flex-wrap: wrap;
    list-style: none;
    padding: 0;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .legend svg {
    width: 30px;
    height: 12px;
    margin: 0;
  }
  .scroll {
    max-width: 100%;
    overflow: auto;
    margin-top: 1rem;
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
    text-align: left;
  }
  td {
    white-space: nowrap;
  }
  td:last-child {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .pager,
  .actions,
  header {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    flex-wrap: wrap;
    margin: 0.8rem 0;
  }
  header {
    justify-content: space-between;
  }
  h4 {
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
  p {
    max-width: 70ch;
  }
  button,
  summary {
    min-height: 44px;
  }
  summary {
    padding: 0.65rem 0;
    cursor: pointer;
  }
  details,
  section {
    border-top: 1px solid var(--line);
    margin-top: 1.5rem;
    padding-top: 0.6rem;
  }
</style>
