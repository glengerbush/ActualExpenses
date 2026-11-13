<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { errorMessage, RequestError } from "./api";
  import { preferences } from "./preferences.svelte";
  import { formatAmount } from "./money";
  import { displayDate } from "./dates";
  import { readValue, writeValue } from "./offline/storage";
  import { localDay, type ReportDocument } from "./reports";
  import {
    forecastRequest,
    queryForecast,
    type ForecastProjection,
    type ForecastRequest,
  } from "./forecasts";
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
  let projection = $state.raw<ForecastProjection | null>(null),
    busy = $state(false),
    error = $state(""),
    cached = $state(false),
    cacheSaveFailed = $state(false),
    page = $state(0),
    detailPage = $state(0);
  let alive = true,
    generation = 0;
  let enabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.shared.experimental.balanceForecastReport,
  );
  let awaitingPreferences = $derived(
    preferences.budgetId !== budgetId ||
      !preferences.hasUsableShared ||
      preferences.budgetEpoch !== epoch,
  );
  let chart = $derived(projection?.chart ?? []),
    rows = $derived(chart.slice(page * 50, page * 50 + 50));
  let entries = $derived.by(() => {
    if (!projection) return [];
    const first = chart[0]?.date ?? "",
      last = chart.at(-1)?.date ?? "";
    return projection.result.dataPoints
      .filter(
        (p) =>
          p.date.slice(0, first.length) >= first &&
          p.date.slice(0, last.length) <= last,
      )
      .flatMap((p) =>
        p.transactions.map((entry, i) => ({
          ...entry,
          date: p.date,
          account: p.accountName,
          id: p.date + ":" + p.accountId + ":" + i,
        })),
      );
  });
  let details = $derived(entries.slice(detailPage * 50, detailPage * 50 + 50));
  let path = $derived.by(() => {
    const min = Math.min(...chart.map((p) => p.balance)),
      max = Math.max(...chart.map((p) => p.balance));
    return chart
      .map(
        (p, i) =>
          `${i ? "L" : "M"}${20 + (560 * i) / Math.max(1, chart.length - 1)},${180 - (160 * (p.balance - min)) / Math.max(1, max - min)}`,
      )
      .join(" ");
  });
  const money = (v: number) => (awaitingPreferences ? "••••" : formatAmount(v));
  const date = (v: string) => displayDate(v, preferences.shared.date_format);
  const key = () => `view:forecast:${budgetId}:${document.id}`;
  async function load() {
    const current = ++generation;
    busy = true;
    error = "";
    cached = false;
    cacheSaveFailed = false;
    page = 0;
    detailPage = 0;
    try {
      if (!enabled)
        throw new Error(
          "Enable Balance forecast in Settings → Experimental features. Saved settings are preserved.",
        );
      if (disconnected)
        throw new Error("Reconnect to calculate a current forecast.");
      const query = await forecastRequest(budgetId, document),
        value = await queryForecast(budgetId, query);
      if (!alive || current !== generation) return;
      if (value.epoch !== epoch)
        throw new Error(
          "The budget version changed. Refresh dashboards before calculating again.",
        );
      projection = value;
      try {
        await writeValue(key(), {
          query,
          projection: value,
          revision: document.revision,
          epoch,
        });
      } catch {
        // Downloaded views are optional; a cache failure cannot undo a valid read.
        if (alive && current === generation) cacheSaveFailed = true;
      }
    } catch (e) {
      if (!alive || current !== generation) return;
      error = errorMessage(e);
      projection = null;
      if (enabled && !(e instanceof RequestError && !e.uncertain)) {
        const prior = await readValue<{
          query: ForecastRequest;
          projection: ForecastProjection;
          revision: number;
          epoch: string;
        }>(key()).catch(() => null);
        if (
          alive &&
          current === generation &&
          prior?.epoch === epoch &&
          prior.revision === document.revision
        ) {
          projection = prior.projection;
          cached = true;
        }
      }
    } finally {
      if (alive && current === generation) busy = false;
    }
  }
  // Prop changes start asynchronous reads; the response is not a derived value.
  // Generation guards prevent an earlier read from replacing a newer result.
  $effect(() => {
    disconnected;
    enabled;
    untrack(() => {
      void load();
    });
  });
  onMount(() => {
    return () => {
      alive = false;
    };
  });
</script>

