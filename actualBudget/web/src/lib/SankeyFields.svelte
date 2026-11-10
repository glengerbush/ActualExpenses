<script lang="ts">
  import { t } from "./i18n.svelte";
  import { sankeyLayers, layerLabels, type SankeyLayer } from "./sankey";
  let {
    meta,
    onchange,
  }: {
    meta: Record<string, unknown>;
    onchange: (patch: Record<string, unknown>) => void;
  } = $props();
  let mode = $derived(String(meta.mode ?? "spent"));
  let available = $derived(
    sankeyLayers.filter((layer) =>
      mode === "budgeted" ? layer !== "payee" : layer !== "budget",
    ),
  );
  function choices(field: string) {
    const from = String(
        meta.layerFrom ?? (mode === "budgeted" ? "income_category" : "payee"),
      ),
      to = String(meta.layerTo ?? "category"),
      other = available.indexOf(
        (field === "layerFrom" ? to : from) as SankeyLayer,
      );
    return other < 0
      ? available
      : available.filter((_, index) =>
          field === "layerFrom" ? index < other : index > other,
        );
  }
  function changeMode(value: string) {
    const layers = sankeyLayers.filter((layer) =>
        value === "budgeted" ? layer !== "payee" : layer !== "budget",
      ),
      from = String(
        meta.layerFrom ?? (value === "budgeted" ? "income_category" : "payee"),
      ),
      to = String(meta.layerTo ?? "category");
    let first = layers.indexOf(from as SankeyLayer),
      last = layers.indexOf(to as SankeyLayer);
    if (first < 0) first = 0;
    if (last <= first) last = layers.length - 1;
    if (first >= last) first = 0;
    onchange({ mode: value, layerFrom: layers[first], layerTo: layers[last] });
  }
</script>

<div class="fields">
  <label
    >{t("View mode")}<select
      value={mode}
      onchange={(event) => changeMode(event.currentTarget.value)}
      ><option value="spent">{t("Spent")}</option><option value="budgeted"
        >{t("Budgeted")}</option
      >{#if !["spent", "budgeted"].includes(mode)}<option value={mode}
          >{t("Preserved:")} {mode}</option
        >{/if}</select
    ></label
  >
  <p>
    {mode === "budgeted"
      ? t(
          "Budgeted flows combine planned allocations with recorded income and the complete budget context. Only category/group conditions filter this view; named category conditions apply. Other saved conditions are retained and ignored.",
        )
      : t(
          "Spent flows use recorded entries grouped by category, account and income payee. Named saved conditions are retained and ignored.",
        )}
  </p>
  <div class="grid">
    <label
      >{t("Maximum nodes per layer")}<input
        type="number"
        required
        min="1"
        max="100000"
        step="1"
        value={Number(meta.topNcategories ?? 15)}
        oninput={(event) =>
          onchange({ topNcategories: event.currentTarget.valueAsNumber })}
      /></label
    ><label
      >{t("Sorting")}<select
        value={String(meta.categorySort ?? "per-group")}
        onchange={(event) =>
          onchange({ categorySort: event.currentTarget.value })}
        >{#each [["per-group", "Sort per group"], ["global", "Sort all"], ["budget-order", "Sort as budget"]] as option (option[0])}<option
            value={option[0]}>{t(option[1])}</option
          >{/each}{#if meta.categorySort && !["per-group", "global", "budget-order"].includes(String(meta.categorySort))}<option
            value={String(meta.categorySort)}
            >{t("Preserved:")} {String(meta.categorySort)}</option
          >{/if}</select
      ></label
    >
  </div>
  <p>
    {t(
      "100000 means All. Small flows are grouped into Other to fit the widget height; this maximum is retained separately from that display limit.",
    )}
  </p>
  <div class="grid">
    {#each [["layerFrom", "From layer", mode === "budgeted" ? "income_category" : "payee"], ["layerTo", "To layer", "category"]] as field (field[0])}<label
        >{t(field[1])}<select
          value={String(meta[field[0]] ?? field[2])}
          onchange={(event) =>
            onchange({ [field[0]]: event.currentTarget.value })}
          >{#each choices(field[0]) as layer (layer)}<option value={layer}
              >{t(layerLabels[layer])}</option
            >{/each}{#if meta[field[0]] && !choices(field[0]).includes(String(meta[field[0]]) as SankeyLayer)}<option
              value={String(meta[field[0]])}
              >{t("Preserved:")} {String(meta[field[0]])}</option
            >{/if}</select
        ></label
      >{/each}
  </div>
  <label class="check"
    ><input
      type="checkbox"
      checked={meta.showPercentages === true}
      onchange={(event) =>
        onchange({ showPercentages: event.currentTarget.checked })}
    />{t("Show percentages")}</label
  >
  {#if mode === "spent"}<label class="check"
      ><input
        type="checkbox"
        checked={meta.groupAccounts === true}
        onchange={(event) =>
          onchange({ groupAccounts: event.currentTarget.checked })}
      />{t("Group accounts")}</label
    ><label class="check"
      ><input
        type="checkbox"
        disabled={meta.groupAccounts === true}
        checked={meta.showTransfers === true}
        onchange={(event) =>
          onchange({ showTransfers: event.currentTarget.checked })}
      />{t("Show transfers")}</label
    >{#if meta.groupAccounts === true}<p>
        {t(
          "Transfers between grouped accounts are not drawn. The saved transfer setting is retained.",
        )}
      </p>{/if}{/if}
  <p>
    {t(
      "Negative allocations and refunds follow the saved report’s flow conventions. Flow totals can differ from the budget summary; a flow diagram does not show account balances.",
    )}
  </p>
</div>

<style>
  .fields {
    display: grid;
    gap: 0.8rem;
    min-width: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }
  input,
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
  p {
    max-width: 70ch;
    margin: 0.2rem 0;
  }
</style>
