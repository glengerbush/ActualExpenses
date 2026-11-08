<script lang="ts">
  import { tick } from "svelte";
  import { preferences } from "./preferences.svelte";
  import { languageState, t } from "./i18n.svelte";
  import { today } from "./money";
  import {
    calendarWeeks,
    dateObject,
    displayDate,
    moveDay,
    moveMonth,
    parseDateInput,
    validDate,
  } from "./dates";
  import type { DateFormat } from "./preferences/types";
  let {
    id,
    label,
    value = $bindable(""),
    required = false,
    disabled = false,
    format,
  }: {
    id: string;
    label: string;
    value?: string;
    required?: boolean;
    disabled?: boolean;
    format?: DateFormat;
  } = $props();
  let open = $state(false);
  let cursor = $state(today());
  let month = $state(today().slice(0, 7));
  let input: HTMLInputElement | undefined;
  let calendar: HTMLDivElement | undefined;
  const pattern = $derived(format ?? preferences.shared.date_format);
  const firstDay = $derived(preferences.shared.first_day_of_week);
  const weeks = $derived(calendarWeeks(month, firstDay));
  const days = $derived(
    Array.from({ length: 7 }, (_, index) => (firstDay + index) % 7),
  );
  function weekday(day: number) {
    return new Intl.DateTimeFormat(languageState.language, {
      weekday: "short",
      timeZone: "UTC",
    }).format(dateObject(`2026-11-0${day + 1}`));
  }
  function dateLabel(date: string) {
    return new Intl.DateTimeFormat(languageState.language, {
      dateStyle: "full",
      timeZone: "UTC",
    }).format(dateObject(date));
  }
  function monthLabel() {
    return new Intl.DateTimeFormat(languageState.language, {
      month: "long",
      year: "numeric",
      timeZone: "UTC",
    }).format(dateObject(`${month}-01`));
  }
  function validateInput() {
    input?.setCustomValidity(
      (!value && !required) || validDate(parseDateInput(value, pattern))
        ? ""
        : t("Enter a valid date."),
    );
  }
  function edit(raw: string) {
    // Persist unfinished text, including invalid dates, without expanding a
    // valid abbreviation while the user is still typing its remaining digits.
    value = raw;
    validateInput();
  }
  function commit() {
    value = parseDateInput(value, pattern);
    validateInput();
  }
  async function focusDay() {
    await tick();
    calendar
      ?.querySelector<HTMLButtonElement>(`[data-date="${cursor}"]`)
      ?.focus();
  }
  async function toggle() {
    open = !open;
    if (open) {
      cursor = validDate(value) ? value : today();
      month = cursor.slice(0, 7);
      await focusDay();
    }
  }
  function choose(date: string) {
    value = date;
    input?.setCustomValidity("");
    open = false;
    input?.focus();
  }
  async function pageMonth(offset: number) {
    cursor = moveMonth(cursor, offset);
    month = cursor.slice(0, 7);
    await focusDay();
  }
  async function keydown(event: KeyboardEvent, date: string) {
    let next = date;
    switch (event.key) {
      case "ArrowLeft":
        next = moveDay(date, -1);
        break;
      case "ArrowRight":
        next = moveDay(date, 1);
        break;
      case "ArrowUp":
        next = moveDay(date, -7);
        break;
      case "ArrowDown":
        next = moveDay(date, 7);
        break;
      case "Home":
        next = moveDay(
          date,
          -((dateObject(date).getUTCDay() - firstDay + 7) % 7),
        );
        break;
      case "End":
        next = moveDay(
          date,
          6 - ((dateObject(date).getUTCDay() - firstDay + 7) % 7),
        );
        break;
      case "PageUp":
        next = moveMonth(date, -1);
        break;
      case "PageDown":
        next = moveMonth(date, 1);
        break;
      case "Escape":
        event.preventDefault();
        open = false;
        input?.focus();
        return;
      default:
        return;
    }
    event.preventDefault();
    cursor = next;
    month = next.slice(0, 7);
    await focusDay();
  }
</script>

<div class="date-field">
  <label for={id}>{label}</label>
  <div class="date-control">
    <input
      {id}
      type="text"
      inputmode="numeric"
      autocomplete="off"
      maxlength="10"
      {required}
      {disabled}
      placeholder={pattern.toUpperCase()}
      aria-describedby={`${id}-format`}
      bind:value={() => displayDate(value, pattern), edit}
      onblur={commit}
      onkeydown={(event) => {
        if (event.key === "Enter") commit();
      }}
      {@attach (element) => {
        input = element;
        validateInput();
        return () => {
          input = undefined;
        };
      }}
    /><button
      type="button"
      {disabled}
      aria-label={t("Choose date")}
      aria-expanded={open}
      aria-controls={`${id}-calendar`}
      onclick={toggle}
      ><svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        ><rect x="3" y="5" width="18" height="16" rx="2" /><path
          d="M7 2v6m10-6v6M3 11h18"
        /></svg
      ></button
    >
  </div>
  <p class="hint" id={`${id}-format`}>
    {t("Format: {{format}}", { format: pattern.toUpperCase() })}
  </p>
  {#if open}<div
      class="calendar"
      id={`${id}-calendar`}
      {@attach (element) => {
        calendar = element;
        return () => {
          calendar = undefined;
        };
      }}
    >
      <div class="calendar-heading">
        <button
          type="button"
          aria-label={t("Previous month")}
          disabled={month === "0001-01"}
          onclick={() => pageMonth(-1)}
          ><svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"><path d="m15 18-6-6 6-6" /></svg
          ></button
        ><strong aria-live="polite">{monthLabel()}</strong><button
          type="button"
          aria-label={t("Next month")}
          disabled={month === "9999-12"}
          onclick={() => pageMonth(1)}
          ><svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            aria-hidden="true"><path d="m9 18 6-6-6-6" /></svg
          ></button
        >
      </div>
      <div role="grid" aria-label={monthLabel()}>
        <div role="row" class="calendar-week">
          {#each days as day (day)}<span role="columnheader"
              >{weekday(day)}</span
            >{/each}
        </div>
        {#each weeks as week, index (index)}<div
            role="row"
            class="calendar-week"
          >
            {#each week as date, index (index)}{#if date}<button
                  type="button"
                  role="gridcell"
                  data-date={date}
                  aria-label={dateLabel(date)}
                  aria-selected={date === value}
                  aria-current={date === today() ? "date" : undefined}
                  tabindex={date === cursor ? 0 : -1}
                  class={{ outside: date.slice(0, 7) !== month }}
                  onclick={() => choose(date)}
                  onkeydown={(event) => keydown(event, date)}
                  >{Number(date.slice(-2))}</button
                >{:else}<span role="gridcell" aria-disabled="true"
                ></span>{/if}{/each}
          </div>{/each}
      </div>
      <button class="text-button" type="button" onclick={() => choose(today())}
        >{t("Today")}</button
      >
    </div>{/if}
</div>
