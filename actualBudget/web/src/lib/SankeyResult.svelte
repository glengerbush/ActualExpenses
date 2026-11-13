<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import { errorMessage } from "./api";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import { displayDate } from "./dates";
  import {
    inspectSankey,
    type SankeyProjection,
    type SankeyQuery,
    type SankeyDetails,
    type SankeyNode,
  } from "./sankey";
  let {
    budgetId,
    query,
    projection,
    cached = false,
    disconnected = false,
    showPercentages = false,
    plotHeight = 420,
  }: {
    budgetId: string;
    query: SankeyQuery;
    projection: SankeyProjection;
    cached?: boolean;
    disconnected?: boolean;
    showPercentages?: boolean;
    plotHeight?: number;
  } = $props();
  let page = $state(0),
    details = $state.raw<SankeyDetails | null>(null),
    busy = $state(false),
    error = $state(""),
    selected = $state<{ source: string; target: string } | null>(null);
  let breakdownPages = $state<Record<string, number>>({});
  let alive = true,
    generation = 0;
  let awaitingPreferences = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== projection.epoch,
  );
  let nodeMeta = $derived(
    new Map(projection.node_metadata.map((node) => [node.key, node])),
  );
  function name(node: SankeyNode) {
    const meta = nodeMeta.get(node.key);
    if (meta && !meta.user_owned && meta.label_key) {
      let label = t(meta.label_key);
      for (const [key, value] of Object.entries(meta.label_params ?? {}))
        label = label.replaceAll("{{" + key + "}}", value);
      return label;
    }
    return node.name || t("Unnamed flow");
  }
  const money = (value: number) =>
    awaitingPreferences ? "••••" : formatAmount(value);
  let flows = $derived(
    projection.result.links.flatMap((link, index) => {
      const meta = projection.link_metadata[index],
        source = projection.result.nodes[link.source],
        target = projection.result.nodes[link.target];
      return meta?.kind === "flow" &&
        source &&
        target &&
        !nodeMeta.get(source.key)?.hidden &&
        !nodeMeta.get(target.key)?.hidden
        ? [
            {
              ...link,
              sourceNode: source,
              targetNode: target,
              id: meta.source + ":" + meta.target,
              selection: { source: meta.source, target: meta.target },
              members: meta.member_count,
            },
          ]
        : [];
    }),
  );
  let visible = $derived(flows.slice(page * 50, page * 50 + 50));
  let positive = $derived(flows.filter((link) => link.value > 0));
  let diagram = $derived.by(() => {
    const used = new Set(
      positive.flatMap((link) => [link.sourceNode.key, link.targetNode.key]),
    );
    const nodes = projection.result.nodes.filter((node) => used.has(node.key));
    if (nodes.length > 200 || positive.length > 400) return null;
    const groups = new Map<number, SankeyNode[]>();
    for (const node of nodes) {
      const depth = nodeMeta.get(node.key)?.depth ?? 0;
      groups.set(depth, [...(groups.get(depth) ?? []), node]);
    }
    const depths = [...groups.keys()].sort((a, b) => a - b),
      width = Math.max(800, depths.length * 180 + 100),
      weights = new Map<string, number>();
    for (const node of nodes) {
      const incoming = positive
          .filter((link) => link.targetNode.key === node.key)
          .reduce((sum, link) => sum + link.value, 0),
        outgoing = positive
          .filter((link) => link.sourceNode.key === node.key)
          .reduce((sum, link) => sum + link.value, 0);
      weights.set(node.key, Math.max(incoming, outgoing));
    }
    const maxCount = Math.max(
      1,
      ...[...groups.values()].map((group) => group.length),
    );
    if (maxCount > 100) return null;
    const height = Math.max(plotHeight, maxCount * 52 + 60),
      layerTotal = Math.max(
        1,
        ...[...groups.values()].map((group) =>
          group.reduce((sum, node) => sum + (weights.get(node.key) ?? 0), 0),
        ),
      ),
      scale = Math.max(1, height - 60 - maxCount * 42) / layerTotal,
      positions = new Map<string, { x: number; y: number; height: number }>();
    for (const [depth, group] of groups) {
      const sizes = group.map((node) =>
          Math.max(3, (weights.get(node.key) ?? 0) * scale),
        ),
        total =
          sizes.reduce((a, b) => a + b, 0) + 36 * Math.max(0, group.length - 1);
      let y = (height - total) / 2;
      group.forEach((node, index) => {
        positions.set(node.key, {
          x:
            40 +
            (depths.indexOf(depth) * (width - 260)) /
              Math.max(1, depths.length - 1),
          y,
          height: sizes[index],
        });
        y += sizes[index] + 36;
      });
    }
    const outgoing = new Map<string, number>(),
      incoming = new Map<string, number>();
    const paths = positive.map((link) => {
      const a = positions.get(link.sourceNode.key)!,
        b = positions.get(link.targetNode.key)!,
        thickness = Math.max(0.6, link.value * scale),
        from = a.y + (outgoing.get(link.sourceNode.key) ?? 0) + thickness / 2,
        to = b.y + (incoming.get(link.targetNode.key) ?? 0) + thickness / 2;
      outgoing.set(
        link.sourceNode.key,
        (outgoing.get(link.sourceNode.key) ?? 0) + thickness,
      );
      incoming.set(
        link.targetNode.key,
        (incoming.get(link.targetNode.key) ?? 0) + thickness,
      );
      const bend = a.x >= b.x ? Math.max(a.x, b.x) + 100 : (a.x + b.x) / 2;
      return {
        ...link,
        thickness,
        path: `M${a.x + 12},${from} C${bend},${from} ${bend},${to} ${b.x},${to}`,
      };
    });
    return { nodes, positions, paths, width, height };
  });
  const color = (value?: string) =>
    ({
      palette0: "var(--accent)",
      palette1: "var(--error)",
      palette2: "var(--muted)",
      positive: "var(--accent)",
      negative: "var(--error)",
      neutral: "var(--muted)",
      flow: "var(--text)",
    })[value ?? ""] ?? "var(--muted)";
  const short = (value: string) =>
    Array.from(value).length > 23
      ? Array.from(value).slice(0, 20).join("") + "…"
      : value;
  async function inspect(link: { source: string; target: string }, offset = 0) {
    const current = ++generation;
    selected = link;
    busy = true;
    error = "";
    details = null;
    try {
      const value = await inspectSankey(
        budgetId,
        query,
        projection.snapshot,
        link,
        offset,
      );
      if (alive && current === generation) details = value;
    } catch (e) {
      if (alive && current === generation)
        error =
          errorMessage(e) +
          " " +
          t("Recalculate the flow for current details.");
    } finally {
      if (alive && current === generation) busy = false;
    }
  }
  function label(key: string) {
    const node = projection.result.nodes.find((node) => node.key === key);
    return node ? name(node) : key;
  }
  function close() {
    selected = null;
    details = null;
    error = "";
    generation++;
    busy = false;
  }
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

