<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { errorMessage } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import {
    queryReport,
    customReportQuery,
    object,
    localDay,
    type ReportDocument,
    type ReportQuery,
    type Projection,
  } from "./reports";
  import CrossoverWidget from "./CrossoverWidget.svelte";
  import MarkdownNotes from "./MarkdownNotes.svelte";
  import ReportResult from "./ReportResult.svelte";
  let {
    budgetId,
    document,
    epoch,
    disconnected = false,
    onedit,
    onremove,
    report = null,
    onopenreport,
  }: {
    budgetId: string;
    document: ReportDocument;
    epoch: string;
    disconnected?: boolean;
    onedit: () => void;
    onremove: () => void;
    report?: ReportDocument | null;
    onopenreport: (id: string) => void;
  } = $props();
  let meta = $derived(object(document.config.meta)),
    type = $derived(String(document.config.type)),
    name = $derived(
      String(
        meta.name ||
          (
            {
              "cash-flow-card": t("Cash flow"),
              "net-worth-card": t("Net worth"),
              "spending-card": t("Spending analysis"),
              "summary-card": t("Summary"),
              "calendar-card": t("Transaction calendar"),

              "crossover-card": t("Crossover Point"),

              "markdown-card": t("Text"),
              "custom-report": String(
                report?.config.name || t("Saved custom report"),
              ),
            } as Record<string, string>
          )[type] ||
          type,
      ),
    );
  let query = $state.raw<ReportQuery | null>(null),
    projection = $state.raw<Projection | null>(null),
    error = $state(""),
    busy = $state(false),
    cached = $state(false);
  let mounted = true;
  const kinds: Record<string, string> = {
    "cash-flow-card": "cash-flow",
    "net-worth-card": "net-worth",
    "spending-card": "spending",
    "summary-card": "summary",
    "calendar-card": "calendar",
    "custom-report": "custom",
  };
  const cache = () => `view:report-widget:${budgetId}:${document.id}`;
  async function load() {
    if (!kinds[type]) return;
    busy = true;
    error = "";
    cached = false;
    try {
      if (type === "custom-report") {
        if (!report)
          throw Error(
            "This saved report is unavailable. Its widget reference is preserved; choose another saved report or remove the widget.",
          );
        const q = await customReportQuery(
            budgetId,
            report.config,
            localDay(),
            preferences.shared.first_day_of_week,
          ),
          p = await queryReport(budgetId, q);
        await writeValue(cache(), {
          query: q,
          projection: p,
          revision: document.revision,
          reportRevision: report.revision,
        });
        if (mounted) {
          query = q;
          projection = p;
        }
        return;
      }
      const as_of = localDay(),
        dates = await queryReport(budgetId, {
          version: 1,
          kind: "widget-range",
          as_of,
          options: { timeFrame: meta.timeFrame },
        }),
        range = dates.result as unknown as string[];
      let summary: unknown = { type: "sum" };
      if (type === "summary-card" && meta.content) {
        summary =
          typeof meta.content === "string"
            ? JSON.parse(meta.content)
            : meta.content;
      }
      const q: ReportQuery = {
        version: 1,
        kind: kinds[type],
        as_of,
        options: {
          ...meta,
          start: range[0],
          end: range[1],
          summary,
          first_day_of_week: String(preferences.shared.first_day_of_week),
          isConcise: meta.isConcise ?? false,
          budgetType: meta.budgetType,
        },
      };
      if (q.kind === "spending") {
        q.options.compare = meta.compare || as_of.slice(0, 7);
        q.options.compareTo =
          ["budget", "average"].includes(String(meta.mode)) &&
          meta.isLive !== false
            ? q.options.compare
            : meta.compareTo;
      }
      const p = await queryReport(budgetId, q);
      await writeValue(cache(), {
        query: q,
        projection: p,
        revision: document.revision,
      });
      if (mounted) {
        query = q;
        projection = p;
      }
    } catch (e) {
      const prior = await readValue<{
        query: ReportQuery;
        projection: Projection;
        revision: number;
        reportRevision?: number;
      }>(cache()).catch(() => undefined);
      if (mounted) {
        error = errorMessage(e);
        if (
          prior?.revision === document.revision &&
          (type !== "custom-report" ||
            (report && prior.reportRevision === report.revision))
        ) {
          query = prior.query;
          projection = prior.projection;
          cached = true;
        }
      }
    } finally {
      if (mounted) busy = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      mounted = false;
    };
  });
</script>

<article data-testid="report-widget" aria-label={name}>
  <header>
    <h3>
      {name}
    </h3>
    <div class="actions">
      <button onclick={onedit} disabled={disconnected}
        >{t("Edit")} <span class="sr-only">{name}</span></button
      ><button onclick={onremove} disabled={disconnected}
        >{t("Remove")} <span class="sr-only">{name}</span></button
      >
    </div>
  </header>
  {#if type === "custom-report" && report}<button
      onclick={() => onopenreport(report!.id)}
      disabled={disconnected}>{t("Open saved report")}</button
    >{/if}
  {#if type === "markdown-card"}<div
      style:text-align={["left", "center", "right"].includes(
        String(meta.text_align),
      )
        ? String(meta.text_align)
        : "left"}
    >
      <MarkdownNotes text={String(meta.content || "")} />
    </div>
  {:else if type === "crossover-card"}<CrossoverWidget
      {budgetId}
      {document}
      {epoch}
      {disconnected}
    />
  {:else if !kinds[type]}<p>
      {t(
        "This widget’s settings are preserved. Its report type is not available in this version.",
      )}
    </p>
  {:else}<p class="muted">
      {projection?.basis === "budget_allocations"
        ? t("Planned budget allocations")
        : t("Recorded entries")} · {#if cached}{t("Cached view ·")}
      {/if}{query?.as_of || localDay()}
    </p>
    {#if busy}<p role="status">{t("Loading report…")}</p>{/if}{#if error}<p
        role="alert"
      >
        {t(error)}
      </p>
      <button onclick={load} disabled={busy || disconnected}
        >{t("Refresh report")}</button
      >{/if}{#if query && projection}{#key projection.snapshot + JSON.stringify(query)}<ReportResult
          {budgetId}
          {query}
          {projection}
        />{/key}{/if}{/if}
</article>

<style>
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  article {
    min-width: 0;
    height: 100%;
    padding: 1rem;
    border: 1px solid var(--line);
    background: var(--surface);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  h3 {
    margin: 0;
    font-size: 1.15rem;
  }
  .actions {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .actions button {
    padding: 0.35rem 0.65rem;
  }
  .muted {
    font-size: 0.9rem;
  }
  button {
    min-height: 44px;
  }
</style>
