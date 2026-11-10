<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { Account } from "./api";
  import { object, localDay, type ReportDocument } from "./reports";
  import ReportConditions from "./ReportConditions.svelte";
  import FormulaWidget from "./FormulaWidget.svelte";
  let {
    budgetId,
    accounts,
    document,
    epoch,
    onchange,
  }: {
    budgetId: string;
    accounts: Account[];
    document: ReportDocument;
    epoch: string;
    onchange: (patch: Record<string, unknown>) => void;
  } = $props();
  let meta = $derived(object(document.config.meta)),
    queries = $derived(object(meta.queries));
  let selected = $state(""),
    error = $state("");
  let query = $derived(object(queries[selected])),
    frame = $derived(object(query.timeFrame));
  const modes = [
    ["", "No date restriction"],
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
  function setQueries(next: Record<string, unknown>) {
    onchange({
      queries: next,
      queriesVersion: Number(meta.queriesVersion || 0) + 1,
    });
  }
  function updateQuery(patch: Record<string, unknown>) {
    if (selected && Object.hasOwn(queries, selected))
      setQueries({ ...queries, [selected]: { ...query, ...patch } });
  }
  function addQuery() {
    const name = String(meta._queryName ?? "").trim();
    if (!name || name.length > 512) {
      error = t("Enter a query name (up to 512 characters).");
      return;
    }
    if (Object.hasOwn(queries, name)) {
      error = t("Query with this name already exists");
      return;
    }
    if (Object.keys(queries).length >= 32) {
      error = t("A formula supports up to 32 queries.");
      return;
    }
    error = "";
    selected = name;
    onchange({ _queryName: "" });
    setQueries({
      ...queries,
      [name]: {
        conditions: [],
        conditionsOp: "and",
        timeFrame: {
          start: localDay().slice(0, 7) + "-01",
          end: localDay(),
          mode: "sliding-window",
        },
      },
    });
  }
  function removeQuery() {
    const next = { ...queries };
    delete next[selected];
    selected = "";
    setQueries(next);
  }
  function importQuery() {
    try {
      const value: unknown = JSON.parse(String(meta._queryImport || ""));
      if (!value || typeof value !== "object" || Array.isArray(value))
        throw Error();
      const q = value as Record<string, unknown>;
      if (
        Object.keys(q).some(
          (k) => !["conditions", "conditionsOp", "timeFrame"].includes(k),
        ) ||
        (q.conditions !== undefined &&
          (!Array.isArray(q.conditions) || q.conditions.length > 20)) ||
        (q.conditionsOp !== undefined &&
          !["and", "or"].includes(String(q.conditionsOp))) ||
        (q.timeFrame != null &&
          (typeof q.timeFrame !== "object" || Array.isArray(q.timeFrame)))
      )
        throw Error();
      setQueries({ ...queries, [selected]: q });
      onchange({ _queryImport: "" });
      error = "";
    } catch {
      error = t(
        "Invalid query configuration. Check the JSON object, conditions and date range.",
      );
    }
  }
</script>

<div class="formula-fields">
  <label
    >{t("Formula")}<textarea
      required
      maxlength="8192"
      rows="4"
      spellcheck="false"
      value={String(meta.formula ?? "=SUM(1, 2, 3)")}
      oninput={(e) => onchange({ formula: e.currentTarget.value })}
    ></textarea></label
  >
  <p>
    {t(
      'Start with =. Use QUERY("name"), QUERY_COUNT, QUERY_EXTRACT_CATEGORIES, BUDGET_QUERY, BALANCE_OF, FORMATNUMBER and FORMATCURRENCY alongside spreadsheet functions.',
    )}
  </p>
  <label
    >{t("Color formula")}<textarea
      maxlength="8192"
      rows="2"
      spellcheck="false"
      value={String(meta.colorFormula ?? "")}
      oninput={(e) => onchange({ colorFormula: e.currentTarget.value })}
    ></textarea></label
  >
  <p>
    {t(
      'Optional: =IF(RESULT<0,"#a00","#060"). RESULT is the calculated value. theme_reportsNumberPositive, theme_reportsNumberNegative and theme_reportsNumberNeutral follow the current theme.',
    )}
  </p>
  <div class="grid">
    <label
      >{t("Font size mode")}<select
        value={String(meta.fontSizeMode || "dynamic")}
        onchange={(e) => onchange({ fontSizeMode: e.currentTarget.value })}
      >
        <option value="dynamic">{t("Dynamic")}</option><option value="static"
          >{t("Static")}</option
        >
      </select></label
    >
    {#if meta.fontSizeMode === "static"}<label
        >{t("Font size")}<input
          type="number"
          required
          min="12"
          max="200"
          value={typeof meta.staticFontSize === "number"
            ? meta.staticFontSize
            : String(meta.staticFontSize ?? 32)}
          oninput={(e) =>
            onchange({
              staticFontSize: Number.isFinite(e.currentTarget.valueAsNumber)
                ? e.currentTarget.valueAsNumber
                : "",
            })}
        /></label
      >{/if}
    <label class="check"
      ><input
        type="checkbox"
        checked={meta.showTitle !== false}
        onchange={(e) => onchange({ showTitle: e.currentTarget.checked })}
      />{t("Show title")}</label
    >
  </div>
  <h4>{t("Named queries")}</h4>
  <div class="grid">
    <label
      >{t("New query name")}<input
        maxlength="512"
        value={String(meta._queryName ?? "")}
        oninput={(e) => onchange({ _queryName: e.currentTarget.value })}
      /></label
    >
    <button type="button" onclick={addQuery}>{t("Add Query")}</button>
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if Object.keys(queries).length}
    <label
      >{t("Edit query")}<select
        value={selected}
        onchange={(e) => {
          selected = e.currentTarget.value;
          error = "";
        }}
      >
        <option value="">{t("Choose a query")}</option>
        {#each Object.keys(queries) as name (name)}<option value={name}
            >{name}</option
          >{/each}
      </select></label
    >
    {#if selected && Object.hasOwn(queries, selected)}
      <div class="query">
        <h5>{selected}</h5>
        <button type="button" onclick={removeQuery}>{t("Remove query")}</button>
        <p>
          {t(
            "Removing a query does not rewrite formulas that reference its name.",
          )}
        </p>
        <div class="grid">
          <label
            >{t("Query date mode")}<select
              value={String(frame.mode ?? "")}
              onchange={(e) =>
                updateQuery({
                  timeFrame: e.currentTarget.value
                    ? { ...frame, mode: e.currentTarget.value }
                    : null,
                })}
            >
              {#each modes as mode (mode[0])}<option value={mode[0]}
                  >{t(mode[1])}</option
                >{/each}
            </select></label
          >
          {#if frame.mode}
            <label
              >{t("Query start month")}<input
                type="month"
                required
                value={String(frame.start || localDay()).slice(0, 7)}
                oninput={(e) =>
                  updateQuery({
                    timeFrame: {
                      ...frame,
                      start: e.currentTarget.value + "-01",
                    },
                  })}
              /></label
            >
            <label
              >{t("Query end month")}<input
                type="month"
                required
                value={String(frame.end || localDay()).slice(0, 7)}
                oninput={(e) =>
                  updateQuery({
                    timeFrame: { ...frame, end: e.currentTarget.value },
                  })}
              /></label
            >
          {/if}
        </div>
        {#key selected}<ReportConditions
            {budgetId}
            {accounts}
            conditions={query.conditions}
            conditionsOp={query.conditionsOp}
            sourceSyntax
            namedConditionsIgnored={false}
            onchange={(conditions, conditionsOp) =>
              updateQuery({ conditions, conditionsOp })}
          />{/key}
        <details>
          <summary>{t("Import or export query configuration")}</summary>
          <p>
            {t(
              "Use this JSON field to transfer a query configuration. Export fills the field for copying, including on plain HTTP.",
            )}
          </p>
          <button
            type="button"
            onclick={() =>
              onchange({ _queryImport: JSON.stringify(query, null, 2) })}
            >{t("Export query configuration")}</button
          >
          <label
            >{t("Query configuration JSON")}<textarea
              rows="6"
              maxlength="32768"
              spellcheck="false"
              value={String(meta._queryImport ?? "")}
              oninput={(e) => onchange({ _queryImport: e.currentTarget.value })}
            ></textarea></label
          >
          <button type="button" onclick={importQuery}
            >{t("Import query configuration")}</button
          >
        </details>
      </div>
    {/if}
  {:else}<p>
      {t(
        "No queries defined. Add a named query to calculate a filtered summary.",
      )}
    </p>{/if}
  <FormulaWidget {budgetId} {document} {epoch} preview />
</div>

<style>
  .formula-fields {
    display: grid;
    gap: 1rem;
    min-width: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
    align-items: end;
  }
  label {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }
  textarea,
  input,
  select {
    width: 100%;
    max-width: 100%;
    min-height: 44px;
  }
  textarea {
    font-family: ui-monospace, monospace;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .check input {
    width: auto;
  }
  button,
  summary {
    min-height: 44px;
    max-width: 100%;
  }
  .query {
    border-block: 1px solid var(--line);
    padding-block: 1rem;
    display: grid;
    gap: 1rem;
    min-width: 0;
  }
  p {
    max-width: 70ch;
    margin: 0;
  }
  h4,
  h5 {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
