<script lang="ts">
  import { t } from "./i18n.svelte";
  let {
    meta,
    onchange,
  }: {
    meta: Record<string, unknown>;
    onchange: (key: string, value: unknown) => void;
  } = $props();
</script>

<div class="fields">
  <p>
    {t(
      "Monthly budget analysis uses planned allocations and recorded entries in expense categories. Category and group filters select the categories included; other saved filters are retained and ignored.",
    )}
  </p>
  <label
    >{t("Chart type")}<select
      value={String(meta.graphType ?? "Bar")}
      onchange={(event) => onchange("graphType", event.currentTarget.value)}
      ><option value="Line">{t("Line Graph")}</option><option value="Bar"
        >{t("Bar Graph")}</option
      >{#if meta.graphType && !["Line", "Bar"].includes(String(meta.graphType))}<option
          value={String(meta.graphType)}
          >{t("Preserved:")} {String(meta.graphType)}</option
        >{/if}</select
    ></label
  >
  <label class="check"
    ><input
      type="checkbox"
      checked={meta.balanceOnly === true}
      onchange={(event) => onchange("balanceOnly", event.currentTarget.checked)}
    />{t("Balance only")}</label
  >
  {#if meta.balanceOnly === true}<p>
      {t(
        "Balance-only view displays a line. The saved chart style and other series settings are retained.",
      )}
    </p>{:else}<label class="check"
      ><input
        type="checkbox"
        checked={meta.showBalance !== false}
        onchange={(event) =>
          onchange("showBalance", event.currentTarget.checked)}
      />{t("Show balance")}</label
    >{/if}
  <label class="check"
    ><input
      type="checkbox"
      checked={meta.showHiddenCategories === true}
      onchange={(event) =>
        onchange("showHiddenCategories", event.currentTarget.checked)}
    />{t("Show hidden categories")}</label
  >
  {#if meta.interval && meta.interval !== "Monthly"}<p>
      {t(
        "The saved interval is retained. Budget analysis calculates monthly values.",
      )}
    </p>{/if}
  <p>
    {t(
      "Income and uncategorized entries are excluded. Rollover and overspending adjustments follow the selected budgeting method; the result can differ from the budget summary.",
    )}
  </p>
</div>

<style>
  .fields {
    display: grid;
    gap: 0.8rem;
    min-width: 0;
  }
  p {
    max-width: 70ch;
    margin: 0.2rem 0;
  }
  label {
    display: grid;
    gap: 0.4rem;
  }
  select {
    max-width: 100%;
    min-height: 44px;
  }
  .check {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .check input {
    width: auto;
    flex: none;
    margin: 0;
  }
</style>
