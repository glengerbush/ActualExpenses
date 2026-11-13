<script lang="ts">
  import BackupSettings from "./BackupSettings.svelte";
  import { onMount } from "svelte";
  import { preferences } from "./preferences.svelte";
  import { languages, languageState, t } from "./i18n.svelte";
  import {
    dateFormats,
    numberFormats,
    type SharedPreferences,
  } from "./preferences/types";
  import { SettingsSession } from "./preferences/SettingsSession.svelte";
  let { budgetId, onclose }: { budgetId?: string; onclose: () => void } =
    $props();
  let session = $state<SettingsSession | null>(null);
  const formatLabels = [
    "1,000.33",
    "1.000,33",
    "1 000,33",
    "1’000.33",
    "1,00,000.33",
  ];

  function reviewValues() {
    if (!session?.review?.values || !session.draft) return [];
    const current = session.review.values,
      original = session.draft;
    const labels: Record<string, string> = {
      number_format: "Numbers",
      date_format: "Dates",
      hide_fraction: "Hide decimal places",
      first_day_of_week: "First day of the week",
    };
    function shown(key: string, value: unknown): string {
      if (typeof value === "boolean") return t(value ? "On" : "Off");
      if (key === "number_format")
        return formatLabels[
          numberFormats.indexOf(value as (typeof numberFormats)[number])
        ];
      if (key === "date_format") return String(value).toUpperCase();
      if (key === "first_day_of_week")
        return t(
          [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
          ][Number(value)],
        );
      return t(String(value));
    }
    return Object.keys(labels)
      .filter(
        (key) =>
          original.base[key as keyof SharedPreferences] !==
          original.values[key as keyof SharedPreferences],
      )
      .map((key) => ({
        key,
        label: t(labels[key]),
        current: shown(key, current[key as keyof SharedPreferences]),
        requested: shown(key, original.values[key as keyof SharedPreferences]),
      }));
  }
  function update(changes: Partial<SharedPreferences>) {
    session?.update(changes);
  }
  function languageName(code: string) {
    return new Intl.DisplayNames([code], { type: "language" }).of(code) ?? code;
  }
  function weekday(day: number) {
    return new Intl.DateTimeFormat(languageState.language, {
      weekday: "long",
      timeZone: "UTC",
    }).format(new Date(Date.UTC(2026, 10, 1 + day)));
  }
  function close() {
    onclose();
  }
  onMount(() => {
    if (!budgetId) return;
    session = new SettingsSession(budgetId);
    session.load();
  });
</script>

