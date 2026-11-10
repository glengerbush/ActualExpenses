<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { errorMessage, RequestError, uuid, type Account } from "./api";
  import { readValue, writeValue } from "./offline/storage";
  import {
    loadReporting,
    readReportOperation,
    prepareReportOperation,
    sendReportOperation,
    cancelReportOperation,
    completeReportOperation,
    draftKey,
    readCustomDraft,
    object,
    localDay,
    type Reporting,
    type ReportDocument,
    type ReportOperation,
  } from "./reports";
  import { preferences } from "./preferences.svelte";
  import ForecastFields from "./ForecastFields.svelte";
  import { addMonths } from "./forecasts";
  import CustomReports from "./CustomReports.svelte";
  import ReportWidget from "./ReportWidget.svelte";
  import ReportConditions from "./ReportConditions.svelte";
  let {
    budgetId,
    accounts = [],
    disconnected = false,
    onclose,
  }: {
    budgetId: string;
    accounts?: Account[];
    disconnected?: boolean;
    onclose: () => void;
  } = $props();
  let data = $state.raw<Reporting | null>(null),
    pending = $state.raw<ReportOperation | null>(null),
    editor = $state<ReportDocument | null>(null),
    editorEpoch = $state(""),
    selected = $state(""),
    busy = $state(true),
    cached = $state(false),
    error = $state(""),
    notice = $state(""),
    confirm = $state(""),
    storageBlocked = $state(false),
    tab = $state("dashboards"),
    customId = $state("");
  let customReports = $derived(
    (data?.documents || []).filter(
      (d) => d.kind === "custom_report" && !d.tombstone,
    ),
  );
  async function switchTab(value: string, id = "") {
    tab = value;
    customId = id;
    await writeValue(`view:reporting-tab:${budgetId}`, value);
    if (value !== "custom") await load();
  }
  function reportsChanged(value: Reporting) {
    data = value;
    pending = null;
    cached = false;
  }
  let pages = $derived(
    (data?.documents || []).filter((d) => d.kind === "page" && !d.tombstone),
  );
  let widgets = $derived(
    (data?.documents || [])
      .filter(
        (d) =>
          d.kind === "widget" &&
          !d.tombstone &&
          d.config.dashboard_page_id === selected,
      )
      .sort(
        (a, b) =>
          Number(a.config.y) - Number(b.config.y) ||
          Number(a.config.x) - Number(b.config.x) ||
          a.id.localeCompare(b.id),
      ),
  );
  let page = $derived(pages.find((d) => d.id === selected));
  let meta = $derived(object(editor?.config.meta));
  let forecastEnabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.shared.experimental.balanceForecastReport,
  );
  let displayedWidgets = $derived(
    tab === "forecast"
      ? widgets.filter((w) => w.config.type === "balance-forecast-card")
      : widgets,
  );
  let types = $derived([
    ["cash-flow-card", "Cash flow"],
    ["net-worth-card", "Net worth"],
    ["spending-card", "Spending analysis"],
    ["markdown-card", "Text"],
    ["summary-card", "Summary"],
    ["calendar-card", "Transaction calendar"],
    ["custom-report", "Saved custom report"],
    ...(forecastEnabled ? [["balance-forecast-card", "Balance forecast"]] : []),
  ]);
  const dateModes = [
    ["sliding-window", "Live sliding window"],
    ["static", "Static dates"],
    ["full", "Through latest recorded month"],
    ["lastMonth", "Last month"],
    ["lastYear", "Last year"],
    ["yearToDate", "Year to date"],
    ["priorYearToDate", "Prior year to date"],
    ["currentQuarter", "Current quarter"],
    ["previousQuarter", "Previous quarter"],
  ];
  let alive = true,
    initialized = false;
  async function remember() {
    await writeValue(`view:reporting-selection:${budgetId}`, selected);
  }
  async function persistDraft() {
    if (editor)
      await writeValue(draftKey(budgetId), {
        version: 1,
        epoch: editorEpoch,
        document: $state.snapshot(editor),
      });
  }
  async function change(work: () => void) {
    work();
    try {
      await persistDraft();
    } catch (e) {
      storageBlocked = true;
      error = errorMessage(e);
    }
  }
  function config(key: string, value: unknown) {
    void change(() => {
      if (editor) editor.config[key] = value;
    });
  }
  function metadata(key: string, value: unknown) {
    void change(() => {
      if (editor) editor.config.meta = { ...meta, [key]: value };
    });
  }
  function timeframe(key: string, value: string) {
    metadata("timeFrame", {
      ...(editor?.config.type === "balance-forecast-card"
        ? {
            mode: "static",
            start: meta.startDate || localDay().slice(0, 7),
            end: meta.endDate || addMonths(localDay().slice(0, 7), 11),
          }
        : {}),
      ...object(meta.timeFrame),
      [key]: value,
    });
  }
  function summaryType() {
    try {
      return (
        object(
          typeof meta.content === "string"
            ? JSON.parse(meta.content)
            : meta.content,
        ).type || "sum"
      );
    } catch {
      return "sum";
    }
  }
  function setSummaryType(type: string) {
    let previous: Record<string, unknown> = {};
    try {
      previous = object(
        typeof meta.content === "string"
          ? JSON.parse(meta.content)
          : meta.content,
      );
    } catch {}
    metadata(
      "content",
      JSON.stringify({
        ...previous,
        type,
        ...(type === "percentage" && previous.type !== "percentage"
          ? {
              divisorConditions: [{ field: "amount", op: "gt", value: 0 }],
              divisorConditionsOp: "and",
              divisorAllTimeDateRange: false,
            }
          : {}),
      }),
    );
  }
  async function load() {
    busy = true;
    error = "";
    try {
      const response = await loadReporting(budgetId);
      if (!alive) return;
      data = response.data;
      cached = response.cached;
      pending = await readReportOperation(budgetId);
      if (!initialized) {
        initialized = true;
        if (
          pending?.body.writes.every((d) => d.kind === "custom_report") ||
          (await readCustomDraft(budgetId))
        )
          tab = "custom";
        else
          tab =
            (await readValue<string>(`view:reporting-tab:${budgetId}`)) ||
            "dashboards";
      }
      const prior = await readValue<string>(
        `view:reporting-selection:${budgetId}`,
      );
      if (!pages.some((p) => p.id === selected))
        selected = pages.some((p) => p.id === prior)
          ? prior!
          : pages[0]?.id || "";
      const draft = await readValue<{
        version: number;
        epoch: string;
        document: ReportDocument;
      }>(draftKey(budgetId));
      if (draft?.version === 1) {
        editor = draft.document;
        editorEpoch = draft.epoch;
      }
      storageBlocked = false;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      alive = false;
    };
  });
  async function edit(document: ReportDocument) {
    error = "";
    confirm = "";
    editor = structuredClone(document);
    editorEpoch = data?.epoch || "";
    try {
      await persistDraft();
    } catch (e) {
      storageBlocked = true;
      error = errorMessage(e);
    }
  }
  async function newPage() {
    await edit({
      id: uuid(),
      kind: "page",
      revision: 0,
      config: { name: "" },
      tombstone: false,
    });
  }
  async function newWidget(type = "cash-flow-card") {
    const today = localDay();
    let position = {
      x: 0,
      y: Math.max(
        0,
        ...widgets.map((w) => Number(w.config.y) + Number(w.config.height)),
      ),
    };
    outer: for (const y of [
      ...new Set([
        0,
        ...widgets.map((w) => Number(w.config.y) + Number(w.config.height)),
      ]),
    ].sort((a, b) => a - b)) {
      for (const x of [0, 6]) {
        if (
          !widgets.some(
            (w) =>
              x < Number(w.config.x) + Number(w.config.width) &&
              x + 6 > Number(w.config.x) &&
              y < Number(w.config.y) + Number(w.config.height) &&
              y + 3 > Number(w.config.y),
          )
        ) {
          position = { x, y };
          break outer;
        }
      }
    }
    await edit({
      id: uuid(),
      kind: "widget",
      revision: 0,
      config: {
        dashboard_page_id: selected,
        type,
        ...position,
        width: 6,
        height: 3,
        meta: {
          name: "",
          timeFrame: {
            start: today.slice(0, 7),
            end:
              type === "balance-forecast-card"
                ? addMonths(today.slice(0, 7), 11)
                : today.slice(0, 7),
            mode:
              type === "balance-forecast-card" ? "static" : "sliding-window",
          },
        },
      },
      tombstone: false,
    });
  }
  async function resetEditor() {
    if (!editor || pending) return;
    const saved = data?.documents.find((d) => d.id === editor!.id);
    if (saved) await edit(saved);
    else {
      editor = null;
      await writeValue(draftKey(budgetId), null);
    }
  }
  async function closeEditor() {
    if (pending) return;
    editor = null;
    await writeValue(draftKey(budgetId), null);
  }
  async function deliver(cancel = false) {
    if (!pending || busy) return;
    busy = true;
    error = "";
    try {
      if (cancel) pending = await cancelReportOperation(pending);
      const confirmed = await sendReportOperation(pending);
      await completeReportOperation(pending);
      data = confirmed;
      cached = false;
      pending = null;
      editor = null;
      await writeValue(draftKey(budgetId), null);
      confirm = "";
      notice = "Reporting change saved.";
      if (!pages.some((p) => p.id === selected)) selected = pages[0]?.id || "";
      await remember();
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        pending
      ) {
        await completeReportOperation(pending);
        pending = null;
        notice = "Change canceled before saving. Your draft is retained.";
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function commit(writes: ReportDocument[]) {
    if (!data || busy || pending || cached || disconnected || storageBlocked)
      return;
    error = "";
    try {
      if (editor) {
        await persistDraft();
        if (editorEpoch !== data.epoch)
          throw Error(
            "This draft belongs to an earlier budget version. Keep it for reference, then discard or reset before saving.",
          );
      }
      pending = await prepareReportOperation(budgetId, data.epoch, writes);
      await deliver();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function saveEditor() {
    if (!editor) return;
    if (editor.kind === "page") selected = editor.id;
    await commit([structuredClone($state.snapshot(editor))]);
  }
  async function duplicatePage() {
    if (!page) return;
    const id = uuid();
    await commit([
      {
        ...structuredClone(page),
        id,
        revision: 0,
        config: {
          ...page.config,
          name: String(page.config.name).slice(0, 114) + " copy",
        },
      },
      ...widgets.map((w) => ({
        ...structuredClone(w),
        id: uuid(),
        revision: 0,
        config: { ...w.config, dashboard_page_id: id },
      })),
    ]);
    if (!pending && !error) {
      selected = id;
      await remember();
    }
  }
  async function remove(document: ReportDocument) {
    await commit([
      { ...structuredClone(document), tombstone: true },
      ...(document.kind === "page"
        ? widgets.map((w) => ({ ...structuredClone(w), tombstone: true }))
        : []),
    ]);
  }
</script>

<section aria-labelledby="reporting-title">
  <header class="heading">
    <div>
      <h2 id="reporting-title">{t("Dashboards and reports")}</h2>
      <p class="muted">
        {t("Recorded history, saved views and transaction details.")}
      </p>
    </div>
    <button onclick={onclose}>{t("Close reports")}</button>
  </header>
  <nav class="actions" aria-label={t("Reporting views")}>
    <button
      aria-pressed={tab === "dashboards"}
      disabled={!!editor || busy}
      onclick={() => switchTab("dashboards")}>{t("Dashboards")}</button
    ><button
      aria-pressed={tab === "custom"}
      disabled={!!editor || busy}
      onclick={() => switchTab("custom")}>{t("Custom reports")}</button
    >
    {#if forecastEnabled || tab === "forecast"}<button
        aria-pressed={tab === "forecast"}
        disabled={!!editor || busy}
        onclick={() => switchTab("forecast")}>{t("Balance forecast")}</button
      >{/if}
  </nav>
  {#if tab === "custom"}{#key customId}<CustomReports
        {budgetId}
        {accounts}
        {disconnected}
        initialId={customId}
        onchanged={reportsChanged}
      />{/key}
  {:else}
    {#if tab === "forecast"}<p>
        {t(
          "Forecasts are saved dashboard widgets. Choose a dashboard, then create or edit a forecast. Estimates never change recorded balances.",
        )}
      </p>
      {#if !forecastEnabled}<p role="status">
          {t(
            "Enable Balance forecast in Settings → Experimental features. Saved settings are preserved.",
          )}
        </p>{/if}{/if}
    {#if error}<p role="alert">{t(error)}</p>{/if}{#if notice}<p role="status">
        {t(notice)}
      </p>{/if}{#if cached}<p role="status">
        {t(
          "Cached dashboard settings. Reconnect and refresh before editing or loading current reports.",
        )}
      </p>{/if}
    {#if pending}<section
        class="pending"
        aria-label={t("Saved reporting change")}
      >
        <h3>{t("Saved change needs confirmation")}</h3>
        <p>
          {t("The original request and budget version are retained.")}
          {pending.cancelRequested
            ? t(
                "Cancellation is pending. Retrying keeps the cancellation fence.",
              )
            : t(
                "Retry to learn whether the change was saved, or request cancellation.",
              )}
        </p>
        <div class="actions">
          <button disabled={busy || disconnected} onclick={() => deliver()}
            >{t("Retry saved reporting change")}</button
          ><button
            disabled={busy || disconnected || pending.cancelRequested}
            onclick={() => deliver(true)}
            >{t("Cancel saved reporting change")}</button
          >
        </div>
      </section>{/if}
    {#if busy}<p role="status">{t("Loading reporting settings…")}</p>{/if}
    <div class="toolbar">
      <label
        >{t("Dashboard")}<select
          bind:value={selected}
          onchange={() => {
            confirm = "";
            void remember();
          }}
          disabled={busy || !!editor || !!pending}
          >{#each pages as value (value.id)}<option value={value.id}
              >{String(value.config.name)}</option
            >{/each}</select
        ></label
      >
      <div class="actions">
        <button
          onclick={newPage}
          disabled={busy || !!editor || !!pending || cached || disconnected}
          >{t("New dashboard")}</button
        ><button
          onclick={load}
          disabled={busy || !!editor || !!pending || disconnected}
          >{t("Refresh dashboards")}</button
        >{#if page}<button
            onclick={() => edit(page!)}
            disabled={busy || !!editor || !!pending || cached || disconnected}
            >{t("Rename dashboard")}</button
          ><button
            onclick={duplicatePage}
            disabled={busy || !!editor || !!pending || cached || disconnected}
            >{t("Duplicate dashboard")}</button
          ><button
            onclick={() => (confirm = page!.id)}
            disabled={busy ||
              !!editor ||
              !!pending ||
              pages.length < 2 ||
              cached ||
              disconnected}>{t("Delete dashboard")}</button
          ><button
            onclick={() =>
              newWidget(
                tab === "forecast" ? "balance-forecast-card" : "cash-flow-card",
              )}
            disabled={busy ||
              !!editor ||
              !!pending ||
              cached ||
              disconnected ||
              (tab === "forecast" && !forecastEnabled)}
            >{t(tab === "forecast" ? "New forecast" : "Add widget")}</button
          >{/if}
      </div>
    </div>
    {#if confirm}{@const document = data?.documents.find(
        (d) => d.id === confirm,
      )}{#if document}<section class="confirmation">
          <p>
            {t("Remove")}
            {document.kind === "page"
              ? t("this dashboard and its widgets")
              : t("this widget")}
            {t("from your saved layout?")}
          </p>
          <button disabled={busy || !!pending} onclick={() => remove(document)}
            >{t("Confirm remove")}</button
          ><button onclick={() => (confirm = "")} disabled={busy}
            >{t("Keep saved layout")}</button
          >
        </section>{/if}{/if}
    {#if editor}<form
        onsubmit={(e) => {
          e.preventDefault();
          void saveEditor();
        }}
        aria-label={t("Reporting editor")}
      >
        <h3>
          {editor.kind === "page"
            ? editor.revision
              ? t("Rename dashboard")
              : t("New dashboard")
            : t("Edit widget")}
        </h3>
        <fieldset disabled={busy || !!pending || cached || disconnected}>
          {#if editor.kind === "page"}<label
              >{t("Dashboard name")}<input
                required
                maxlength="120"
                value={String(editor.config.name || "")}
                oninput={(e) => config("name", e.currentTarget.value)}
              /></label
            >
          {:else}<label
              >{t("Widget type")}<select
                value={String(editor.config.type)}
                onchange={(e) => config("type", e.currentTarget.value)}
                >{#if !types.some((t) => t[0] === editor?.config.type)}<option
                    value={String(editor.config.type)}
                    >{t("Preserved:")} {String(editor.config.type)}</option
                  >{/if}{#each types as type (type[0])}<option value={type[0]}
                    >{t(type[1])}</option
                  >{/each}</select
              ></label
            ><label
              >{t("Widget name")}<input
                maxlength="120"
                value={String(meta.name || "")}
                oninput={(e) => metadata("name", e.currentTarget.value)}
              /></label
            >
            {#if editor.config.type === "markdown-card"}<label
                >{t("Text content")}<textarea
                  rows="5"
                  value={String(meta.content || "")}
                  oninput={(e) => metadata("content", e.currentTarget.value)}
                ></textarea></label
              ><label
                >{t("Text alignment")}<select
                  value={String(meta.text_align || "left")}
                  onchange={(e) =>
                    metadata("text_align", e.currentTarget.value)}
                  ><option value="left">{t("left")}</option><option
                    value="center">{t("center")}</option
                  ><option value="right">{t("right")}</option></select
                ></label
              >
            {:else if editor.config.type === "custom-report"}<label
                >{t("Saved custom report")}<select
                  value={String(meta.id || "")}
                  required
                  onchange={(e) => metadata("id", e.currentTarget.value)}
                  ><option value="">{t("Choose a saved report")}</option
                  >{#if meta.id && !customReports.some((d) => d.id === meta.id)}<option
                      value={String(meta.id)}
                      >{t(
                        "Unavailable saved report (reference preserved)",
                      )}</option
                    >{/if}{#each customReports as report (report.id)}<option
                      value={report.id}>{String(report.config.name)}</option
                    >{/each}</select
                ></label
              >
              <p class="muted">
                {t(
                  "This widget follows the saved report’s settings. Edit the report in Custom reports.",
                )}
              </p>
            {:else}<div class="field-grid">
                <label
                  >{t("Date mode")}<select
                    value={String(
                      object(meta.timeFrame).mode ||
                        (editor.config.type === "balance-forecast-card"
                          ? "static"
                          : "sliding-window"),
                    )}
                    onchange={(e) => timeframe("mode", e.currentTarget.value)}
                    >{#each dateModes as mode (mode[0])}<option value={mode[0]}
                        >{t(mode[1])}</option
                      >{/each}</select
                  ></label
                ><label
                  >{t("Start month")}<input
                    type="month"
                    required
                    value={String(
                      object(meta.timeFrame).start ||
                        (editor.config.type === "balance-forecast-card"
                          ? meta.startDate
                          : null) ||
                        localDay(),
                    ).slice(0, 7)}
                    oninput={(e) => timeframe("start", e.currentTarget.value)}
                  /></label
                ><label
                  >{t("End month")}<input
                    type="month"
                    required
                    value={String(
                      object(meta.timeFrame).end ||
                        (editor.config.type === "balance-forecast-card"
                          ? meta.endDate
                          : null) ||
                        localDay(),
                    ).slice(0, 7)}
                    oninput={(e) => timeframe("end", e.currentTarget.value)}
                  /></label
                >
              </div>
              {#if editor.config.type === "balance-forecast-card"}<ForecastFields
                  {meta}
                  {accounts}
                  onchange={metadata}
                />{/if}
              {#if editor.config.type === "net-worth-card"}<label
                  >{t("Interval")}<select
                    value={String(meta.interval || "Monthly")}
                    onchange={(e) =>
                      metadata("interval", e.currentTarget.value)}
                    >{#each ["Daily", "Weekly", "Monthly", "Yearly"] as interval (interval)}<option
                        value={interval}>{t(interval)}</option
                      >{/each}</select
                  ></label
                ><label
                  >{t("Net worth view")}<select
                    value={String(meta.mode || "trend")}
                    onchange={(e) => metadata("mode", e.currentTarget.value)}
                    ><option value="trend">{t("Trend")}</option><option
                      value="stacked">{t("Stacked accounts")}</option
                    ></select
                  ></label
                ><label
                  >{t("Account breakdown")}<select
                    multiple
                    value={Array.isArray(meta.accounts)
                      ? (meta.accounts as string[])
                      : accounts.map((a) => a.id)}
                    onchange={(e) =>
                      metadata(
                        "accounts",
                        Array.from(e.currentTarget.selectedOptions).map(
                          (o) => o.value,
                        ),
                      )}
                    >{#each accounts as account (account.id)}<option
                        value={account.id}
                        >{account.name}{account.off_budget
                          ? t(" (off budget)")
                          : ""}</option
                      >{/each}</select
                  ></label
                >
              {:else if editor.config.type === "cash-flow-card"}<label
                  class="check"
                  ><input
                    type="checkbox"
                    checked={meta.isConcise === true}
                    onchange={(e) =>
                      metadata("isConcise", e.currentTarget.checked)}
                  />{t("Monthly totals")}</label
                ><label class="check"
                  ><input
                    type="checkbox"
                    checked={meta.showBalance !== false}
                    onchange={(e) =>
                      metadata("showBalance", e.currentTarget.checked)}
                  />{t("Show balance in plot")}</label
                >
              {:else if editor.config.type === "summary-card"}<label
                  >{t("Summary calculation")}<select
                    value={String(summaryType())}
                    onchange={(e) => setSummaryType(e.currentTarget.value)}
                    >{#each [["sum", "Sum"], ["avgPerMonth", "Average per month"], ["avgPerYear", "Average per year"], ["avgPerTransact", "Average per entry"], ["percentage", "Percentage of deposits"]] as type (type[0])}<option
                        value={type[0]}>{t(type[1])}</option
                      >{/each}</select
                  ></label
                >
              {:else if editor.config.type === "spending-card"}<label
                  >{t("Spending comparison")}<select
                    value={String(meta.mode || "single-month")}
                    onchange={(e) => metadata("mode", e.currentTarget.value)}
                    ><option value="single-month"
                      >{t("Previous or selected month")}</option
                    ><option value="budget">{t("Budget")}</option><option
                      value="average">{t("Historical average")}</option
                    ></select
                  ></label
                ><label class="check"
                  ><input
                    type="checkbox"
                    checked={meta.isLive !== false}
                    onchange={(e) =>
                      metadata("isLive", e.currentTarget.checked)}
                  />{t("Live spending dates")}</label
                >
                <div class="field-grid">
                  <label
                    >{t("Selected spending month")}<input
                      type="month"
                      value={String(meta.compare || localDay()).slice(0, 7)}
                      oninput={(e) =>
                        metadata("compare", e.currentTarget.value)}
                    /></label
                  ><label
                    >{t("Comparison month")}<input
                      type="month"
                      value={String(meta.compareTo || "")}
                      oninput={(e) =>
                        metadata("compareTo", e.currentTarget.value)}
                    /></label
                  ><label
                    >{t("Spending average")}<select
                      value={String(
                        object(meta.averageRange).mode || "last-n-months",
                      ) +
                        ":" +
                        String(object(meta.averageRange).months || 3)}
                      onchange={(e) => {
                        const [mode, n] = e.currentTarget.value.split(":");
                        metadata("averageRange", {
                          mode,
                          ...(mode === "last-n-months"
                            ? { months: Number(n) }
                            : {}),
                        });
                      }}
                      ><option value="last-n-months:3"
                        >{t("Last 3 months")}</option
                      ><option value="last-n-months:6"
                        >{t("Last 6 months")}</option
                      ><option value="last-n-months:12"
                        >{t("Last 12 months")}</option
                      ><option value="year-to-date:3"
                        >{t("Year to date")}</option
                      ><option value="all-time:3">{t("All time")}</option
                      ></select
                    ></label
                  >
                </div>{/if}{/if}
            {#if editor.config.type !== "markdown-card"}<ReportConditions
                {budgetId}
                {accounts}
                conditions={meta.conditions}
                conditionsOp={meta.conditionsOp}
                onchange={(conditions, conditionsOp) => {
                  void change(() => {
                    if (editor)
                      editor.config.meta = {
                        ...meta,
                        conditions,
                        conditionsOp,
                      };
                  });
                }}
              />{/if}
            <div class="field-grid">
              {#each [["x", "Column", 0, 11], ["y", "Row", 0, 100000], ["width", "Width", 1, 12], ["height", "Height", 1, 100]] as field (String(field[0]))}<label
                  >{t(String(field[1]))}<input
                    type="number"
                    required
                    min={Number(field[2])}
                    max={Number(field[3])}
                    value={Number(editor.config[String(field[0])])}
                    oninput={(e) =>
                      config(String(field[0]), Number(e.currentTarget.value))}
                  /></label
                >{/each}
            </div>
            <p class="muted">
              {t(
                "Layout uses 12 columns. Column and row controls work with keyboard and touch. Imported conditions and metadata are retained when other settings change.",
              )}
            </p>{/if}
          <div class="actions">
            <button type="submit" disabled={storageBlocked}
              >{editor.kind === "page"
                ? t("Save dashboard")
                : t("Save widget")}</button
            ><button type="button" onclick={resetEditor}
              >{t("Reset to saved")}</button
            ><button type="button" onclick={closeEditor}
              >{t("Discard local draft")}</button
            >
          </div>
        </fieldset>
      </form>{/if}
    {#if !pages.length && !busy && !editor}<p>
        {t("No dashboards yet. Create one, then add a report or text widget.")}
      </p>{/if}
    {#if tab === "forecast" && !displayedWidgets.length && pages.length}<p>
        {t(
          "No saved forecasts on this dashboard. Choose New forecast to begin.",
        )}
      </p>{/if}
    <div class={["dashboard", { forecasts: tab === "forecast" }]}>
      {#each displayedWidgets as widget (widget.id + ":" + widget.revision + ":" + (customReports.find((d) => d.id === object(widget.config.meta).id)?.revision ?? "missing"))}<div
          class="widget"
          style:--column={Number(widget.config.x) + 1}
          style:--width={Number(widget.config.width)}
          style:--row={Number(widget.config.y) + 1}
          style:--height={Number(widget.config.height)}
        >
          <ReportWidget
            {budgetId}
            document={widget}
            epoch={data?.epoch || ""}
            report={customReports.find(
              (d) => d.id === object(widget.config.meta).id,
            ) || null}
            onopenreport={(id) => switchTab("custom", id)}
            disconnected={disconnected ||
              busy ||
              !!editor ||
              !!pending ||
              cached}
            onedit={() => edit(widget)}
            onremove={() => (confirm = widget.id)}
          />
        </div>{/each}
    </div>
  {/if}
</section>

<style>
  section {
    min-width: 0;
  }
  .heading,
  .toolbar,
  .actions {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  .heading h2 {
    margin: 0;
  }
  .toolbar {
    margin: 1.5rem 0;
    align-items: end;
  }
  .actions {
    justify-content: flex-start;
    align-items: center;
  }
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }
  input,
  select,
  textarea {
    max-width: 100%;
    width: 100%;
    box-sizing: border-box;
  }
  .field-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 1rem;
  }
  form,
  .pending,
  .confirmation {
    padding: 1rem;
    background: var(--surface);
    border: 1px solid var(--line);
    margin: 1rem 0;
  }
  fieldset {
    border: 0;
    padding: 0;
    display: grid;
    gap: 1rem;
    min-width: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
  }
  .dashboard {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: 1rem;
    grid-auto-rows: minmax(2rem, auto);
    align-items: stretch;
  }
  .dashboard.forecasts {
    display: flex;
    flex-direction: column;
  }
  .dashboard.forecasts .widget {
    width: 100%;
  }
  .widget {
    min-width: 0;
    grid-column: var(--column) / span var(--width);
    grid-row: var(--row) / span var(--height);
  }
  @media (max-width: 700px) {
    .toolbar > .actions {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      width: 100%;
    }
    .toolbar > .actions button {
      min-width: 0;
      width: 100%;
      white-space: normal;
    }
  }
  @media (max-width: 900px) {
    .dashboard {
      display: flex;
      flex-direction: column;
    }
    .widget {
      width: 100%;
    }
    .toolbar {
      display: block;
    }
    .toolbar .actions {
      margin-top: 1rem;
    }
  }
  button {
    min-height: 44px;
  }
</style>
