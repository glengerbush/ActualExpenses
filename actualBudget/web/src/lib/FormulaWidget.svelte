<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t, languageState } from "./i18n.svelte";
  import { preferences } from "./preferences.svelte";
  import { themes } from "./themes/themes.svelte";
  import { errorMessage, RequestError } from "./api";
  import { captureAmountFormat, formatAmount } from "./money";
  import { readValue, writeValue } from "./offline/storage";
  import { object, type ReportDocument } from "./reports";
  import {
    calculateFormula,
    validFormulaResult,
    type FormulaResult,
  } from "./formulas";
  let {
    budgetId,
    document,
    epoch,
    disconnected = false,
    preview = false,
  }: {
    budgetId: string;
    document: ReportDocument;
    epoch: string;
    disconnected?: boolean;
    preview?: boolean;
  } = $props();
  let value = $state.raw<FormulaResult | null>(null),
    busy = $state(false),
    error = $state(""),
    cached = $state(false),
    cacheFailed = $state(false);
  let generation = 0,
    alive = true;
  let meta = $derived(object(document.config.meta));
  let enabled = $derived(
    preferences.budgetId === budgetId &&
      preferences.hasUsableShared &&
      preferences.budgetEpoch === epoch &&
      preferences.shared.experimental.formulaMode,
  );
  let hidden = $derived(!enabled || preferences.shared.privacy_mode);
  let text = $derived.by(() => {
    if (hidden) return "••••";
    const output = value?.output;
    if (!output || output.result === null) return "";
    if (typeof output.result === "number") {
      const minor = Math.round(
        output.result * 10 ** captureAmountFormat().decimal_places,
      );
      return Number.isSafeInteger(minor)
        ? formatAmount(minor)
        : String(output.result);
    }
    return String(output.result);
  });
  let color = $derived.by(() => {
    const candidate = value?.color?.result;
    return !hidden &&
      !value?.color?.error &&
      typeof candidate === "string" &&
      candidate.length < 256 &&
      CSS.supports("color", candidate)
      ? candidate
      : undefined;
  });
  let fontSize = $derived(
    meta.fontSizeMode === "static" && Number.isFinite(meta.staticFontSize)
      ? `${Math.max(12, Math.min(200, Number(meta.staticFontSize))) / 16}rem`
      : "clamp(1.5rem, 4cqi, 4rem)",
  );
  const key = () => `view:formula:${budgetId}:${document.id}`;
  const signature = () =>
    JSON.stringify([
      document.revision,
      preferences.shared.revision,
      languageState.language,
      preferences.device.theme,
      themes.appearance,
    ]);
  async function load() {
    const current = ++generation,
      expected = signature();
    busy = true;
    error = "";
    cached = false;
    cacheFailed = false;
    try {
      if (!enabled)
        throw Error(
          "Enable Formula in Settings → Experimental features. Saved settings are preserved.",
        );
      if (disconnected)
        throw Error("Reconnect to calculate the current formula.");
      const next = await calculateFormula(budgetId, document, epoch, preview);
      if (!alive || current !== generation) return;
      if (!validFormulaResult(next, epoch))
        throw new RequestError(
          "Formula result is unreadable. Refresh dashboards and retry.",
        );
      value = next;
      if (!preview)
        try {
          await writeValue(key(), { value: next, signature: expected });
        } catch {
          if (alive && current === generation) cacheFailed = true;
        }
    } catch (e) {
      if (!alive || current !== generation) return;
      value = null;
      error = errorMessage(e);
      if (!preview && enabled && !(e instanceof RequestError && !e.uncertain)) {
        const prior = await readValue<{
          value: FormulaResult;
          signature: string;
        }>(key()).catch(() => null);
        if (
          alive &&
          current === generation &&
          prior?.signature === expected &&
          validFormulaResult(prior.value, epoch)
        ) {
          value = prior.value;
          cached = true;
        }
      }
    } finally {
      if (alive && current === generation) busy = false;
    }
  }
  $effect(() => {
    enabled;
    disconnected;
    document.revision;
    preferences.shared.revision;
    languageState.language;
    preferences.device.theme;
    themes.appearance;
    if (!preview) untrack(() => void load());
  });
  onMount(() => () => {
    alive = false;
    generation++;
  });
</script>

<section aria-label={t(preview ? "Formula preview" : "Formula result")}>
  <p>{t("Calculated summary · does not change recorded balances")}</p>
  {#if !enabled}<p>
      {t(
        "Enable Formula in Settings → Experimental features. Saved settings are preserved.",
      )}
    </p>{/if}
  <button
    type="button"
    onclick={load}
    disabled={busy || disconnected || !enabled}
    >{t(preview ? "Calculate preview" : "Recalculate formula")}</button
  >
  {#if busy}<p role="status">{t("Calculating formula…")}</p>{/if}
  {#if error}<p role="alert">{t(error)}</p>{/if}
  {#if cacheFailed}<p role="status">
      {t(
        "Calculated, but this browser could not save a cached copy. Current results remain available.",
      )}
    </p>{/if}
  {#if value}
    {#if cached}<p role="status">
        {t(
          "Cached formula. Inputs may have changed; reconnect and recalculate before relying on it.",
        )}
      </p>{/if}
    {#if value.output.error}<p role="alert">
        {t(value.output.error)}
      </p>{:else}<output
        aria-label={t("Calculated value")}
        style:color
        style:font-size={fontSize}>{text}</output
      >{/if}
    {#if value.color?.error}<p role="alert">
        {t("Color formula:")}
        {t(value.color.error)}
      </p>{/if}
    <p>{t("Calculated at")} {value.evaluated_at}</p>
    {#if preview}<p>
        {t(
          "Preview uses the settings present when Calculate preview was selected.",
        )}
      </p>{/if}
  {/if}
</section>

<style>
  section {
    min-width: 0;
    container-type: inline-size;
    overflow-wrap: anywhere;
  }
  output {
    display: block;
    margin-block: 1rem;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-variant-numeric: tabular-nums;
  }
  p {
    max-width: 70ch;
  }
  button {
    min-height: 44px;
    max-width: 100%;
  }
</style>