{#if awaitingPreferences}<p>
    {t("Loading display preferences…")}
  </p>{:else if !diagram}<p>
    {t(
      "This diagram is too large to draw here. Narrow dates or filters; exact flows and record details remain available in the table.",
    )}
  </p>{:else if !positive.length}<p>
    {flows.length
      ? t(
          "Non-positive source flows are available in the table and are not drawn.",
        )
      : t(
          "No flows for this selection. Review dates, grouped categories and filters.",
        )}
  </p>{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (The financial diagram scrolls with keyboard access; exact table follows.) -->
  <div
    class="chart-scroll"
    role="region"
    tabindex="0"
    aria-label={t("Money flow diagram")}
  >
    <svg
      viewBox={`0 0 ${diagram.width} ${diagram.height}`}
      style:width={`${diagram.width}px`}
      style:height={`${diagram.height}px`}
      role="img"
      aria-label={t(
        "Money flow diagram. Exact values and keyboard record controls follow in the table.",
      )}
    >
      {#each diagram.paths as link (link.id)}<path
          d={link.path}
          fill="none"
          stroke={color(link.color)}
          stroke-width={link.thickness}
          stroke-opacity=".42"
          ><title
            >{name(link.sourceNode)} → {name(link.targetNode)}: {money(
              link.value,
            )}</title
          ></path
        >{/each}
      {#each diagram.nodes as node (node.key)}{@const position =
          diagram.positions.get(node.key)!}{@const meta = nodeMeta.get(
          node.key,
        )}<g
          ><rect
            x={position.x}
            y={position.y}
            width="12"
            height={position.height}
            fill={color(node.color)}
          /><text
            x={position.x + 18}
            y={position.y + Math.min(position.height / 2, 16)}
            fill="var(--text)"
            >{short(name(node))}<title>{name(node)}</title></text
          ><text
            class="value"
            x={position.x + 18}
            y={position.y + Math.min(position.height / 2, 16) + 16}
            fill="var(--muted)"
            >{showPercentages
              ? node.percentageLabel
              : money(meta?.amount ?? 0)}</text
          ></g
        >{/each}
    </svg>
  </div>
{/if}
<p class="muted">
  {t(
    "Each layer repeats flows; do not add all links into one total. Non-positive flows remain in the exact table. Node values and percentages use the complete context before layer selection.",
  )}
</p>
<p class="muted">
  {t("Scroll horizontally for all columns on smaller screens.")}
</p>
<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial table needs keyboard access.) -->
<div
  class="scroll"
  tabindex="0"
  role="region"
  aria-label={t("Money flow table")}
>
  <table aria-label={t("Money flows")}>
    <thead
      ><tr
        ><th>{t("From")}</th><th>{t("To")}</th><th>{t("Amount")}</th><th
          >{t("Details")}</th
        ></tr
      ></thead
    ><tbody
      >{#each visible as link (link.id)}<tr
          ><td>{name(link.sourceNode)}</td><td>{name(link.targetNode)}</td><td
            class="money">{money(link.value)}</td
          ><td
            ><button
              disabled={busy || cached || disconnected}
              onclick={() => inspect(link.selection)}
              aria-label={t("Inspect flow") +
                " " +
                name(link.sourceNode) +
                " → " +
                name(link.targetNode)}>{t("Inspect flow")}</button
            >{#if link.tooltipInfo.length}<details>
                <summary>{t("Other breakdown")}</summary>
                <ul>
                  {#each link.tooltipInfo.slice((breakdownPages[link.id] ?? 0) * 50, (breakdownPages[link.id] ?? 0) * 50 + 50) as item (item.name)}<li
                    >
                      {item.name}: {money(item.value)}
                    </li>{/each}
                </ul>
                {#if link.tooltipInfo.length > 50}<div class="pager">
                    <button
                      disabled={!(breakdownPages[link.id] ?? 0)}
                      onclick={() =>
                        (breakdownPages[link.id] =
                          (breakdownPages[link.id] ?? 0) - 1)}
                      >{t("Previous grouped labels")}</button
                    ><span
                      >{(breakdownPages[link.id] ?? 0) * 50 + 1}–{Math.min(
                        (breakdownPages[link.id] ?? 0) * 50 + 50,
                        link.tooltipInfo.length,
                      )} / {link.tooltipInfo.length}</span
                    ><button
                      disabled={(breakdownPages[link.id] ?? 0) * 50 + 50 >=
                        link.tooltipInfo.length}
                      onclick={() =>
                        (breakdownPages[link.id] =
                          (breakdownPages[link.id] ?? 0) + 1)}
                      >{t("Next grouped labels")}</button
                    >
                  </div>{/if}
                <p>
                  {t(
                    "These grouped labels follow the source report. Inspect the flow for all contributing records.",
                  )}
                </p>
              </details>{/if}</td
          ></tr
        >{/each}</tbody
    >
  </table>
</div>
<div class="pager">
  <button disabled={!page} onclick={() => page--}>{t("Previous flows")}</button
  ><span
    >{flows.length ? page * 50 + 1 : 0}–{Math.min(
      (page + 1) * 50,
      flows.length,
    )} / {flows.length}</span
  ><button disabled={(page + 1) * 50 >= flows.length} onclick={() => page++}
    >{t("Next flows")}</button
  >
</div>
{#if error}<p role="alert">{t(error)}</p>{/if}
{#if selected}<section aria-label={t("Flow records")}>
    <header>
      <h4>{label(selected.source)} → {label(selected.target)}</h4>
      <button onclick={close}>{t("Close details")}</button>
    </header>
    {#if busy}<p role="status">{t("Loading details…")}</p>{/if}{#if details}<p>
        {t("Signed contributions to this flow:")}
        {money(details.total_amount)} · {details.total_count}
        {t("records")}
      </p>
      <p>
        {t(
          "Refunds and opposing transfers can reduce the flow. Original recorded or planned amounts remain separate. Budget context rows are derived, not purchases. A linked transfer row represents both eligible legs; its original amount belongs to the listed account.",
        )}
      </p>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable contribution table needs keyboard access.) -->
      <div
        class="scroll"
        role="region"
        tabindex="0"
        aria-label={t("Flow contribution table")}
      >
        <table aria-label={t("Flow contributions")}>
          <thead
            ><tr
              ><th>{t("Date")}</th><th>{t("Basis")}</th><th
                >{t("Account / category")}</th
              ><th>{t("Payee")}</th><th>{t("Flow contribution")}</th><th
                >{t("Original amount")}</th
              ><th>{t("Notes")}</th></tr
            ></thead
          ><tbody
            >{#each details.rows as row (row.basis + ":" + row.id)}<tr
                ><td
                  >{row.date
                    ? displayDate(row.date, preferences.shared.date_format)
                    : row.month}</td
                ><td>{t(row.label_key)}</td><td
                  >{row.account_name ||
                    row.category_name ||
                    t(
                      "Budget context",
                    )}{#if row.account_name && row.category_name}<br
                    />{row.category_name}{/if}{#if row.peer_account_name}<br
                    />{t("Linked account:")}
                    {row.peer_account_name}{/if}{#if row.parent_id}<br />{t(
                      "Split entry",
                    )}{/if}</td
                ><td>{row.payee_name}</td><td class="money"
                  >{money(row.amount)}</td
                ><td class="money">{money(row.original_amount)}</td><td
                  >{row.notes}</td
                ></tr
              >{/each}</tbody
          >
        </table>
      </div>
      <div class="pager">
        <button
          disabled={!details.offset || busy || cached || disconnected}
          onclick={() =>
            inspect(selected!, Math.max(0, details!.offset - details!.limit))}
          >{t("Previous records")}</button
        ><span
          >{details.total_count ? details.offset + 1 : 0}–{details.offset +
            details.rows.length} / {details.total_count}</span
        ><button
          disabled={!details.has_more || busy || cached || disconnected}
          onclick={() => inspect(selected!, details!.offset + details!.limit)}
          >{t("Next records")}</button
        >
      </div>
    {/if}
  </section>{/if}
<details>
  <summary>{t("Assumptions and limits")}</summary
  >{#each projection.assumptions as assumption (assumption.code)}<p>
      {t(assumption.message)}
    </p>{/each}
</details>

<style>
  .chart-scroll,
  .scroll {
    overflow: auto;
    max-width: 100%;
    margin-top: 1rem;
  }
  .chart-scroll {
    border-bottom: 1px solid var(--line);
  }
  svg {
    display: block;
  }
  text {
    font-size: 13px;
  }
  .value {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    min-width: 42rem;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    text-align: left;
    padding: 0.65rem;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  .money {
    white-space: nowrap;
  }
  .pager,
  header {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
    align-items: center;
    margin: 0.8rem 0;
  }
  header {
    justify-content: space-between;
  }
  h4 {
    margin: 0;
  }
  p {
    max-width: 70ch;
  }
  .muted {
    color: var(--muted);
  }
  button,
  summary {
    min-height: 44px;
  }
  button {
    max-width: 100%;
  }
  summary {
    cursor: pointer;
    padding: 0.6rem 0;
  }
  section {
    border-top: 1px solid var(--line);
    margin-top: 1.4rem;
    padding-top: 0.6rem;
  }
  details {
    margin-top: 0.6rem;
  }
  ul {
    padding-left: 1.2rem;
  }
</style>
