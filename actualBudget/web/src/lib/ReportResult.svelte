<script lang="ts">
  import CustomReportPlot from "./CustomReportPlot.svelte";
  import { errorMessage } from "./api";
  import { formatAmount } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    displayProjection,
    customMetric,
    array,
    inspectReport,
    type ReportQuery,
    type Projection,
    type Inspection,
  } from "./reports";
  let {
    budgetId,
    query,
    projection,
  }: { budgetId: string; query: ReportQuery; projection: Projection } =
    $props();
  let view = $derived(displayProjection(query, projection)),
    page = $state(0),
    cell = $state(""),
    inspection = $state.raw<Inspection | null>(null),
    busy = $state(false),
    error = $state("");
  let visible = $derived(view.rows.slice(page * 50, page * 50 + 50));
  let percent = $derived(view.columns[0] === "Percentage");
  const colors = [
    "var(--accent)",
    "var(--error)",
    "var(--text)",
    "var(--muted)",
  ];
  let series = $derived.by(() => {
    const columns =
      query.kind === "net-worth"
        ? [view.columns[0]]
        : query.kind === "cash-flow" && query.options.showBalance === false
          ? view.columns.slice(0, 3)
          : view.columns;
    const values = view.rows.flatMap((r) =>
        r.values.filter((v): v is number => v !== null),
      ),
      min = Math.min(0, ...values),
      max = Math.max(1, ...values);
    return columns.map((label, column) => {
      let previous = false;
      const path = view.rows
        .map((row, i) => {
          const value = row.values[column];
          if (value === null) {
            previous = false;
            return "";
          }
          const command = previous ? "L" : "M";
          previous = true;
          return `${command}${20 + (i / Math.max(1, view.rows.length - 1)) * 560},${180 - ((value - min) / (max - min)) * 160}`;
        })
        .join(" ");
      return {
        label,
        path,
        color: colors[column % colors.length],
        dash: column > 3 ? "4 3" : undefined,
        point:
          view.rows.length === 1
            ? 180 -
              (((view.rows[0].values[column] ?? 0) - min) / (max - min)) * 160
            : null,
      };
    });
  });
  let stacked = $derived.by(() => {
    if (query.kind !== "net-worth" || query.options.mode !== "stacked")
      return null;
    const bounds = view.rows.map((r) => ({
      positive: r.values
        .slice(1)
        .reduce<number>((sum, v) => sum + Math.max(0, v ?? 0), 0),
      negative: r.values
        .slice(1)
        .reduce<number>((sum, v) => sum + Math.min(0, v ?? 0), 0),
    }));
    const min = Math.min(0, ...bounds.map((b) => b.negative)),
      max = Math.max(1, ...bounds.map((b) => b.positive)),
      y = (v: number) => 180 - ((v - min) / (max - min)) * 160,
      width = 560 / Math.max(1, view.rows.length);
    const bars = view.rows.flatMap((row, index) => {
      let positive = 0,
        negative = 0;
      return row.values.slice(1).map((v, account) => {
        const value = v ?? 0,
          base = value >= 0 ? positive : negative;
        if (value >= 0) positive += value;
        else negative += value;
        return {
          id: `${index}:${account}`,
          x: 20 + index * width,
          y: Math.min(y(base), y(base + value)),
          width: Math.max(1, width - 1),
          height: Math.abs(y(base) - y(base + value)),
          color: colors[account % colors.length],
        };
      });
    });
    return { bars, labels: view.columns.slice(1) };
  });
  function amount(value: number | null) {
    return value === null
      ? "Unavailable"
      : percent
        ? preferences.shared.privacy_mode
          ? "••••"
          : `${value}%`
        : formatAmount(Math.round(value));
  }
  async function inspect(offset = 0) {
    busy = true;
    error = "";
    try {
      inspection = await inspectReport(
        budgetId,
        query,
        projection.snapshot,
        cell || projection.cells[0]?.id || "",
        offset,
      );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if query.kind === "custom"}
  {#if !preferences.shared.privacy_mode}<CustomReportPlot
      {query}
      {projection}
    />{:else}<p class="muted">Financial plot hidden for privacy.</p>{/if}
  <p class="summary-value">
    {amount(
      customMetric(
        projection.result,
        String(query.options.balanceTypeOp || "totalDebts"),
      ),
    )}
  </p>
  <p class="muted">
    Report total · {["totalDebts", "netDebts"].includes(
      String(query.options.balanceTypeOp),
    )
      ? "Payments are shown as positive magnitudes; details retain recorded signs."
      : "Values retain their signs."}
  </p>
{:else if query.kind === "summary"}
  <p class="summary-value">{amount(view.rows[0]?.values[0] ?? null)}</p>
  <p class="muted">
    {view.rows[0]
      ?.label}{#if ["avgPerMonth", "avgPerYear", "avgPerTransact"].includes(String(query.options.summary && (query.options.summary as Record<string, unknown>).type))}
      · Average from recorded entries{/if}
  </p>
{:else if query.kind === "calendar"}
  {#each array(projection.result.calendarData) as month (String(month.start))}<h4
    >
      {String(month.start).slice(0, 7)}
    </h4>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable calendar needs keyboard access.) -->
    <div
      class="scroll-region"
      tabindex="0"
      role="region"
      aria-label="Transaction calendar"
    >
      <div class="calendar-grid">
        {#each array(month.data) as day (String(day.date))}<div
            class="calendar-day"
          >
            <strong>{String(day.date).slice(8, 10)}</strong><span
              >In {amount(Number(day.incomeValue))}</span
            ><span>Out {amount(Number(day.expenseValue))}</span>
          </div>{/each}
      </div>
    </div>{/each}
{:else if !preferences.shared.privacy_mode && stacked}<svg
    viewBox="0 0 600 200"
    role="img"
    aria-label="Stacked account balances; exact account values in table below"
    >{#each stacked.bars as bar (bar.id)}<rect
        x={bar.x}
        y={bar.y}
        width={bar.width}
        height={bar.height}
        fill={bar.color}
      />{/each}</svg
  >
  <p class="muted">
    Account balances stacked above and below zero. Exact account names and
    balances appear in the table.
  </p>
{:else if !preferences.shared.privacy_mode && view.rows.length}
  <svg
    viewBox="0 0 600 200"
    role="img"
    aria-label="Recorded history plot; exact values in table below"
    ><line
      x1="20"
      y1="180"
      x2="580"
      y2="180"
      stroke="currentColor"
      opacity="0.3"
    />{#each series as line (line.label)}<path
        d={line.path}
        fill="none"
        stroke={line.color}
        stroke-width="2"
        stroke-dasharray={line.dash}
      />{#if line.point !== null}<circle
          cx="20"
          cy={line.point}
          r="3"
          fill={line.color}
        />{/if}{/each}</svg
  >
  <ul class="legend">
    {#each series as line (line.label)}<li style:color={line.color}>
        {line.label}
      </li>{/each}
  </ul>
{:else if preferences.shared.privacy_mode}<p class="muted">
    Financial plot hidden for privacy.
  </p>{/if}
<details
  open={query.kind === "custom" &&
    ["Table", "TableGraph"].includes(String(query.options.graphType))}
>
  <summary>Exact values</summary>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
  <div
    class="scroll-region"
    tabindex="0"
    role="region"
    aria-label="Historical report values"
  >
    <table>
      <thead
        ><tr
          ><th scope="col"
            >{query.kind === "custom" ? "Group / period" : "Period"}</th
          >{#each view.columns as column, columnIndex (columnIndex)}<th
              scope="col"
              class="amount">{column}</th
            >{/each}</tr
        ></thead
      ><tbody
        >{#each visible as row (row.id)}<tr
            ><th scope="row">{row.label}</th
            >{#each row.values as value, index (index)}<td class="amount"
                >{amount(value)}</td
              >{/each}</tr
          >{/each}</tbody
      >
    </table>
  </div>
  {#if view.rows.length > 50}<div class="actions">
      <button disabled={page === 0} onclick={() => page--}
        >Previous values</button
      ><span
        >{page * 50 + 1}–{Math.min((page + 1) * 50, view.rows.length)} of {view
          .rows.length}</span
      ><button
        disabled={(page + 1) * 50 >= view.rows.length}
        onclick={() => page++}>Next values</button
      >
    </div>{/if}
</details>
{#if projection.cells.length}<details>
    <summary
      >{projection.basis === "budget_allocations"
        ? "Inspect budget allocations"
        : "Inspect recorded entries"}</summary
    >
    <label
      >Report value<select bind:value={cell} disabled={busy}
        >{#each projection.cells as value (value.id)}<option value={value.id}
            >{value.label}</option
          >{/each}</select
      ></label
    ><button disabled={busy} onclick={() => inspect()}>Load entries</button>
    {#if error}<p role="alert">{error}</p>{/if}
    {#if inspection}<p>
        {inspection.total_count}
        {inspection.record_kind === "budget_allocations"
          ? "budget allocations"
          : "recorded entries"} · Sum {formatAmount(inspection.total_amount)}
      </p>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
      <div
        class="scroll-region"
        tabindex="0"
        role="region"
        aria-label="Report entries"
      >
        <table>
          <thead
            ><tr
              ><th scope="col">Date</th><th scope="col">Account</th><th
                scope="col">Payee / notes</th
              ><th scope="col">Category</th><th scope="col">Amount</th></tr
            ></thead
          ><tbody
            >{#each inspection.rows as row (row.id)}<tr
                ><td>{row.date}</td><td>{row.account}</td><td
                  >{row.payee}<span class="muted"> {row.notes}</span></td
                ><td>{row.category}</td><td class="amount"
                  >{formatAmount(row.amount)}</td
                ></tr
              >{/each}</tbody
          >
        </table>
      </div>
      <div class="actions">
        <button
          disabled={busy || inspection.offset === 0}
          onclick={() => inspect(Math.max(0, inspection!.offset - 50))}
          >Previous entries</button
        ><button
          disabled={busy || !inspection.has_more}
          onclick={() => inspect(inspection!.offset + 50)}>Next entries</button
        >
      </div>{/if}
  </details>{/if}

<style>
  .legend {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
    padding-left: 1rem;
    font-size: 0.9rem;
  }
  .calendar-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(5rem, 1fr));
    min-width: 35rem;
  }
  .calendar-day {
    border: 1px solid var(--line);
    padding: 0.35rem;
    display: grid;
    gap: 0.25rem;
    font-variant-numeric: tabular-nums;
    font-size: 0.85rem;
  }
  svg {
    display: block;
    width: 100%;
    max-height: 16rem;
    color: var(--text);
  }
  details {
    margin-top: 1rem;
  }
  summary {
    cursor: pointer;
    padding: 0.65rem 0;
  }
  .scroll-region {
    overflow: auto;
    max-height: 26rem;
    max-width: 100%;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 0.5rem;
    text-align: left;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }
  .amount {
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .summary-value {
    font-size: 1.75rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  label {
    display: grid;
    gap: 0.4rem;
    max-width: 100%;
  }
  select {
    max-width: 100%;
    width: 100%;
  }
</style>
