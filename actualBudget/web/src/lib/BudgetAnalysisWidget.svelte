<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { errorMessage, RequestError } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import { object, type ReportDocument } from "./reports";
  import {
    analysisQuery,
    readAnalysis,
    type AnalysisProjection,
    type AnalysisQuery,
  } from "./budget-analysis";
  import BudgetAnalysisResult from "./BudgetAnalysisResult.svelte";
  let {
    budgetId,
    document,
    epoch,
    disconnected = false,
  }: {
    budgetId: string;
    document: ReportDocument;
    epoch: string;
    disconnected?: boolean;
  } = $props();
  let projection = $state.raw<AnalysisProjection | null>(null),
    query = $state.raw<AnalysisQuery | null>(null),
    busy = $state(false),
    error = $state(""),
    cached = $state(false),
    cacheSaveFailed = $state(false);
  let alive = true,
    generation = 0;
  let enabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.shared.experimental.budgetAnalysisReport,
  );
  const key = () => `view:budget-analysis:${budgetId}:${document.id}`;
  function valid(value: AnalysisProjection) {
    return (
      value?.epoch === epoch &&
      typeof value.snapshot === "string" &&
      Array.isArray(value.result?.intervalData) &&
      value.result.intervalData.every(
        (row) =>
          typeof row.date === "string" &&
          [
            row.budgeted,
            row.spent,
            row.balance,
            row.overspendingAdjustment,
          ].every(Number.isSafeInteger),
      ) &&
      [
        value.result.totalBudgeted,
        value.result.totalSpent,
        value.result.totalOverspendingAdjustment,
        value.result.finalOverspendingAdjustment,
      ].every(Number.isSafeInteger) &&
      Array.isArray(value.assumptions)
    );
  }
  async function load() {
    const current = ++generation;
    busy = true;
    error = "";
    cached = false;
    cacheSaveFailed = false;
    try {
      if (!enabled)
        throw Error(
          "Enable Budget Analysis in Settings → Experimental features. Saved settings are preserved.",
        );
      if (disconnected)
        throw Error("Reconnect to calculate current budget analysis.");
      const request = await analysisQuery(budgetId, document),
        value = await readAnalysis(budgetId, request);
      if (!alive || current !== generation) return;
      if (!valid(value))
        throw Error(
          "The budget version changed. Refresh dashboards before calculating again.",
        );
      query = request;
      projection = value;
      try {
        await writeValue(key(), {
          query: request,
          projection: value,
          revision: document.revision,
          epoch,
        });
      } catch {
        if (alive && current === generation) cacheSaveFailed = true;
      }
    } catch (e) {
      if (!alive || current !== generation) return;
      error = errorMessage(e);
      projection = null;
      query = null;
      if (enabled && !(e instanceof RequestError && !e.uncertain)) {
        const prior = await readValue<{
          query: AnalysisQuery;
          projection: AnalysisProjection;
          revision: number;
          epoch: string;
        }>(key()).catch(() => null);
        if (
          alive &&
          current === generation &&
          prior?.epoch === epoch &&
          prior.revision === document.revision &&
          valid(prior.projection) &&
          prior.query?.version === 1
        ) {
          projection = prior.projection;
          query = prior.query;
          cached = true;
        }
      }
    } finally {
      if (alive && current === generation) busy = false;
    }
  }
  // Prop changes initiate external reads. Generation guards reject stale responses.
  $effect(() => {
    disconnected;
    enabled;
    epoch;
    document.revision;
    untrack(() => void load());
  });
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

<section aria-label={t("Budget analysis result")}>
  <p>
    {t(
      "Planned allocations and recorded spending · monthly expense categories",
    )}
  </p>
  {#if busy}<p role="status">{t("Calculating budget analysis…")}</p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if cacheSaveFailed && projection && enabled}<p role="status">
      {t(
        "Analysis calculated, but this browser could not save a cached copy. Current results remain available.",
      )}
    </p>{/if}
  <button onclick={load} disabled={busy || disconnected || !enabled}
    >{t("Recalculate analysis")}</button
  >
  {#if projection && query && enabled}
    {#if cached}<p role="status">
        {t(
          "Cached analysis. Inputs may have changed; reconnect and recalculate before relying on it. Details are unavailable until recalculated.",
        )}
      </p>{/if}
    <p>
      {t("As of")}
      {projection.as_of} · {projection.result.startDate}–{projection.result
        .endDate}
    </p>
    {#key projection.snapshot + JSON.stringify(query)}<BudgetAnalysisResult
        {budgetId}
        {query}
        {projection}
        meta={object(document.config.meta)}
        {cached}
        {disconnected}
      />{/key}
  {/if}
</section>

<style>
  section {
    min-width: 0;
  }
  p {
    max-width: 70ch;
  }
  button {
    min-height: 44px;
    max-width: 100%;
  }
</style>
