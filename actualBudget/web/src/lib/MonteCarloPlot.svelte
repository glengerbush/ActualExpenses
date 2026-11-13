<script lang="ts">
  import { t } from "./i18n.svelte";
  import { formatAmount } from "./money";
  import type { PlotSeries } from "./monte-carlo";
  let {
    title,
    series,
    ages,
    awaitingPreferences,
    stacked = false,
  }: {
    title: string;
    series: PlotSeries[];
    ages: number[];
    awaitingPreferences: boolean;
    stacked?: boolean;
  } = $props();
  let extents = $derived(
    stacked
      ? ages.flatMap((_, i) => [
          series.reduce((sum, s) => sum + Math.max(0, s.values[i] ?? 0), 0),
          series.reduce((sum, s) => sum + Math.min(0, s.values[i] ?? 0), 0),
        ])
      : series.flatMap((s) => s.values),
  );
  let min = $derived(Math.min(0, ...extents)),
    max = $derived(Math.max(1, ...extents));
  const y = (value: number) => 220 - (200 * (value - min)) / (max - min);
  const x = (index: number) =>
    16 +
    568 *
      (stacked
        ? (index + 0.5) / Math.max(1, ages.length)
        : index / Math.max(1, ages.length - 1));
  let bars = $derived.by(() => {
    if (!stacked) return [];
    return ages.flatMap((age, index) => {
      let positive = 0,
        negative = 0;
      return series.map((s) => {
        const value = s.values[index] ?? 0,
          start = value >= 0 ? positive : negative;
        if (value >= 0) positive += value;
        else negative += value;
        return {
          key: `${s.key}:${age}`,
          label: s.label,
          value,
          age,
          color: s.color,
          x: x(index),
          y: Math.min(y(start), y(start + value)),
          height: Math.abs(y(start) - y(start + value)),
        };
      });
    });
  });
</script>

<figure aria-label={t(title)}>
  <figcaption>{t(title)}</figcaption>
  {#if awaitingPreferences}<p>{t("Loading display preferences…")}</p>
  {:else}
    <p class="scale">
      {t("Amount range:")}
      {formatAmount(Math.round(min))} – {formatAmount(Math.round(max))}
    </p>
    <svg role="img" aria-label={t(title)} viewBox="0 0 600 240">
      <line
        x1="16"
        x2="584"
        y1={y(0)}
        y2={y(0)}
        stroke="currentColor"
        opacity="0.4"
      />
      {#if stacked}
        {#each bars as bar (bar.key)}
          {#if bar.height > 0}<rect
              x={bar.x - 220 / Math.max(1, ages.length)}
              y={bar.y}
              width={440 / Math.max(1, ages.length)}
              height={bar.height}
              fill={bar.color}
            >
              <title
                >{t("Age")}
                {bar.age} · {t(bar.label)}: {formatAmount(
                  Math.round(bar.value),
                )}</title
              >
            </rect>{/if}
        {/each}
      {:else}
        {#each series as item (item.key)}<polyline
            points={item.values
              .map((value, i) => `${x(i)},${y(value)}`)
              .join(" ")}
            fill="none"
            stroke={item.color}
            stroke-width="2"
            stroke-dasharray={item.dash}
            vector-effect="non-scaling-stroke"
            ><title>{t(item.label)}</title></polyline
          >{/each}
      {/if}
    </svg>
    <div class="ages">
      <span>{t("Age")} {ages[0]}</span><span>{t("Age")} {ages.at(-1)}</span>
    </div>
    <ul aria-label={t("Chart legend")}>
      {#each series as item (item.key)}<li>
          <svg aria-hidden="true" viewBox="0 0 30 12"
            ><line
              x1="0"
              x2="30"
              y1="6"
              y2="6"
              stroke={item.color}
              stroke-width={stacked ? 8 : 2}
              stroke-dasharray={item.dash}
            /></svg
          >{t(item.label)}
        </li>{/each}
    </ul>
    <p class="muted">{t("Exact values are available in the table below.")}</p>
  {/if}
</figure>

<style>
  figure {
    margin: 1.5rem 0;
    min-width: 0;
  }
  figcaption {
    font-weight: 600;
  }
  figure > svg {
    display: block;
    width: 100%;
    max-height: 320px;
  }
  .scale,
  .ages {
    font-variant-numeric: tabular-nums;
  }
  .ages {
    display: flex;
    justify-content: space-between;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    list-style: none;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  li svg {
    width: 30px;
    height: 12px;
  }
  p {
    max-width: 70ch;
  }
</style>
