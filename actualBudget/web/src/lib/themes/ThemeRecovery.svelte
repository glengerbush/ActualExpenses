<script lang="ts">
  import { themes, exportJson } from "./themes.svelte";
  import { t } from "../i18n.svelte";
</script>

{#if themes.record.enabled || themes.error}
  <aside class="theme-recovery" aria-label={t("Custom theme recovery")}>
    {#if themes.error}<p role="alert">{t(themes.error)}</p>{/if}
    <button type="button" onclick={() => themes.reset()}
      >{t("Use built-in theme")}</button
    >
    {#if themes.error}<button
        type="button"
        onclick={() =>
          exportJson(themes.export(), "actualBudget-theme-source.json")}
        >{t("Export original theme data")}</button
      >{/if}
  </aside>
{/if}

<style>
  .theme-recovery {
    position: relative;
    width: 100%;
    padding: 0.6rem;
    background: #fff;
    color: #17231b;
    border: 2px solid #34533d;
    border-radius: 6px;
    font:
      1rem/1.4 system-ui,
      sans-serif;
  }
  .theme-recovery p {
    margin: 0 0 0.5rem;
    color: #17231b;
    font: inherit;
  }
  .theme-recovery button {
    font: inherit;
    background: #fff;
    color: #17231b;
    border: 1px solid #34533d;
    margin: 0.15rem;
    padding: 0.4rem 0.6rem;
    min-height: 44px;
  }
  .theme-recovery button:hover {
    background: #e7eee8;
    color: #17231b;
  }
  .theme-recovery button:focus-visible {
    outline: 3px solid #153f24;
    outline-offset: 2px;
  }
</style>
