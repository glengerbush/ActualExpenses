<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { errorMessage, RequestError, uuid, type Account } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import {
    loadReporting,
    readReportOperation,
    prepareReportOperation,
    sendReportOperation,
    cancelReportOperation,
    completeReportOperation,
    readCustomDraft,
    customDraftKey,
    newCustomReport,
    customReportQuery,
    queryReport,
    flag,
    localDay,
    balanceModes,
    graphTypes,
    liveDateRanges,
    type Reporting,
    type ReportDocument,
    type ReportOperation,
    type ReportQuery,
    type Projection,
  } from "./reports";
  import ReportConditions from "./ReportConditions.svelte";
  import ReportResult from "./ReportResult.svelte";
  let {
    budgetId,
    accounts,
    disconnected = false,
    initialId = "",
    onchanged,
  }: {
    budgetId: string;
    accounts: Account[];
    disconnected?: boolean;
    initialId?: string;
    onchanged: (registry: Reporting) => void;
  } = $props();
  let registry = $state.raw<Reporting | null>(null),
    document = $state<ReportDocument | null>(null),
    original = $state.raw<ReportDocument | null>(null),
    pending = $state.raw<ReportOperation | null>(null),
    epoch = $state(""),
    busy = $state(true),
    reading = $state(false),
    cached = $state(false),
    settingsCached = $state(false),
    storageBlocked = $state(false),
    error = $state(""),
    notice = $state(""),
    confirmDelete = $state(false),
    projection = $state.raw<Projection | null>(null),
    query = $state.raw<ReportQuery | null>(null),
    previewConfig = $state("");
  let saved = $derived(
    (registry?.documents || [])
      .filter((d) => d.kind === "custom_report" && !d.tombstone)
      .sort((a, b) =>
        String(a.config.name).localeCompare(String(b.config.name)),
      ),
  );
  let config = $derived(document?.config || {}),
    changed = $derived(
      !!document &&
        (!original ||
          JSON.stringify(config) !== JSON.stringify(original.config)),
    ),
    locked = $derived(
      busy || !!pending || cached || disconnected || storageBlocked,
    ),
    ownsPending = $derived(
      pending?.body.writes.every((d) => d.kind === "custom_report") ?? false,
    );
  const selectionKey = () => `view:custom-report-selection:${budgetId}`,
    cacheKey = (id: string) => `view:custom-report:${budgetId}:${id}`;
  let alive = true;
  async function persist() {
    if (document)
      await writeValue(
        customDraftKey(budgetId),
        changed
          ? {
              version: 1,
              origin: location.origin,
              budget: budgetId,
              epoch,
              document: $state.snapshot(document),
            }
          : null,
      );
  }
  async function update(work: () => void) {
    work();
    confirmDelete = false;
    error = "";
    try {
      await persist();
    } catch (e) {
      storageBlocked = true;
      error = errorMessage(e);
    }
  }
  function field(key: string, value: unknown) {
    void update(() => {
      if (document) document.config[key] = value;
    });
  }
  function balance(value: string) {
    void update(() => {
      if (!document) return;
      document.config.balance_type = value;
      if (value === "Budgeted") {
        if (["Payee", "Account"].includes(String(config.group_by)))
          document.config.group_by = "Category";
        if (["Daily", "Weekly"].includes(String(config.interval)))
          document.config.interval = "Monthly";
      }
    });
  }
  function staticDates(value: boolean) {
    void update(() => {
      if (!document) return;
      if (value && query) {
        document.config.start_date = query.options.startDate;
        document.config.end_date = query.options.endDate;
      }
      document.config.date_static = value ? 1 : 0;
    });
  }
  async function preview() {
    if (!document || busy || reading) return;
    reading = true;
    error = "";
    try {
      await persist();
      const captured = JSON.stringify($state.snapshot(config));
      const q = await customReportQuery(
          budgetId,
          $state.snapshot(config),
          localDay(),
          preferences.shared.first_day_of_week,
        ),
        p = await queryReport(budgetId, q);
      if (!alive) return;
      query = q;
      projection = p;
      previewConfig = captured;
      cached = settingsCached;
      await writeValue(cacheKey(document.id), {
        query: q,
        projection: p,
        config: captured,
      });
    } catch (e) {
      error = errorMessage(e);
      if (document) {
        const prior = await readValue<{
          query: ReportQuery;
          projection: Projection;
          config: string;
        }>(cacheKey(document.id)).catch(() => undefined);
        if (prior && prior.config === JSON.stringify($state.snapshot(config))) {
          query = prior.query;
          projection = prior.projection;
          previewConfig = prior.config;
          cached = true;
        }
      }
    } finally {
      reading = false;
    }
  }
  async function load() {
    busy = true;
    error = "";
    try {
      const response = await loadReporting(budgetId);
      registry = response.data;
      settingsCached = response.cached;
      cached = response.cached;
      pending = await readReportOperation(budgetId);
      const draft = await readCustomDraft(budgetId);
      if (draft) {
        document = draft.document;
        epoch = draft.epoch;
        original =
          registry.documents.find((d) => d.id === document!.id) || null;
      } else if (
        pending &&
        pending.body.writes.every((d) => d.kind === "custom_report")
      ) {
        document = structuredClone(pending.body.writes[0]);
        epoch = pending.epoch;
        original =
          registry.documents.find((d) => d.id === document!.id) || null;
      } else {
        const id = initialId || (await readValue<string>(selectionKey()));
        const selected = saved.find((d) => d.id === id) || saved[0];
        document = selected ? structuredClone(selected) : newCustomReport();
        original = selected || null;
        epoch = registry.epoch;
      }
      if (original && !draft) await writeValue(selectionKey(), document!.id);
      storageBlocked = false;
    } catch (e) {
      storageBlocked = true;
      error = errorMessage(e);
    } finally {
      busy = false;
    }
    if (document && !pending) await preview();
  }
  onMount(() => {
    void load();
    return () => {
      alive = false;
    };
  });
  async function choose(id: string) {
    if (locked) return;
    const selected = saved.find((d) => d.id === id);
    document = selected ? structuredClone(selected) : newCustomReport();
    original = selected || null;
    epoch = registry?.epoch || "";
    query = null;
    projection = null;
    previewConfig = "";
    notice = "";
    confirmDelete = false;
    await writeValue(customDraftKey(budgetId), null);
    await writeValue(selectionKey(), document.id);
    await preview();
  }
  async function discard() {
    if (locked) return;
    await writeValue(customDraftKey(budgetId), null);
    document = original ? structuredClone(original) : null;
    await choose(original?.id || saved[0]?.id || "");
    notice = "Local changes discarded.";
  }
  async function reset() {
    if (locked || !document) return;
    busy = true;
    error = "";
    try {
      const response = await loadReporting(budgetId);
      if (response.cached)
        throw Error("Reconnect before resetting to the current saved report.");
      registry = response.data;
      settingsCached = false;
      const current = registry.documents.find(
        (d) => d.id === document!.id && !d.tombstone,
      );
      if (!current)
        throw Error(
          "This report is no longer saved. Duplicate the draft to keep it as a new report.",
        );
      document = structuredClone(current);
      original = current;
      epoch = registry.epoch;
      await writeValue(customDraftKey(budgetId), null);
      notice = "Reset to the current saved report.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
    await preview();
  }
  async function deliver(cancel = false) {
    if (!pending || busy || !ownsPending) return;
    busy = true;
    error = "";
    let refresh = false;
    try {
      if (cancel) pending = await cancelReportOperation(pending);
      const intent = pending.body.writes[0],
        confirmed = await sendReportOperation(pending);
      await completeReportOperation(pending);
      pending = null;
      registry = confirmed;
      cached = false;
      settingsCached = false;
      onchanged(confirmed);
      epoch = confirmed.epoch;
      if (intent.tombstone) {
        document = {
          ...structuredClone($state.snapshot(document!)),
          id: uuid(),
          revision: 0,
          tombstone: false,
        };
        original = null;
        await persist();
        notice =
          "Saved report removed. The current view remains an unsaved draft.";
      } else {
        const current = confirmed.documents.find((d) => d.id === intent.id)!;
        document = structuredClone(current);
        original = current;
        await writeValue(customDraftKey(budgetId), null);
        await writeValue(selectionKey(), current.id);
        notice = "Report saved.";
      }
      confirmDelete = false;
      refresh = true;
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        pending
      ) {
        await completeReportOperation(pending);
        pending = null;
        notice =
          "Reporting change canceled before saving. Your draft is retained.";
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
    if (refresh) await preview();
  }
  async function save(writes?: ReportDocument[]) {
    if (!document || !registry || locked) return;
    error = "";
    try {
      await persist();
      if (epoch !== registry.epoch)
        throw Error(
          "This draft belongs to an earlier budget version. Keep it for reference, then reload a saved report or duplicate it before saving.",
        );
      pending = await prepareReportOperation(
        budgetId,
        epoch,
        writes || [structuredClone($state.snapshot(document))],
      );
      await deliver();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function duplicate() {
    if (!document || locked) return;
    document = {
      ...structuredClone($state.snapshot(document)),
      id: uuid(),
      revision: 0,
      tombstone: false,
      config: {
        ...$state.snapshot(config),
        name: String(config.name || "Report").slice(0, 114) + " copy",
      },
    };
    original = null;
    epoch = registry?.epoch || "";
    await persist();
    await save();
  }
</script>

<section aria-labelledby="custom-title">
  <header>
    <h3 id="custom-title">{t("Custom reports")}</h3>
    <p class="muted">
      {t("Save a view of recorded entries or planned budget allocations.")}
    </p>
  </header>
  {#if error}<p role="alert">{t(error)}</p>{/if}{#if notice}<p role="status">
      {t(notice)}
    </p>{/if}{#if busy}<p role="status">{t("Loading saved reports…")}</p>{/if}
  {#if pending}<section class="pending">
      <h4>{t("Saved reporting change needs confirmation")}</h4>
      <p>
        {pending.cancelRequested
          ? t("Cancellation is pending. Retrying keeps the cancellation fence.")
          : t("The original request and budget version are retained.")}
      </p>
      {#if ownsPending}<button
          disabled={busy || disconnected}
          onclick={() => deliver()}>{t("Retry saved reporting change")}</button
        ><button
          disabled={busy || disconnected || pending.cancelRequested}
          onclick={() => deliver(true)}
          >{t("Cancel saved reporting change")}</button
        >{:else}<p>
          {t(
            "Return to Dashboards to recover its saved change before editing reports.",
          )}
        </p>{/if}
    </section>{/if}
  <div class="actions">
    <label
      >{t("Saved report")}<select
        value={original?.id || ""}
        onchange={(e) => {
          void choose(e.currentTarget.value);
        }}
        disabled={locked || changed}
        ><option value="">{t("New report")}</option
        >{#each saved as report (report.id)}<option value={report.id}
            >{String(report.config.name)}</option
          >{/each}</select
      ></label
    ><button onclick={() => choose("")} disabled={locked || changed}
      >{t("New custom report")}</button
    ><button onclick={discard} disabled={locked || !changed}
      >{t("Discard local changes")}</button
    ><button onclick={duplicate} disabled={locked || !document}
      >{t("Duplicate and save")}</button
    ><button
      onclick={() => (confirmDelete = true)}
      disabled={locked || !original || changed}
      >{t("Delete saved report")}</button
    >
  </div>
  {#if confirmDelete && original}<section class="confirmation">
      <p>
        {t("Remove “")}{String(original.config.name)}{t(
          "” from saved reports? Its current query stays available as a draft.",
        )}
      </p>
      <button
        onclick={() =>
          save([{ ...structuredClone(original!), tombstone: true }])}
        disabled={locked}>{t("Confirm delete report")}</button
      ><button onclick={() => (confirmDelete = false)} disabled={busy}
        >{t("Keep saved report")}</button
      >
    </section>{/if}
  {#if document}<form
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
      aria-label={t("Custom report editor")}
    >
      <fieldset disabled={locked}>
        <label
          >{t("Report name")}<input
            required
            maxlength="120"
            value={String(config.name || "")}
            oninput={(e) => field("name", e.currentTarget.value)}
          /></label
        >
        <div class="field-grid">
          <label
            >{t("Report format")}<select
              value={String(config.graph_type || "BarGraph")}
              onchange={(e) => field("graph_type", e.currentTarget.value)}
              >{#if !graphTypes.some((g) => g[0] === config.graph_type)}<option
                  value={String(config.graph_type)}
                  >{t("Preserved:")} {String(config.graph_type)}</option
                >{/if}{#each graphTypes as graph (graph[0])}<option
                  value={graph[0]}>{t(graph[1])}</option
                >{/each}</select
            ></label
          ><label
            >{t("Report mode")}<select
              value={String(config.mode || "total")}
              onchange={(e) => field("mode", e.currentTarget.value)}
              ><option value="total">{t("Total")}</option><option value="time"
                >{t("Over time")}</option
              ></select
            ></label
          ><label
            >{t("Balance mode")}<select
              value={String(config.balance_type || "Payment")}
              onchange={(e) => balance(e.currentTarget.value)}
              >{#if !balanceModes.some((m) => m[0] === config.balance_type)}<option
                  value={String(config.balance_type)}
                  >{t("Preserved:")} {String(config.balance_type)}</option
                >{/if}{#each balanceModes as mode (mode[0])}<option
                  value={mode[0]}>{t(mode[0])}</option
                >{/each}</select
            ></label
          ><label
            >{t("Group report by")}<select
              value={String(config.group_by || "Category")}
              onchange={(e) => field("group_by", e.currentTarget.value)}
              >{#each [["Category", "Category"], ["Group", "Category group"], ["CategoryGroup", "Category group and category"], ["Payee", "Payee"], ["Interval", "Interval"]] as group (group[0])}<option
                  value={group[0]}
                  disabled={config.balance_type === "Budgeted" &&
                    ["Payee", "Account"].includes(group[0])}
                  >{t(group[1])}</option
                >{/each}</select
            ></label
          ><label
            >{t("Report interval")}<select
              value={String(config.interval || "Monthly")}
              onchange={(e) => field("interval", e.currentTarget.value)}
              >{#each ["Daily", "Weekly", "Monthly", "Yearly"] as interval (interval)}<option
                  value={interval}
                  disabled={config.balance_type === "Budgeted" &&
                    ["Daily", "Weekly"].includes(interval)}
                  >{t(interval)}</option
                >{/each}</select
            ></label
          ><label
            >{t("Sort report")}<select
              value={String(config.sort_by || "desc")}
              onchange={(e) => field("sort_by", e.currentTarget.value)}
              ><option value="desc">{t("Largest first")}</option><option
                value="asc">{t("Smallest first")}</option
              ><option value="name">{t("Name")}</option><option value="budget"
                >{t("Budget order")}</option
              ></select
            ></label
          >
        </div>
        <details>
          <summary>{t("Dates, visibility and filters")}</summary>
          <div class="options">
            <fieldset>
              <legend>{t("Report dates")}</legend><label class="check"
                ><input
                  type="checkbox"
                  checked={flag(config.date_static)}
                  onchange={(e) => staticDates(e.currentTarget.checked)}
                />{t("Use static dates")}</label
              >{#if flag(config.date_static)}<div class="field-grid">
                  <label
                    >{t("Start date")}<input
                      required
                      value={String(config.start_date || "")}
                      oninput={(e) =>
                        field("start_date", e.currentTarget.value)}
                      placeholder="YYYY-MM-DD"
                    /></label
                  ><label
                    >{t("End date")}<input
                      required
                      value={String(config.end_date || "")}
                      oninput={(e) => field("end_date", e.currentTarget.value)}
                      placeholder="YYYY-MM-DD"
                    /></label
                  >
                </div>
                <p class="muted">
                  {t(
                    "Dates accept a calendar day, month or year. Monthly/yearly reports include recorded entries throughout the selected intervals.",
                  )}
                </p>{:else}<label
                  >{t("Live date range")}<select
                    value={String(config.date_range || "Last 6 months")}
                    onchange={(e) => field("date_range", e.currentTarget.value)}
                    >{#if !liveDateRanges.includes(String(config.date_range))}<option
                        value={String(config.date_range)}
                        >{t("Preserved:")} {String(config.date_range)}</option
                      >{/if}{#each liveDateRanges as range (range)}<option
                        value={range}>{t(range)}</option
                      >{/each}</select
                  ></label
                ><label class="check"
                  ><input
                    type="checkbox"
                    checked={flag(config.include_current, true)}
                    onchange={(e) =>
                      field("include_current", e.currentTarget.checked ? 1 : 0)}
                  />{t("Include current interval")}</label
                >{/if}
            </fieldset>
            <div class="checks">
              {#each [["show_empty", "Show empty groups", false], ["show_offbudget", "Include off-budget accounts", false], ["show_uncategorized", "Include uncategorized on-budget entries", true], ["trim_intervals", "Trim empty leading and trailing intervals", false], ["show_trend_lines", "Show trend lines for line reports", false]] as option (String(option[0]))}<label
                  class="check"
                  ><input
                    type="checkbox"
                    checked={flag(
                      config[String(option[0])],
                      Boolean(option[2]),
                    )}
                    onchange={(e) =>
                      field(String(option[0]), e.currentTarget.checked ? 1 : 0)}
                  />{t(String(option[1]))}</label
                >{/each}
            </div>
            {#if config.group_by === "Payee"}<p class="muted">
                {t("Payee grouping excludes entries without a payee.")}
              </p>{/if}{#if config.balance_type === "Budgeted"}<p class="muted">
                {t(
                  "Budgeted values are planned allocations. Only category and category-group conditions apply; transaction-only conditions do not change allocations.",
                )}
              </p>{/if}
            <ReportConditions
              {budgetId}
              {accounts}
              conditions={config.conditions}
              conditionsOp={config.conditions_op}
              onchange={(conditions, conditionsOp) => {
                void update(() => {
                  if (document) {
                    document.config.conditions = conditions;
                    document.config.conditions_op = conditionsOp;
                  }
                });
              }}
            />
          </div>
        </details>
        <div class="actions">
          <button type="button" onclick={preview} disabled={reading}
            >{t("Preview report")}</button
          ><button type="submit" disabled={!String(config.name || "").trim()}
            >{t("Save report")}</button
          ><button type="button" onclick={reset} disabled={!original}
            >{t("Reset to saved")}</button
          >
        </div>
      </fieldset>
    </form>
    {#if changed}<p role="status">
        {t(
          "Unsaved report changes. The draft stays on this browser until you save or reset it.",
        )}
      </p>{/if}{#if reading}<p role="status">
        {t("Loading report values…")}
      </p>{/if}{#if cached}<p role="status">
        {t(
          "Cached report view. Reconnect and refresh current settings before saving.",
        )}
      </p>
      <button onclick={load} disabled={busy || disconnected || !!pending}
        >{t("Reconnect and refresh report")}</button
      >{/if}
    {#if projection && query}<section aria-label={t("Custom report result")}>
        <h4>{String(config.name || "Custom report")}</h4>
        {#if previewConfig !== JSON.stringify($state.snapshot(config))}<p
            role="status"
          >
            {t(
              "The values below use the last preview. Preview again to apply changed options.",
            )}
          </p>{/if}
        <p class="muted">
          {projection.basis === "budget_allocations"
            ? t("Planned budget allocations")
            : t("Recorded entries")} · {String(query.options.startDate)} – {String(
            query.options.endDate,
          )}
        </p>
        {#if ["netAssets", "netDebts"].includes(String(query.options.balanceTypeOp))}<p
            class="muted"
          >
            {t(
              "Net totals are calculated for each interval before combining intervals. Group totals may differ.",
            )}
          </p>{/if}{#key projection.snapshot + JSON.stringify(query)}<ReportResult
            {budgetId}
            {query}
            {projection}
          />{/key}
      </section>{/if}{/if}
</section>

<style>
  .options {
    display: grid;
    gap: 1rem;
    margin-top: 1rem;
  }
  summary {
    cursor: pointer;
    padding: 0.5rem 0;
  }
  header h3 {
    margin-bottom: 0.35rem;
  }
  .actions {
    display: flex;
    gap: 0.75rem;
    align-items: end;
    flex-wrap: wrap;
    margin: 1rem 0;
  }
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }
  input,
  select {
    max-width: 100%;
    width: 100%;
    box-sizing: border-box;
  }
  fieldset {
    border: 1px solid var(--line);
    min-width: 0;
    padding: 1rem;
    display: grid;
    gap: 1rem;
  }
  .field-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    gap: 1rem;
  }
  .checks {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: 0.75rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
  }
  form {
    margin: 1.5rem 0;
  }
  .pending,
  .confirmation {
    padding: 1rem;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  button {
    min-height: 44px;
  }
  section {
    min-width: 0;
  }
</style>
