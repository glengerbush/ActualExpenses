<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { t } from "./i18n.svelte";
  import { uuid, errorMessage } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import type { ReportDocument } from "./reports";
  import {
    calculationInputs,
    runMonteCarlo,
    cancelMonteCarlo,
    validRunDetails,
    type MonteCarloProjection,
    type RunDetail,
  } from "./monte-carlo";
  import MonteCarloResult from "./MonteCarloResult.svelte";
  import MonteCarloRuns from "./MonteCarloRuns.svelte";
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
  interface SavedResult {
    projection: MonteCarloProjection;
    revision: number;
    seed: number;
    deflate: boolean;
    calculatedAt: string;
    detailIndex?: number;
  }
  let saved = $state.raw<SavedResult | null>(null),
    busy = $state(false),
    cancelling = $state(false),
    error = $state(""),
    notice = $state(""),
    seed = $state(1234),
    deflate = $state(false),
    cached = $state(false);
  const details = new SvelteMap<number, RunDetail[]>();
  let availableRuns = $derived([...details.keys()]);
  let enabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.shared.experimental.monteCarloReport,
  );
  let alive = true,
    active = "",
    controller: AbortController | null = null;
  const key = () => `view:monte-carlo:${budgetId}:${document.id}`;
  function valid(value: SavedResult | null | undefined): value is SavedResult {
    try {
      const projection = value?.projection,
        result = projection?.result;
      return (
        !!value &&
        value.revision === document.revision &&
        projection?.epoch === epoch &&
        !!result &&
        Number.isFinite(projection.config.currentAge) &&
        [
          projection.config.pots,
          projection.config.incomeStreams,
          projection.config.contributions,
        ].every(Array.isArray) &&
        Number.isSafeInteger(value.seed) &&
        typeof value.deflate === "boolean" &&
        Number.isFinite(Date.parse(value.calculatedAt)) &&
        [
          result.medianEndingBalance,
          result.medianTotalWithdrawn,
          result.successRate,
          result.simulationCount,
          result.horizonYears,
        ].every(Number.isFinite) &&
        [
          result.medianDepletionYear,
          result.earliestDepletionYear,
          result.latestDepletionYear,
        ].every((n) => n === null || Number.isFinite(n)) &&
        Array.isArray(result.percentileBands) &&
        result.percentileBands.length <= 101 &&
        result.percentileBands.every((row) =>
          [
            row.year,
            row.p5,
            row.p10,
            row.p25,
            row.p30,
            row.p50,
            row.p70,
            row.p75,
            row.p90,
          ].every(Number.isFinite),
        ) &&
        Array.isArray(result.depletionHistogram) &&
        result.depletionHistogram.length <= 100 &&
        result.depletionHistogram.every(
          (row) => Number.isFinite(row.year) && Number.isFinite(row.count),
        ) &&
        Array.isArray(result.depletionProbabilityByYear) &&
        result.depletionProbabilityByYear.every(Number.isFinite) &&
        [
          result.endingBalances,
          result.depletionYearBySimulation,
          result.totalWithdrawnBySimulation,
        ].every(
          (values) =>
            Array.isArray(values) &&
            values.length === result.simulationCount &&
            values.length <= 10000 &&
            values.every(Number.isFinite),
        ) &&
        Array.isArray(result.worstRunPath) &&
        result.worstRunPath.every(Number.isFinite) &&
        (value.detailIndex === undefined ||
          (Number.isInteger(value.detailIndex) &&
            value.detailIndex >= 0 &&
            value.detailIndex < result.simulationCount)) &&
        (result.runDetail === undefined ||
          validRunDetails(
            result.runDetail,
            projection.config,
            result.horizonYears,
          )) &&
        Array.isArray(projection.assumptions) &&
        projection.assumptions.every((s) => typeof s === "string") &&
        Array.isArray(projection.warnings) &&
        projection.warnings.every((w) => typeof w.message === "string")
      );
    } catch {
      return false;
    }
  }
  function remember(index: number, rows: RunDetail[]) {
    details.delete(index);
    details.set(index, rows);
    while (details.size > 4) details.delete(details.keys().next().value!);
  }
  async function persist(next: SavedResult) {
    try {
      await writeValue(key(), next);
    } catch {
      if (alive)
        notice =
          "Result is available, but could not be saved for offline viewing.";
    }
  }
  async function inspect(index: number) {
    const original = saved;
    if (
      !original ||
      busy ||
      !enabled ||
      !Number.isInteger(index) ||
      index < 0 ||
      index >= original.projection.result.simulationCount
    )
      return;
    let rows = details.get(index);
    if (!rows && disconnected) return;
    error = "";
    notice = "";
    if (!rows) {
      busy = true;
      cancelling = false;
      const id = uuid(),
        abort = new AbortController();
      active = id;
      controller = abort;
      try {
        // Resolve a saved scenario against its captured balances, never today's
        // linked account values. Only an explicit Run refreshes those balances.
        const raw = calculationInputs(
          (document.config.meta ?? null) as Record<string, unknown> | null,
        );
        const rawPots =
          Array.isArray(raw?.pots) && raw.pots.length
            ? raw.pots
            : original.projection.config.pots;
        const frozen = {
          ...raw,
          pots: original.projection.config.pots.map((pot, index) => ({
            ...rawPots[index],
            accountId: null,
            startingBalance: pot.startingBalance,
          })),
        };
        const projection = await runMonteCarlo(
          budgetId,
          id,
          frozen,
          original.seed,
          original.deflate,
          abort.signal,
          index,
        );
        if (!alive || cancelling || active !== id) return;
        if (projection.epoch !== epoch || !projection.result.runDetail)
          throw Error(
            "The saved run is unavailable. Reload the budget and run the plan again.",
          );
        rows = projection.result.runDetail;
      } catch (cause) {
        if (alive && active === id && !cancelling) error = errorMessage(cause);
        return;
      } finally {
        if (active === id) {
          active = "";
          controller = null;
          if (alive) {
            busy = false;
            cancelling = false;
          }
        }
      }
    }
    if (!alive || !rows) return;
    const next = {
      ...original,
      detailIndex: index,
      projection: {
        ...original.projection,
        result: { ...original.projection.result, runDetail: rows },
      },
    };
    if (!valid(next)) {
      error =
        "The saved run is unavailable. Reload the budget and run the plan again.";
      return;
    }
    remember(index, rows);
    saved = next;
    await persist(next);
  }
  async function run(event: SubmitEvent) {
    event.preventDefault();
    if (busy || disconnected || !enabled) return;
    busy = true;
    cancelling = false;
    error = "";
    notice = "";
    const id = uuid(),
      abort = new AbortController(),
      selectedSeed = seed,
      selectedDeflate = deflate;
    active = id;
    controller = abort;
    try {
      const projection = await runMonteCarlo(
        budgetId,
        id,
        calculationInputs(
          (document.config.meta ?? null) as Record<string, unknown> | null,
        ),
        selectedSeed,
        selectedDeflate,
        abort.signal,
      );
      if (!alive || cancelling || active !== id) return;
      const next = {
        projection,
        revision: document.revision,
        seed: selectedSeed,
        deflate: selectedDeflate,
        calculatedAt: new Date().toISOString(),
        detailIndex: 0,
      };
      if (!valid(next))
        throw Error(
          "The budget changed during calculation. Reload the budget and run the plan again.",
        );
      saved = next;
      cached = false;
      details.clear();
      if (projection.result.runDetail) remember(0, projection.result.runDetail);
      await persist(next);
    } catch (cause) {
      if (alive && active === id && !cancelling) error = errorMessage(cause);
    } finally {
      if (active === id) {
        active = "";
        controller = null;
        if (alive) {
          busy = false;
          cancelling = false;
        }
      }
    }
  }
  async function cancel() {
    if (!active) return;
    const id = active,
      abort = controller;
    cancelling = true;
    notice = "Cancellation requested.";
    try {
      const confirmed = await cancelMonteCarlo(budgetId, id);
      if (alive && active === id)
        notice = confirmed
          ? "Simulation cancelled."
          : "Simulation is no longer running.";
    } catch (cause) {
      if (alive && active === id) error = errorMessage(cause);
    } finally {
      abort?.abort();
    }
  }
  onMount(() => {
    void readValue<SavedResult>(key())
      .then((value) => {
        if (alive && !busy && !saved && valid(value)) {
          saved = value;
          cached = true;
          seed = value.seed;
          deflate = value.deflate;
          if (value.projection.result.runDetail)
            remember(value.detailIndex ?? 0, value.projection.result.runDetail);
        }
      })
      .catch(() => {
        if (alive)
          notice =
            "Saved results are unavailable. Connect and run the plan again.";
      });
    return () => {
      alive = false;
      if (active) void cancelMonteCarlo(budgetId, active).catch(() => {});
      controller?.abort();
    };
  });
