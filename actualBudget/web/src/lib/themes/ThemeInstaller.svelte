<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage, request } from "../api";
  import { t } from "../i18n.svelte";
  import { themes, exportJson } from "./themes.svelte";
  import { readThemeValue, writeThemeValue } from "./storage";
  import { fetchCatalogTheme } from "./fetch-theme";
  import type {
    CatalogTheme,
    CompiledTheme,
    ThemeDraft,
    ThemeRecord,
  } from "./types";
  let open = $state(false);
  let catalog = $state<CatalogTheme[]>([]);
  let error = $state("");
  let status = $state("");
  let busy = $state(false);
  let ready = $state(false);
  let result = $state.raw<CompiledTheme | null>(null);
  let importText = $state("");
  let rawDraft: unknown;
  let draft = $state<ThemeDraft>({
    version: 1,
    slot: "light",
    name: "Custom theme",
    base_theme: "light",
    css: "",
  });
  let controller = $state.raw<AbortController | null>(null);
  let mounted = true;
  async function load() {
    try {
      const stored = await readThemeValue<ThemeDraft>("draft");
      rawDraft = stored;
      if (stored) {
        if (
          stored.version !== 1 ||
          !["light", "dark", "override"].includes(stored.slot) ||
          !["light", "dark", "midnight"].includes(stored.base_theme) ||
          typeof stored.css !== "string" ||
          typeof stored.name !== "string"
        )
          throw new Error(
            "Saved theme draft is malformed. Export original theme data before replacing it.",
          );
        draft = stored;
      }
      importText = (await readThemeValue<string>("import-draft")) ?? "";
      ready = true;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function discardDraft() {
    try {
      draft = {
        version: 1,
        slot: "light",
        name: "Custom theme",
        base_theme: "light",
        css: "",
      };
      await writeThemeValue("draft", $state.snapshot(draft));
      ready = true;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function loadCatalog() {
    try {
      const { data } = await request<{ data: { entries: CatalogTheme[] } }>(
        "/themes/catalog",
      );
      if (mounted) {
        catalog = data.entries;
        error = "";
      }
    } catch (e) {
      if (mounted) error = errorMessage(e);
    }
  }
  async function expand() {
    open = !open;
    if (open && !catalog.length) await loadCatalog();
  }
  async function edit(changes: Partial<ThemeDraft>) {
    draft = { ...draft, ...changes };
    result = null;
    status = "";
    try {
      await writeThemeValue("draft", $state.snapshot(draft));
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function apply() {
    if (busy || !ready) return;
    busy = true;
    error = "";
    status = "";
    try {
      await writeThemeValue("draft", $state.snapshot(draft));
      result = draft.css.trim()
        ? await themes.compile(draft.css, draft.base_theme, draft.name)
        : null;
      if (result && !result.valid) {
        error =
          "Theme CSS could not be applied. Original text is saved below; correct the reported errors and retry.";
        return;
      }
      await themes.install(draft.slot, result);
      status = "Custom theme saved on this device.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function install(theme: CatalogTheme) {
    if (busy || !ready) return;
    busy = true;
    error = "";
    status = "";
    controller = new AbortController();
    try {
      const { css, original } = await fetchCatalogTheme(
        theme,
        controller.signal,
        async (source) => {
          if (!mounted)
            throw new DOMException("Download canceled", "AbortError");
          await edit({ css: source, name: theme.name, base_theme: theme.mode });
          await writeThemeValue("draft", $state.snapshot(draft));
        },
      );
      if (!mounted) return;
      await edit({ css, name: theme.name, base_theme: theme.mode });
      const compiled = await themes.compile(
        css,
        theme.mode,
        theme.name,
        `https://github.com/${theme.repo}`,
      );
      if (!mounted) return;
      result = compiled;
      if (!compiled.valid)
        throw new Error(
          "Downloaded theme is unsupported. Its original CSS is saved for review.",
        );
      await themes.install(draft.slot, compiled, original);
      status = "Custom theme saved on this device.";
    } catch (e) {
      if (mounted)
        error =
          e instanceof DOMException &&
          ["AbortError", "TimeoutError"].includes(e.name)
            ? "Theme download canceled or timed out. Previous theme kept; retry when connected."
            : errorMessage(e);
    } finally {
      if (mounted) busy = false;
      controller = null;
    }
  }
  async function editImport(value: string) {
    importText = value;
    try {
      await writeThemeValue("import-draft", value);
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function importOriginal() {
    busy = true;
    error = "";
    try {
      const parsed: unknown = JSON.parse(importText);
      const record = parsed as ThemeRecord;
      if (record?.version === 1 && "enabled" in record) {
        await themes.restore(record);
      } else await themes.save(await themes.convert(parsed));
      await writeThemeValue("import-draft", "");
      importText = "";
      status = "Original theme configuration imported on this device.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      mounted = false;
      controller?.abort();
    };
  });
</script>

<section class="custom-themes" aria-labelledby="custom-themes-title">
  <h3 id="custom-themes-title">{t("Custom themes")}</h3>
  <p class="hint">
    {t(
      "Install a catalog theme or paste compatible CSS. Original sources stay on this device.",
    )}
  </p>
  <button type="button" onclick={expand} aria-expanded={open}
    >{t(open ? "Close theme editor" : "Open theme editor")}</button
  >
  {#if open}
    {#if error}<p class="error" role="alert">{t(error)}</p>{/if}{#if status}<p
        role="status"
      >
        {t(status)}
      </p>{/if}
    {#if !ready}<button type="button" onclick={load}
        >{t("Retry draft recovery")}</button
      >
      <button
        type="button"
        onclick={() =>
          exportJson(rawDraft ?? null, "actualBudget-theme-draft.json")}
        >{t("Export saved theme draft")}</button
      >
      <button type="button" onclick={discardDraft}
        >{t("Discard broken draft")}</button
      >
    {/if}
    <fieldset disabled={busy || !ready}>
      <div class="settings-grid">
        <div>
          <label for="theme-slot">{t("Theme slot")}</label><select
            id="theme-slot"
            bind:value={() => draft.slot, (slot) => void edit({ slot })}
            ><option value="light">{t("Light or single theme")}</option><option
              value="dark">{t("System dark theme")}</option
            ><option value="override">{t("Additional CSS override")}</option
            ></select
          >
        </div>
        <div>
          <label for="custom-base">{t("Base theme")}</label><select
            id="custom-base"
            bind:value={
              () => draft.base_theme, (base_theme) => void edit({ base_theme })
            }
            ><option value="light">{t("Light")}</option><option value="dark"
              >{t("Dark")}</option
            ><option value="midnight">{t("Midnight")}</option></select
          >
        </div>
      </div>
      <p class="hint">
        {t(
          "System appearance uses separate light and dark slots. Additional CSS applies last in both modes.",
        )}
      </p>
      {#if !catalog.length}<button type="button" onclick={loadCatalog}
          >{t("Retry catalog")}</button
        >{/if}
      <label for="catalog-theme">{t("Catalog theme")}</label><select
        id="catalog-theme"
        onchange={(event) => {
          const selected = catalog.find(
            (theme) => theme.repo === event.currentTarget.value,
          );
          if (selected) void install(selected);
          event.currentTarget.value = "";
        }}
        ><option value="">{t("Choose a theme to install")}</option
        >{#each catalog as theme (theme.repo)}<option value={theme.repo}
            >{theme.name} ({theme.mode})</option
          >{/each}</select
      >
      <label for="custom-name">{t("Theme name")}</label><input
        id="custom-name"
        maxlength="200"
        bind:value={() => draft.name, (name) => void edit({ name })}
      />
      <label for="theme-css">{t("Custom Theme CSS")}</label><textarea
        id="theme-css"
        rows="9"
        spellcheck="false"
        bind:value={() => draft.css, (css) => void edit({ css })}
        aria-describedby="theme-css-help"></textarea>
      <p class="hint" id="theme-css-help">
        {t(
          "Use one :root block with CSS variables. Embedded fonts are supported; unsupported overrides are reported and preserved.",
        )}
      </p>
      <button type="button" class="primary" onclick={apply}
        >{t("Apply custom CSS")}</button
      >
    </fieldset>
    {#if busy}<p role="status">{t("Installing theme…")}</p>
      {#if controller}<button type="button" onclick={() => controller?.abort()}
          >{t("Cancel download")}</button
        >{/if}{/if}
    {#if result?.warnings.length}<ul aria-label={t("Theme warnings")}>
        {#each result.warnings as warning, index (index)}<li>
            <strong>{warning.key}</strong>: {warning.reason}
          </li>{/each}
      </ul>{/if}
    {#each [themes.record.light, themes.record.dark, themes.record.override] as theme, index (index)}{#if theme}<details
        >
          <summary
            >{theme.name || t("Custom theme")} — {t(
              theme.valid ? "Installed" : "Preserved without applying",
            )}</summary
          >
          <ul>
            {#each theme.warnings as warning, index (index)}<li>
                {warning.key}: {warning.reason}
              </li>{/each}
          </ul>
          <button
            type="button"
            onclick={() => exportJson(theme, "actualBudget-theme.json")}
            >{t("Export theme source")}</button
          >
        </details>{/if}{/each}
    {#if themes.record.warnings.length}<ul>
        {#each themes.record.warnings as warning, index (index)}<li>
            {warning.key}: {warning.reason}
          </li>{/each}
      </ul>{/if}
    <div class="actions">
      <button type="button" onclick={() => themes.reset()}
        >{t("Use built-in theme")}</button
      ><button
        type="button"
        onclick={() =>
          exportJson(themes.export(), "actualBudget-theme-source.json")}
        >{t("Export original theme data")}</button
      >
    </div>
    <details>
      <summary>{t("Import theme configuration")}</summary><label
        for="theme-import"
        >{t("Saved theme data or Actual global preferences JSON")}</label
      ><textarea
        id="theme-import"
        rows="5"
        disabled={busy}
        bind:value={() => importText, (value) => void editImport(value)}
      ></textarea><button
        type="button"
        disabled={busy || !importText.trim()}
        onclick={importOriginal}>{t("Import on this device")}</button
      >
    </details>
  {/if}
</section>

<style>
  .custom-themes {
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--line);
  }
  .custom-themes textarea {
    font-family: ui-monospace, monospace;
  }
  .custom-themes details {
    margin-top: 1rem;
  }
  .custom-themes li {
    overflow-wrap: anywhere;
  }
</style>
