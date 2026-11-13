<script lang="ts">
  import { onMount } from "svelte";
  import { preferences } from "./preferences.svelte";
  import { mutate, type PendingMutation } from "./register-api";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import { nextEditClock } from "./edit-clock";
  import { historyOwner } from "./history-owner";
  let {
    budgetId,
    id,
    kind,
    name,
    color,
    automatic = false,
    compact = false,
    disabled = false,
    onchanged,
  }: {
    budgetId: string;
    id: string;
    kind: "categories" | "category-groups";
    name: string;
    color: string;
    automatic?: boolean;
    compact?: boolean;
    disabled?: boolean;
    onchanged: () => Promise<void>;
  } = $props();
  let busy = $state(false),
    error = $state("");
  let pending = $state.raw<PendingMutation | null>(null);
  const key = $derived(`actualBudget.color.v1.${budgetId}.${kind}.${id}`);
  onMount(() => {
    try {
      const raw = localStorage.getItem(key);
      if (raw) pending = JSON.parse(raw);
    } catch (e) {
      error = errorMessage(e);
    }
  });
  async function save(value: string | null, cancel = false) {
    if (disabled || busy) return;
    busy = true;
    error = "";
    try {
      if (!pending) {
        if (localStorage.getItem(key))
          throw new Error(
            "Another view has a saved color change. Reload to recover it.",
          );
        const record = await request<{
          data: { field_versions: Record<string, string> };
        }>(`/budgets/${budgetId}/${kind}/${id}`);
        pending = {
          path: `/budgets/${budgetId}/${kind}/${id}`,
          method: "PATCH",
          key: uuid(),
          body: { color: value },
          budgetEpoch: preferences.budgetEpoch ?? undefined,
          edit_clock: nextEditClock(record.data.field_versions),
          historyOwner: await historyOwner(),
        };
        try {
          localStorage.setItem(key, JSON.stringify(pending));
        } catch (e) {
          pending = null;
          throw e;
        }
      }
      if (cancel && pending) {
        pending = { ...pending, cancelRequested: true };
        localStorage.setItem(key, JSON.stringify(pending));
      }
      const result = await mutate(pending, pending.cancelRequested);
      localStorage.removeItem(key);
      pending = null;
      await onchanged();
      if (result.conflicts?.length)
        error = "A newer color was kept. Review it before changing it again.";
    } catch (e) {
      if (e instanceof RequestError && e.code === "operation_canceled") {
        localStorage.removeItem(key);
        pending = null;
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<span class="color-control" class:compact>
  <input
    type="color"
    value={color}
    aria-label={`Color for ${name}`}
    title={`Color for ${name}`}
    disabled={disabled || busy || !!pending}
    onchange={(e) => save(e.currentTarget.value)}
  />
  {#if kind === "categories" && !automatic}<button
      class="reset-color"
      title="Use automatic shade"
      aria-label={`Use automatic shade for ${name}`}
      onclick={() => save(null)}
      disabled={disabled || busy || !!pending}
      ><svg
        aria-hidden="true"
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"><path d="M4 10a8 8 0 1 1 1 8M4 4v6h6" /></svg
      ></button
    >{/if}
  {#if pending}<button onclick={() => save(null)} disabled={disabled || busy}
      >Retry color</button
    ><button onclick={() => save(null, true)} disabled={disabled || busy}
      >Cancel color change</button
    >{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</span>

<style>
  .color-control {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.25rem;
    vertical-align: middle;
  }
  input[type="color"] {
    width: 44px;
    height: 44px;
    min-height: 44px;
    padding: 10px;
    border: 0;
    background: transparent;
    cursor: pointer;
  }
  .compact input[type="color"] {
    width: 32px;
    padding: 13px 7px;
  }
  @media (pointer: coarse) {
    .compact input[type="color"] {
      width: 44px;
      padding: 13px;
    }
  }
  input::-webkit-color-swatch-wrapper {
    padding: 0;
  }
  input::-webkit-color-swatch {
    border: 1px solid var(--line);
    border-radius: 50%;
  }
  input::-moz-color-swatch {
    border: 1px solid var(--line);
    border-radius: 50%;
  }
  .reset-color {
    display: grid;
    place-items: center;
    width: 44px;
    min-height: 44px;
    padding: 0;
    background: transparent;
    border: 0;
  }
  [role="alert"] {
    max-width: 24rem;
    font-size: 0.85rem;
    font-weight: 400;
    color: var(--error-text);
  }
</style>
