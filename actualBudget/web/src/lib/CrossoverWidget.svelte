<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { displayDate, dateObject, isoDate } from "./dates";
  import { errorMessage, RequestError } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import type { ReportDocument } from "./reports";
  import {
    crossoverQuery,
    readCrossover,
    type CrossoverProjection,
    type CrossoverQuery,
    validCrossover,
  } from "./crossover";
  import CrossoverResult from "./CrossoverResult.svelte";
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
  let projection = $state.raw<CrossoverProjection | null>(null),
    query = $state.raw<CrossoverQuery | null>(null),
    busy = $state(false),
    error = $state(""),
    cached = $state(false),
    cacheSaveFailed = $state(false);
  let alive = true,
    generation = 0;
  const key = () => `view:crossover:${budgetId}:${document.id}`;
  let endDate = $derived.by(() => {
    if (!query) return "";
    const end = dateObject(query.options.end + "-01");
    end.setUTCMonth(end.getUTCMonth() + 1, 0);
    return isoDate(end);
  });
  let rejectedEpoch = "";
  function valid(value: CrossoverProjection) {
    return validCrossover(value, epoch);
  }
  async function load() {
    const current = ++generation;
    busy = true;
    error = "";
    cached = false;
    cacheSaveFailed = false;
    try {
      if (disconnected)
        throw Error("Reconnect to calculate the current crossover projection.");
      const request = await crossoverQuery(budgetId, document),
        value = await readCrossover(budgetId, request);
      if (!alive || current !== generation) return;
      if (!valid(value)) {
        rejectedEpoch = epoch;
        throw Error(
          "The budget version changed or the projection is unreadable. Refresh dashboards before calculating again.",
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
      error = errorMessage(e);
      projection = null;
      query = null;
      if (
        rejectedEpoch !== epoch &&
        !(e instanceof RequestError && !e.uncertain)
      ) {
        const prior = await readValue<{
          query: CrossoverQuery;
          projection: CrossoverProjection;
          revision: number;
          epoch: string;
        }>(key()).catch(() => null);
        if (
          alive &&
          current === generation &&
          prior?.epoch === epoch &&
          prior.revision === document.revision &&
          valid(prior.projection) &&
          prior.query?.version === 1 &&
          typeof prior.query.options?.start === "string" &&
          typeof prior.query.options.end === "string"
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
    epoch;
    document.revision;
    untrack(() => void load());
  });
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

<section aria-label={t("Crossover result")}>
  <p>
    {t(
      "Projection · financial independence based on recorded history and selected assumptions",
    )}
  </p>
  {#if busy}<p role="status">{t("Calculating crossover…")}</p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if cacheSaveFailed && projection}<p role="status">
      {t(
        "Projection calculated, but this browser could not save a cached copy. Current results remain available.",
      )}
    </p>{/if}
  <button onclick={load} disabled={busy || disconnected}
    >{t("Recalculate crossover")}</button
  >
  {#if projection && query}
    {#if cached}<p role="status">
        {t(
          "Cached projection. Inputs may have changed; reconnect and recalculate before relying on it.",
        )}
      </p>{/if}
    <p>
      {t("As of")}
      {displayDate(projection.as_of, preferences.shared.date_format)} · {displayDate(
        query.options.start + "-01",
        preferences.shared.date_format,
      )}–{displayDate(endDate, preferences.shared.date_format)}
    </p>
    {#key projection.snapshot + JSON.stringify(query)}<CrossoverResult
        {budgetId}
        {projection}
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
