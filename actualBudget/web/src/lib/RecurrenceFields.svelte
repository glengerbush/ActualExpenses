<script lang="ts">
  import type { Recurrence } from "./recurrence";
  import DateInput from "./DateInput.svelte";
  let {
    value,
    compact = false,
    onchange,
  }: {
    compact?: boolean;
    value: Recurrence;
    onchange: (value: Recurrence) => void;
  } = $props();
  function config(field: string, next: unknown) {
    onchange({ ...value, config: { ...value.config, [field]: next } });
  }
</script>

<div class="recurrence" class:compact>
  <label class="recurring-toggle">
    <input
      type="checkbox"
      role="switch"
      checked={value.enabled}
      onchange={(e) => onchange({ ...value, enabled: e.currentTarget.checked })}
    />
    <span class="track" aria-hidden="true"></span>
    <span>Recurring</span>
  </label>
  {#if value.enabled}
    <div class="recurrence-options">
      <label
        >Repeat every
        <input
          aria-label="Repeat interval"
          type="number"
          min="1"
          max="10000"
          required
          value={value.config.interval ?? 1}
          oninput={(e) => config("interval", e.currentTarget.valueAsNumber)}
        />
      </label>
      <label
        >Frequency
        <select
          value={value.config.frequency}
          onchange={(e) => config("frequency", e.currentTarget.value)}
        >
          <option value="daily">Day</option><option value="weekly">Week</option
          ><option value="monthly">Month</option><option value="yearly"
            >Year</option
          >
        </select>
      </label>
      <DateInput
        id="recurrence-start"
        label="Starts"
        bind:value={() => value.config.start, (v) => config("start", v)}
      />
      <label
        >Ends
        <select
          value={value.config.endMode ?? "never"}
          onchange={(e) => config("endMode", e.currentTarget.value)}
        >
          <option value="never">Never</option><option value="on_date"
            >On a date</option
          ><option value="after_n_occurrences">After count</option>
        </select>
      </label>
      {#if value.config.endMode === "on_date"}
        <DateInput
          id="recurrence-end"
          label="End date"
          bind:value={
            () => value.config.endDate ?? "", (v) => config("endDate", v)
          }
        />
      {:else if value.config.endMode === "after_n_occurrences"}
        <label
          >Occurrences<input
            type="number"
            min="1"
            max="120000"
            required
            value={value.config.endOccurrences ?? 1}
            oninput={(e) =>
              config("endOccurrences", e.currentTarget.valueAsNumber)}
          /></label
        >
      {/if}
    </div>
    <p class="hint">
      Future entries repeat this amount, payee and category. Changes apply to
      future repeats.
    </p>
  {/if}
</div>

<style>
  .recurrence.compact {
    margin: 0;
    padding: 0;
    border: 0;
  }
  .compact .recurring-toggle {
    margin: 0;
    gap: 0.4rem;
  }
  .compact:has(.recurrence-options) {
    grid-column: 1 / -1;
  }

  .recurrence {
    margin-top: 1rem;
    border-top: 1px solid var(--line);
    padding-top: 0.5rem;
  }
  .recurring-toggle {
    display: inline-flex;
    position: relative;
    align-items: center;
    gap: 0.65rem;
    min-height: 44px;
    cursor: pointer;
  }
  .recurring-toggle input {
    position: absolute;
    width: 100%;
    height: 100%;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }
  .track {
    width: 40px;
    height: 24px;
    border-radius: 16px;
    background: var(--muted);
    padding: 3px;
    box-sizing: border-box;
  }
  .track::after {
    content: "";
    display: block;
    height: 18px;
    width: 18px;
    border-radius: 50%;
    background: var(--surface, white);
    transition: transform 140ms ease-out;
  }
  input:checked + .track {
    background: var(--accent);
  }
  input:checked + .track::after {
    transform: translateX(16px);
  }
  input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  input:disabled + .track {
    opacity: 0.5;
  }
  .recurrence-options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.75rem;
    margin-top: 0.5rem;
  }
  .recurrence-options label {
    display: grid;
    align-content: start;
    gap: 0.25rem;
    min-width: 0;
  }
  input,
  select {
    width: 100%;
    min-width: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .track::after {
      transition: none;
    }
  }
</style>
