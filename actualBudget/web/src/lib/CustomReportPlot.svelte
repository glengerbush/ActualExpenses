<script lang="ts">
  import { t } from "./i18n.svelte";
  import {
    array,
    customMetric,
    type ReportQuery,
    type Projection,
  } from "./reports";
  let { query, projection }: { query: ReportQuery; projection: Projection } =
    $props();
  const colors = [
    "var(--accent)",
    "var(--error)",
    "var(--text)",
    "var(--muted)",
  ];
  let format = $derived(String(query.options.graphType || "BarGraph")),
    metric = $derived(String(query.options.balanceTypeOp || "totalDebts"));
  let groups = $derived(array(projection.result.data)),
    periods = $derived(array(projection.result.intervalData));
  let time = $derived(
    query.options.mode === "time" && query.options.groupBy !== "Interval",
  );
  let rows = $derived(
    time
      ? periods.map((period) => ({
          label: String(period.date),
          values: groups.map((group) =>
            customMetric(
              array(group.intervalData).find(
                (p) => p.intervalStartDate === period.intervalStartDate,
              ),
              metric,
            ),
          ),
        }))
      : (query.options.groupBy === "Interval" ? periods : groups).map(
          (group) => ({
            label: String(group.name || group.date),
            values: [customMetric(group, metric)],
          }),
        ),
  );
  let labels = $derived(
    time ? groups.map((group) => String(group.name)) : ["Amount"],
  );
  let geometry = $derived.by(() => {
    const values = rows.flatMap((row) => row.values),
      min = Math.min(0, ...values),
      max = Math.max(1, ...values),
      y = (v: number) => 180 - ((v - min) / (max - min)) * 160,
      x = (i: number) => 20 + (i / Math.max(1, rows.length - 1)) * 560;
    const series = labels.map((label, column) => {
      const points = rows.map((row, i) => ({
        x: x(i),
        y: y(row.values[column] || 0),
      }));
      const path = points
        .map((p, i) => `${i ? "L" : "M"}${p.x},${p.y}`)
        .join(" ");
      const meanX = (rows.length - 1) / 2,
        meanY =
          rows.reduce((s, r) => s + (r.values[column] || 0), 0) /
          Math.max(1, rows.length),
        den = rows.reduce((s, _, i) => s + (i - meanX) ** 2, 0),
        slope = den
          ? rows.reduce(
              (s, r, i) => s + (i - meanX) * ((r.values[column] || 0) - meanY),
              0,
            ) / den
          : 0;
      return {
        label,
        id: String(column),
        color: colors[column % colors.length],
        path,
        area: `${path}L${x(rows.length - 1)},${y(0)}L20,${y(0)}Z`,
        points,
        trend: `M20,${y(meanY - slope * meanX)}L580,${y(meanY + slope * meanX)}`,
      };
    });
    const band = 560 / Math.max(1, rows.length),
      bars = rows.flatMap((row, i) =>
        row.values.map((value, column) => ({
          id: `${i}:${column}`,
          x: 20 + i * band + (column * band) / labels.length,
          width: Math.max(0.1, (band / labels.length) * 0.8),
          y: Math.min(y(0), y(value)),
          height: Math.abs(y(value) - y(0)),
          color: colors[column % colors.length],
          label: row.label,
        })),
      );
    const slices = (
        query.options.groupBy === "Interval" ? periods : groups
      ).map((group) => ({
        label: String(group.name || group.date),
        value: Math.abs(customMetric(group, metric)),
      })),
      total = slices.reduce((sum, s) => sum + s.value, 0);
    let angle = -Math.PI / 2;
    const arcs = slices.map((slice, i) => {
      const start = angle,
        end = angle + (total ? slice.value / total : 0) * Math.PI * 2;
      angle = end;
      const span = end - start,
        point = (a: number, r: number) =>
          `${300 + Math.cos(a) * r},${100 + Math.sin(a) * r}`,
        path =
          span >= Math.PI * 2 - 0.000001
            ? "M300,20A80,80 0 1 1 299.999,20L299.999,55A45,45 0 1 0 300,55Z"
            : span > 0
              ? `M${point(start, 80)}A80,80 0 ${span > Math.PI ? 1 : 0} 1 ${point(end, 80)}L${point(end, 45)}A45,45 0 ${span > Math.PI ? 1 : 0} 0 ${point(start, 45)}Z`
              : "";
      return {
        ...slice,
        id: String(i),
        path,
        color: colors[i % colors.length],
      };
    });
    return { series, bars, arcs, zero: y(0), total };
  });
</script>

{#if !["Table", "TableGraph", "BarGraph", "LineGraph", "AreaGraph", "DonutGraph"].includes(format)}<p
  >
    {t("Saved format “")}{format}{t(
      "” is preserved. Exact values remain available below.",
    )}
  </p>
{:else if !["Table", "TableGraph"].includes(format) && rows.length}
  <svg
    viewBox="0 0 600 210"
    role="img"
    aria-label={`${format.replace("Graph", "")} report; exact values in table below`}
  >
    {#if format === "DonutGraph"}{#each geometry.arcs as arc (arc.id)}<path
          d={arc.path}
          fill={arc.color}><title>{arc.label}</title></path
        >{/each}
    {:else}<line
        x1="20"
        y1={geometry.zero}
        x2="580"
        y2={geometry.zero}
        stroke="currentColor"
        opacity=".4"
      />
      {#if format === "BarGraph"}{#each geometry.bars as bar (bar.id)}<rect
            x={bar.x}
            y={bar.y}
            width={bar.width}
            height={bar.height}
            fill={bar.color}><title>{bar.label}</title></rect
          >{/each}
      {:else}{#each geometry.series as series (series.id)}{#if format === "AreaGraph"}<path
              d={series.area}
              fill={series.color}
              opacity=".15"
            />{/if}<path
            d={series.path}
            fill="none"
            stroke={series.color}
            stroke-width="2"
          />{#if series.points.length === 1}<circle
              cx={series.points[0].x}
              cy={series.points[0].y}
              r="3"
              fill={series.color}
            />{/if}{#if format === "LineGraph" && query.options.showTrendLines}<path
              d={series.trend}
              stroke={series.color}
              fill="none"
              stroke-dasharray="5 4"
            />{/if}{/each}{/if}
      <text x="20" y="205">{rows[0]?.label}</text><text
        x="580"
        y="205"
        text-anchor="end">{rows.at(-1)?.label}</text
      >{/if}
  </svg>
  {#if format === "DonutGraph"}<p class="muted">
      {t("Shares use absolute amounts; signed values appear in the table.")}
      {geometry.total === 0 ? "No amounts in this range." : ""}
    </p>
    <ul class="legend">
      {#each geometry.arcs as arc (arc.id)}<li style:color={arc.color}>
          {arc.label}
        </li>{/each}
    </ul>
  {:else if time}<ul class="legend">
      {#each geometry.series as series (series.id)}<li
          style:color={series.color}
        >
          {series.label}
        </li>{/each}
    </ul>{/if}
  {#if format === "LineGraph" && query.options.showTrendLines}<p class="muted">
      {t("Dashed lines show linear trends through the displayed values.")}
    </p>{/if}
{:else if !rows.length}<p>{t("No values match this report.")}</p>{/if}

<style>
  svg {
    display: block;
    width: 100%;
    max-height: 22rem;
    color: var(--text);
  }
  text {
    fill: currentColor;
    font-size: 11px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem 1.5rem;
    padding-left: 1rem;
    font-size: 0.9rem;
  }
</style>