</script>

{#if !enabled}<p role="status">
    {t("Enable Monte Carlo report in Settings to run this plan.")}
  </p>
{:else}
  <p>{t("Experimental planning · no changes to recorded balances")}</p>
  <form aria-label={t("Run Monte Carlo plan")} onsubmit={run}>
    <label
      >{t("Random seed")}<input
        type="number"
        min="0"
        max="4294967295"
        step="1"
        required
        bind:value={seed}
        disabled={busy}
      /></label
    >
    <label class="check"
      ><input type="checkbox" bind:checked={deflate} disabled={busy} />{t(
        "Show results in today’s money",
      )}</label
    >
    <button type="submit" disabled={busy || disconnected}
      >{t("Run simulation")}</button
    >
    {#if busy}<button type="button" onclick={cancel} disabled={cancelling}
        >{t("Cancel simulation")}</button
      >{/if}
  </form>
  {#if busy}<p role="status">
      {t(
        cancelling
          ? "Cancelling simulation…"
          : "Running simulation… You can cancel while it calculates.",
      )}
    </p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if notice}<p role="status">{t(notice)}</p>{/if}
  {#if disconnected}<p role="status">
      {t(
        "Offline: saved results remain readable. Connect to run a simulation.",
      )}
    </p>{/if}
  {#if saved}
    <p class="muted">
      {t(cached || disconnected ? "Saved result" : "Calculated result")} · {new Date(
        saved.calculatedAt,
      ).toLocaleString()} · {t("Seed")}
      {saved.seed} · {t(saved.deflate ? "Today’s money" : "Nominal amounts")}
    </p>
    <p class="muted">
      {t(
        "Results reflect the saved inputs and linked balances at calculation time. Run again after changes.",
      )}
    </p>
    {#if seed !== saved.seed || deflate !== saved.deflate}<p role="status">
        {t("Run again to apply the changed calculation options.")}
      </p>{/if}
    {#each saved.projection.warnings as warning (warning)}<p role="status">
        {t(warning.message)}
      </p>{/each}
    <MonteCarloResult {budgetId} projection={saved.projection} />
    <MonteCarloRuns
      {budgetId}
      projection={saved.projection}
      selected={saved.detailIndex ?? 0}
      {availableRuns}
      {busy}
      {disconnected}
      oninspect={inspect}
    />
  {:else if !busy}<p>
      {t("No result yet. Review the plan assumptions, then run a simulation.")}
    </p>{/if}
{/if}

<style>
  form {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.75rem;
  }
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }
  input {
    min-height: 44px;
    max-width: 100%;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
    flex: none;
  }
  button {
    min-height: 44px;
  }
  p {
    max-width: 70ch;
  }
</style>
