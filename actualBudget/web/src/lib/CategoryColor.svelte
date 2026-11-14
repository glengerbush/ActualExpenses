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
    compact = false,
    disabled = false,
    onchanged,
  }: {
    budgetId: string;
    id: string;
    kind: "categories" | "category-groups";
    name: string;
    color: string;
    compact?: boolean;
    disabled?: boolean;
    onchanged: () => Promise<void>;
  } = $props();
  let busy = $state(false),
    error = $state("");
  let pending = $state.raw<PendingMutation | null>(null);
  let preview = $state<string | null>(null);
  let queuedColor: string | undefined;
  function changeColor(value: string) {
    preview = value;
    // Safari emits change while its native picker is open; keep the input enabled
    // and serialize writes, coalescing intermediate slider positions.
    if (busy) {
      queuedColor = value;
      return;
    }
    void save(value);
  }
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
        queuedColor = undefined;
        preview = null;
        pending = { ...pending, cancelRequested: true };
        localStorage.setItem(key, JSON.stringify(pending));
      }
      const result = await mutate(pending, pending.cancelRequested);
      localStorage.removeItem(key);
      pending = null;
      await onchanged();
      if (result.conflicts?.length) {
        queuedColor = undefined;
        preview = null;
        error = "A newer color was kept. Review it before changing it again.";
      }
    } catch (e) {
      if (e instanceof RequestError && e.code === "operation_canceled") {
        localStorage.removeItem(key);
        pending = null;
      } else error = errorMessage(e);
    } finally {
      busy = false;
      if (!pending && queuedColor !== undefined) {
        const next = queuedColor;
        queuedColor = undefined;
        void save(next);
      } else if (!pending) preview = null;
    }
  }
</script>

<span class="color-control" class:compact>
  <input
    type="color"
    value={preview ?? color}
    aria-label={`Color for ${name}`}
    title={`Color for ${name}`}
    disabled={disabled || (!!pending && !busy)}
    onchange={(e) => changeColor(e.currentTarget.value)}
  />
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
  [role="alert"] {
    max-width: 24rem;
    font-size: 0.85rem;
    font-weight: 400;
    color: var(--error-text);
  }
</style>