<section class="settings" aria-labelledby="settings-title">
  <div class="page-heading">
    <div>
      <h1 id="settings-title">{t("Settings")}</h1>
      <p class="muted">
        {t("Appearance on this device and formatting for this budget.")}
      </p>
    </div>
  </div>
  <BackupSettings />
  <section aria-labelledby="device-settings">
    <h2 id="device-settings">{t("This device")}</h2>
    <p class="hint">{t("Theme and language stay on this browser.")}</p>
    <div class="settings-grid">
      <div>
        <label for="theme">{t("Theme")}</label><select
          id="theme"
          bind:value={
            () => preferences.device.theme,
            (theme) => void preferences.updateDevice({ theme })
          }
          ><option value="system">{t("System default")}</option><option
            value="light">{t("Light")}</option
          ><option value="dark">{t("Dark")}</option><option value="midnight"
            >{t("Midnight")}</option
          ></select
        >
      </div>
      <div>
        <label for="preferred-dark-theme">{t("Preferred dark theme")}</label
        ><select
          id="preferred-dark-theme"
          bind:value={
            () => preferences.device.preferred_dark_theme,
            (preferred_dark_theme) =>
              void preferences.updateDevice({ preferred_dark_theme })
          }
          ><option value="dark">{t("Dark")}</option><option value="midnight"
            >{t("Midnight")}</option
          ></select
        >
      </div>
      <div>
        <label for="language">{t("Language")}</label><select
          id="language"
          bind:value={
            () => preferences.device.language,
            (language) => void preferences.updateDevice({ language })
          }
          ><option value="">{t("System default")}</option
          >{#each languages as language (language)}<option value={language}
              >{languageName(language)}</option
            >{/each}</select
        >
      </div>
    </div>
    {#if preferences.deviceError}<p role="alert" class="error">
        {t(preferences.deviceError)}
      </p>
      <button onclick={() => preferences.retryLanguage()}>{t("Retry")}</button
      >{/if}
  </section>
  {#if budgetId}<section aria-labelledby="shared-settings">
      <h2 id="shared-settings">{t("This budget")}</h2>
      <p class="hint">
        {t(
          "Shared with every device using this budget. Formatting does not change stored amounts or dates.",
        )}
      </p>
      {#if preferences.error}<p class="error" role="alert">
          {t(preferences.error)}
          {#if preferences.cachedAt}{t("Cached preferences from {{date}}.", {
              date: preferences.cachedAt,
            })}{/if}
        </p>
        <button
          onclick={() => preferences.loadBudget(budgetId)}
          disabled={preferences.loading}>{t("Refresh")}</button
        >{/if}
      {#if session?.draft}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void session?.save();
          }}
          aria-busy={session.busy}
        >
          <fieldset
            disabled={session.busy ||
              !!session.draft.pending ||
              !preferences.hasUsableShared}
          >
            <div class="settings-grid">
              <div>
                <label for="number-format">{t("Numbers")}</label><select
                  id="number-format"
                  bind:value={
                    () => session!.draft!.values.number_format,
                    (number_format) => update({ number_format })
                  }
                  >{#each numberFormats as value, index (value)}<option {value}
                      >{formatLabels[index]}</option
                    >{/each}</select
                >
              </div>
              <div>
                <label for="date-format">{t("Dates")}</label><select
                  id="date-format"
                  bind:value={
                    () => session!.draft!.values.date_format,
                    (date_format) => update({ date_format })
                  }
                  >{#each dateFormats as value (value)}<option {value}
                      >{value.toUpperCase()}</option
                    >{/each}</select
                >
              </div>
              <div>
                <label for="week-start">{t("First day of the week")}</label
                ><select
                  id="week-start"
                  bind:value={
                    () => session!.draft!.values.first_day_of_week,
                    (first_day_of_week) => update({ first_day_of_week })
                  }
                  >{#each [0, 1, 2, 3, 4, 5, 6] as value (value)}<option {value}
                      >{weekday(value)}</option
                    >{/each}</select
                >
              </div>
            </div>
            <label class="check-label"
              ><input
                type="checkbox"
                bind:checked={
                  () => session!.draft!.values.hide_fraction,
                  (hide_fraction) => update({ hide_fraction })
                }
              />{t("Hide decimal places")}</label
            >
          </fieldset>
          {#if session.error}<p class="error" role="alert">
              {t(session.error)}
            </p>{/if}
          {#if session.status}<p role="status">{t(session.status)}</p>{/if}
          {#if session.blocked}
            <button
              type="button"
              disabled={session.busy}
              onclick={() => session?.reviewCurrent()}
              >{t("Review current budget")}</button
            >
            {#if session.review}
              <p>{t("Current budget:")} {session.review.budget.name}</p>
              {#if session.review.values}
                <dl class="restore-comparison">
                  {#each reviewValues() as item (item.key)}
                    <dt>{item.label}</dt>
                    <dd>
                      {t("Current:")}
                      {item.current} · {t("Your change:")}
                      {item.requested}
                    </dd>
                  {/each}
                </dl>
              {:else}<p>{t("This budget was deleted.")}</p>{/if}
              <div class="actions">
                <button
                  type="button"
                  disabled={session.busy}
                  onclick={() => {
                    if (session?.resolveReviewed(false)) onclose();
                  }}>{t("Discard saved settings")}</button
                >
                <button
                  type="button"
                  disabled={session.busy || !session.review.values}
                  onclick={() => session?.resolveReviewed(true)}
                  >{t("Reapply as new settings")}</button
                >
              </div>
            {/if}
          {/if}
          {#if session.draft.pending && !session.busy && !session.blocked}<p
              role="status"
              class="error"
            >
              {t(
                "The outcome is unconfirmed. Retry the saved changes before editing them.",
              )}
            </p>{/if}
          <div class="actions">
            <button
              type="button"
              disabled={session.busy || !!session.draft.pending}
              onclick={() => {
                if (session?.cancel()) onclose();
              }}>{t("Discard changes")}</button
            ><button class="primary" disabled={session.busy}
              >{t(
                session.busy
                  ? "Saving…"
                  : session.draft.pending
                    ? "Retry saving"
                    : "Save shared preferences",
              )}</button
            >
          </div>
        </form>
      {:else if session?.error}<p class="error" role="alert">
          {t(session.error)}
        </p>
        <button onclick={() => session?.load()}
          >{t("Retry draft recovery")}</button
        >{/if}
    </section>{/if}
</section>
