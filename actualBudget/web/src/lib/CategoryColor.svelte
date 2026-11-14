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
  const paletteId = $props.id();
  let open = $state(false);
  let trigger: HTMLButtonElement | undefined;
  let palette: HTMLElement | undefined;
  const hues = [
    { name: "Plum", color: "#8a3e89" },
    { name: "Indigo", color: "#6846bd" },
    { name: "Blue", color: "#3182d5" },
    { name: "Teal", color: "#169d98" },
    { name: "Green", color: "#4d923e" },
    { name: "Amber", color: "#d68b13" },
  ];
  const currentColor = $derived(preview ?? color);
  const shades = $derived.by(() => {
    const channels = [1, 3, 5].map((offset) =>
      Number.parseInt(currentColor.slice(offset, offset + 2), 16),
    );
    return [-0.5, -0.25, 0, 0.25, 0.5, 0.75].map((amount) => {
      const target = amount < 0 ? 0 : 255;
      return (
        "#" +
        channels
          .map((channel) =>
            Math.round(channel + (target - channel) * Math.abs(amount))
              .toString(16)
              .padStart(2, "0"),
          )
          .join("")
      );
    });
  });
  function positionPalette(element: HTMLElement) {
    if (!trigger) return;
    const anchor = trigger.getBoundingClientRect();
    const gap = 8;
    const width = Math.min(288, window.innerWidth - gap * 2);
    element.style.width = `${width}px`;
    element.style.left = `${Math.max(gap, Math.min(anchor.left, window.innerWidth - width - gap))}px`;
    const height = element.offsetHeight;
    const below = anchor.bottom + gap;
    element.style.top = `${Math.max(gap, below + height <= window.innerHeight - gap ? below : anchor.top - height - gap)}px`;
  }
  function attachPalette(element: HTMLElement) {
    palette = element;
    const reposition = () => positionPalette(element);
    const toggle = (event: ToggleEvent) => {
      open = event.newState === "open";
      if (open) {
        reposition();
        document.addEventListener("scroll", reposition, true);
        window.addEventListener("resize", reposition);
      } else {
        document.removeEventListener("scroll", reposition, true);
        window.removeEventListener("resize", reposition);
      }
    };
    element.addEventListener("toggle", toggle);
    return () => {
      palette = undefined;
      element.removeEventListener("toggle", toggle);
      document.removeEventListener("scroll", reposition, true);
      window.removeEventListener("resize", reposition);
    };
  }
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
      // Failed delivery needs the recovery controls, which sit outside the popup.
      if (pending && palette?.matches(":popover-open")) palette.hidePopover();
      if (!pending && queuedColor !== undefined) {
        const next = queuedColor;
        queuedColor = undefined;
        void save(next);
      } else if (!pending) preview = null;
    }
  }
</script>

<span class="color-control" class:compact>
  <button
    class="color-trigger"
    type="button"
    aria-label={`Choose color for ${name}`}
    title={`Choose color for ${name}`}
    aria-expanded={open}
    aria-controls={paletteId}
    popovertarget={paletteId}
    disabled={disabled || (!!pending && !busy)}
    {@attach (element) => {
      trigger = element;
      return () => {
        trigger = undefined;
      };
    }}><span class="color-dot" style:--swatch={currentColor}></span></button
  >
  <span
    class="color-palette"
    id={paletteId}
    popover="auto"
    role="dialog"
    aria-label={`Color palette for ${name}`}
    {@attach attachPalette}
  >
    <span class="swatches" role="group" aria-label="Colors">
      {#each hues as hue (hue.color)}
        <button
          type="button"
          class="swatch"
          aria-label={hue.name}
          title={hue.name}
          aria-pressed={currentColor.toLowerCase() === hue.color}
          disabled={disabled || (!!pending && !busy)}
          onclick={() => changeColor(hue.color)}
          ><span class="color-dot" style:--swatch={hue.color}></span></button
        >
      {/each}
    </span>
    <span class="swatches" role="group" aria-label="Shades">
      {#each shades as shade, index (index)}
        <button
          type="button"
          class="swatch"
          aria-label={`Shade ${index + 1}: ${shade}`}
          title={shade}
          aria-pressed={currentColor.toLowerCase() === shade}
          disabled={disabled || (!!pending && !busy)}
          onclick={() => changeColor(shade)}
          ><span class="color-dot" style:--swatch={shade}></span></button
        >
      {/each}
    </span>
    <label class="custom-color">
      Custom color
      <input
        type="color"
        value={currentColor}
        aria-label={`Color for ${name}`}
        disabled={disabled || (!!pending && !busy)}
        onchange={(e) => changeColor(e.currentTarget.value)}
      />
    </label>
  </span>
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
  .color-trigger,
  .swatch {
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
    width: 40px;
    min-height: 40px;
    padding: 6px;
    border: 0;
    background: transparent;
  }
  .color-trigger {
    width: 44px;
    min-height: 44px;
  }
  .compact .color-trigger {
    width: 32px;
    padding-inline: 6px;
  }
  .color-dot {
    display: block;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--swatch);
    border: 1px solid color-mix(in srgb, var(--text) 18%, transparent);
  }
  .color-palette {
    position: fixed;
    inset: auto;
    margin: 0;
    padding: 12px;
    max-height: calc(100dvh - 16px);
    overflow: auto;
    border: 0;
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 6px 24px color-mix(in srgb, var(--text) 22%, transparent);
  }
  .color-palette:popover-open {
    display: grid;
    gap: 4px;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
  }
  .swatch[aria-pressed="true"] .color-dot {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .custom-color {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 8px 0 0;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    font-size: 0.875rem;
    font-weight: 500;
  }
  input[type="color"] {
    width: 44px;
    height: 36px;
    min-height: 36px;
    padding: 4px;
    cursor: pointer;
  }
  @media (pointer: coarse) {
    .compact .color-trigger {
      width: 44px;
    }
    .swatch {
      width: 100%;
      min-height: 44px;
    }
  }
  input::-webkit-color-swatch-wrapper {
    padding: 0;
  }
  input::-webkit-color-swatch,
  input::-moz-color-swatch {
    border: 0;
    border-radius: 3px;
  }
  [role="alert"] {
    max-width: 24rem;
    font-size: 0.85rem;
    font-weight: 400;
    color: var(--error);
  }
</style>
