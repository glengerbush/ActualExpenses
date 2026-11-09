<script lang="ts">
  import { onMount } from "svelte";
  import { request, errorMessage } from "./api";
  import { formatAmount } from "./money";
  import type { SchedulePage } from "./schedules";
  let {
    budgetId,
    accountId,
    onmanage,
  }: { budgetId: string; accountId: string; onmanage: () => void } = $props();
  let view = $state.raw<SchedulePage | null>(null),
    error = $state("");
  let previews = $derived(
    view?.previews
      .filter(
        (p) =>
          !["completed", "suppressed", "matched", "posted"].includes(p.status),
      )
      .sort((a, b) => a.date.localeCompare(b.date) || a.id.localeCompare(b.id))
      .slice(0, 10) ?? [],
  );
  onMount(() => {
    let active = true;
    request<{ data: SchedulePage }>(
      `/budgets/${budgetId}/schedules?account_id=${accountId}&limit=50`,
    )
      .then((result) => {
        if (active) view = result.data;
      })
      .catch((e) => {
        if (active) error = errorMessage(e);
      });
    return () => {
      active = false;
    };
  });
</script>

{#if previews.length || error || (view?.next_offset !== null && view?.next_offset !== undefined)}<section
    aria-labelledby="scheduled-preview-title"
  >
    <div class="heading">
      <h2 id="scheduled-preview-title">Upcoming scheduled entries</h2>
      <button onclick={onmanage}>Manage schedules</button>
    </div>
    <p>
      Forecast only. These entries are excluded from account balances until
      posted.
    </p>
    {#if error}<p role="status">
        Upcoming entries could not be loaded: {error}
      </p>{/if}
    <ul>
      {#each previews as preview (preview.id)}<li>
          <span
            >{preview.date} · {view?.schedules.find(
              (s) => s.id === preview.schedule_id,
            )?.name ?? "Scheduled transaction"}</span
          ><span
            >{formatAmount(preview.amount)} · {preview.status === "paid"
              ? "upcoming"
              : preview.status}</span
          >
        </li>{/each}
    </ul>
    {#if view && (view.next_offset !== null || view.preview_continuations.length > 0 || view.previews.length > 10)}<p
      >
        Showing up to10 forecast entries from this schedule page. Manage
        schedules to view more schedules and full occurrence calendars.
      </p>{/if}
  </section>{/if}

<style>
  section {
    margin-top: 2rem;
    border-top: 1px solid var(--border);
    padding-top: 1rem;
  }
  .heading {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1rem;
    justify-content: space-between;
  }
  h2 {
    font-size: 1.25rem;
    margin: 0;
  }
  ul {
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 0.5rem 1.5rem;
    padding: 0.7rem 0;
    border-bottom: 1px solid var(--border);
    font-variant-numeric: tabular-nums;
  }
</style>