<section aria-label={t("Balance forecast result")}>
  <p class="muted">{t("Estimated balances · not recorded account balances")}</p>
  {#if busy}<p role="status">{t("Calculating forecast…")}</p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if cacheSaveFailed && projection && enabled}<p role="status">
      {t(
        "Forecast calculated, but this browser could not save a cached copy. Current results remain available.",
      )}
    </p>{/if}
  <button onclick={load} disabled={busy || disconnected || !enabled}
    >{t("Recalculate forecast")}</button
  >
  {#if projection && enabled}
    {#if cached}<p role="status">
        {t(
          "Cached estimate. Inputs may have changed; reconnect and recalculate before relying on it.",
        )}
      </p>{/if}
    <p>
      {t("As of")}
      {date(projection.as_of)}{projection.as_of !== localDay()
        ? ` · ${t("Historical calculation; recalculate for today.")}`
        : ""}
    </p>
    {#if chart.length}
      <dl>
        <div>
          <dt>{t("Lowest displayed balance")}</dt>
          <dd>
            {money(projection.lowest_displayed?.balance ?? 0)} · {date(
              projection.lowest_displayed?.date ?? "",
            )}
          </dd>
        </div>
        <div>
          <dt>{t("Ending Balance")}</dt>
          <dd>{money(chart.at(-1)!.balance)} · {date(chart.at(-1)!.date)}</dd>
        </div>
      </dl>
      {#if awaitingPreferences}<p>
          {t("Loading display preferences…")}
        </p>{:else}<svg
          viewBox="0 0 600 200"
          role="img"
          aria-label={t(
            "Estimated balance trend. Exact values follow in the table.",
          )}
          ><path
            d={path}
            fill="none"
            stroke="var(--accent)"
            stroke-width="3"
          />{#if chart.length === 1}<circle
              cx="20"
              cy="180"
              r="4"
              fill="var(--accent)"
            />{/if}</svg
        >{/if}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
      <div
        class="scroll"
        tabindex="0"
        role="region"
        aria-label={t("Forecast balance table")}
      >
        <table aria-label={t("Forecast balances")}>
          <thead
            ><tr><th>{t("Date")}</th><th>{t("Estimated balance")}</th></tr
            ></thead
          ><tbody
            >{#each rows as row (row.date)}<tr
                ><td>{date(row.date)}</td><td>{money(row.balance)}</td></tr
              >{/each}</tbody
          >
        </table>
      </div>
      <div class="pager">
        <button disabled={!page} onclick={() => page--}
          >{t("Previous balances")}</button
        ><span
          >{page * 50 + 1}–{Math.min((page + 1) * 50, chart.length)} / {chart.length}</span
        ><button
          disabled={(page + 1) * 50 >= chart.length}
          onclick={() => page++}>{t("Next balances")}</button
        >
      </div>
    {:else}<p>
        {t(
          "No projected balances for this selection. Choose accounts or review the filters.",
        )}
      </p>{/if}
    <p>
      {t("Scheduled occurrences in displayed range:")}
      {projection.scheduled_occurrence_count}
    </p>
    <details>
      <summary>{t("Scheduled entry details")}</summary>
      <p>
        {t(
          "Simulated entries only. Transfers can appear once per selected account; the occurrence count counts each schedule and date once. Posted history is included in balances, not repeated here.",
        )}
      </p>
      {#if !entries.length}<p>
          {t("No simulated entries in this range.")}
        </p>{:else}<!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial tables need keyboard access.) -->
        <div
          class="scroll"
          tabindex="0"
          role="region"
          aria-label={t("Forecast schedule details")}
        >
          <table>
            <thead
              ><tr
                ><th>{t("Date")}</th><th>{t("Account")}</th><th
                  >{t("Schedule")}</th
                ><th>{t("Amount")}</th></tr
              ></thead
            ><tbody
              >{#each details as row (row.id)}<tr
                  ><td>{date(row.date)}</td><td>{row.account}</td><td
                    >{row.scheduleName || row.payee}</td
                  ><td>{money(row.amount)}</td></tr
                >{/each}</tbody
            >
          </table>
        </div>
        <div class="pager">
          <button disabled={!detailPage} onclick={() => detailPage--}
            >{t("Previous scheduled entries")}</button
          ><span
            >{detailPage * 50 + 1}–{Math.min(
              (detailPage + 1) * 50,
              entries.length,
            )} / {entries.length}</span
          ><button
            disabled={(detailPage + 1) * 50 >= entries.length}
            onclick={() => detailPage++}>{t("Next scheduled entries")}</button
          >
        </div>{/if}
    </details>
    <details>
      <summary>{t("Assumptions and limits")}</summary
      >{#each projection.assumptions as a (a.code)}<p>{t(a.message)}</p>{/each}
      <p>
        {t(
          "Forecasts are estimates based on the selected inputs. They do not predict unrecorded purchases. Large selections may need fewer accounts or a shorter date range.",
        )}
      </p>
    </details>
  {/if}
</section>

<style>
  section {
    min-width: 0;
  }
  .muted {
    color: var(--muted);
  }
  p {
    max-width: 70ch;
  }
  svg {
    width: 100%;
    max-height: 240px;
    display: block;
    margin: 1rem 0;
  }
  dl {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem 3rem;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0.35rem 0;
    font-variant-numeric: tabular-nums;
  }
  .scroll {
    overflow: auto;
    max-width: 100%;
    margin-top: 1rem;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    text-align: left;
    padding: 0.6rem;
    border-bottom: 1px solid var(--line);
    overflow-wrap: anywhere;
  }
  td:last-child {
    white-space: nowrap;
  }
  .pager {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    align-items: center;
    margin: 0.75rem 0;
  }
  button,
  summary {
    min-height: 44px;
  }
  summary {
    cursor: pointer;
    padding: 0.65rem 0;
  }
  details {
    border-top: 1px solid var(--line);
    margin-top: 1rem;
  }
  button {
    max-width: 100%;
  }
</style>
