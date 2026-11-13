<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { displayDate } from "./dates";
  import { errorMessage, RequestError } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import { object, type ReportDocument } from "./reports";
  import {
    sankeyQuery,
    readSankey,
    type SankeyProjection,
    type SankeyQuery,
    validSankey,
    validSankeyQuery,
    heightNodeCap,
  } from "./sankey";
  import SankeyResult from "./SankeyResult.svelte";
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
  let projection = $state.raw<SankeyProjection | null>(null),
    query = $state.raw<SankeyQuery | null>(null),
    busy = $state(false),
    error = $state(""),
    cached = $state(false),
    cacheSaveFailed = $state(false);
  let alive = true,
    generation = 0;
  let enabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.shared.experimental.sankeyReport,
  );
  const key = () => `view:sankey:${budgetId}:${document.id}`;
  let plotHeight = $derived(
    Math.max(280, Math.min(4480, (Number(document.config.height) || 4) * 70)),
  );
  let heightCap = $derived(heightNodeCap(plotHeight));
  let rejectedEpoch = "";
  function valid(value: SankeyProjection) {
    return validSankey(value, epoch);
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
          "Enable Sankey report in Settings → Experimental features. Saved settings are preserved.",
        );
      if (disconnected)
        throw Error("Reconnect to calculate current money flow.");
      const request = await sankeyQuery(budgetId, document, heightCap),
        value = await readSankey(budgetId, request);
      if (!alive || current !== generation) return;
      if (!valid(value)) {
        rejectedEpoch = epoch;
        throw Error(
          "The budget version changed or the flow response is unreadable. Refresh dashboards before calculating again.",
        );
      }
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
      const message = errorMessage(e);
      error = message.includes("charged units")
        ? "This flow needs too much calculation. Select fewer categories or accounts, or increase the visible-node cap."
        : message;
      projection = null;
      query = null;
      if (
        enabled &&
        rejectedEpoch !== epoch &&
        !(e instanceof RequestError && !e.uncertain)
      ) {
        const prior = await readValue<{
          query: SankeyQuery;
          projection: SankeyProjection;
          revision: number;
          epoch: string;
        }>(key()).catch(() => null);
        if (
          alive &&
          current === generation &&
          prior?.epoch === epoch &&
          prior.revision === document.revision &&
          valid(prior.projection) &&
          validSankeyQuery(prior.query)
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
    heightCap;
    untrack(() => void load());
  });
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

<section aria-label={t("Money flow result")}>
  <p>
    {t(
      "Money flow · grouped recorded entries or budget allocations and context",
    )}
  </p>
  {#if busy}<p role="status">{t("Calculating money flow…")}</p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if cacheSaveFailed && projection && enabled}<p role="status">
      {t(
        "Flow calculated, but this browser could not save a cached copy. Current results remain available.",
      )}
    </p>{/if}
  <button onclick={load} disabled={busy || disconnected || !enabled}
    >{t("Recalculate flow")}</button
  >
  {#if projection && query && enabled}
    {#if cached}<p role="status">
        {t(
          "Cached flow. Inputs may have changed; reconnect and recalculate before relying on it. Details are unavailable until recalculated.",
        )}
      </p>{/if}
    <p>
      {t("Tracking budget")} · {t(
        projection.mode === "spent" ? "Spent" : "Budgeted",
      )}
    </p>
    <p>
      {t("Saved maximum nodes per layer:")}
      {String(object(document.config.meta).topNcategories ?? 15)} · {t(
        "Height limit:",
      )}
      {heightCap}. {t("Totals and reversing flows remain separate.")}
    </p>
    <p>
      {t("As of")}
      {displayDate(projection.as_of, preferences.shared.date_format)} · {displayDate(
        query.options.startDate,
        preferences.shared.date_format,
      )}–{displayDate(query.options.endDate, preferences.shared.date_format)}
    </p>
    {#key projection.snapshot + JSON.stringify(query)}<SankeyResult
        {budgetId}
        {query}
        {projection}
        {cached}
        disconnected={disconnected || busy}
        showPercentages={object(document.config.meta).showPercentages === true}
        {plotHeight}
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
