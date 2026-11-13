<script lang="ts">
  let {
    label,
    disabled = false,
    onstart,
    onmove,
    onend,
    oncancel,
    onstep,
  }: {
    label: string;
    disabled?: boolean;
    onstart: () => void;
    onmove: (x: number, y: number) => void;
    onend: () => void;
    oncancel: () => void;
    onstep: (direction: number) => void;
  } = $props();
  let origin: { x: number; y: number; id: number } | null = null;
  let moving = $state(false);
  function cancel() {
    origin = null;
    moving = false;
    oncancel();
  }
</script>

<button
  type="button"
  class="reorder-handle"
  class:moving
  {disabled}
  aria-label={label}
  title="Drag to reorder, or use ↑ / ↓"
  onpointerdown={(e) => {
    if (e.button !== 0 || disabled) return;
    origin = { x: e.clientX, y: e.clientY, id: e.pointerId };
    e.currentTarget.setPointerCapture(e.pointerId);
  }}
  onpointermove={(e) => {
    if (!origin) return;
    if (!moving && Math.hypot(e.clientX - origin.x, e.clientY - origin.y) < 5)
      return;
    if (!moving) {
      moving = true;
      onstart();
    }
    onmove(e.clientX, e.clientY);
  }}
  onpointerup={(e) => {
    const dropped = moving;
    origin = null;
    moving = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
    if (dropped) onend();
  }}
  onpointercancel={cancel}
  onlostpointercapture={() => {
    if (origin) cancel();
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      onstep(e.key === "ArrowUp" ? -1 : 1);
    }
  }}
  ><svg
    aria-hidden="true"
    width="16"
    height="20"
    viewBox="0 0 16 20"
    fill="currentColor"
  >
    <circle cx="5" cy="5" r="1.2" /><circle cx="11" cy="5" r="1.2" />
    <circle cx="5" cy="10" r="1.2" /><circle cx="11" cy="10" r="1.2" />
    <circle cx="5" cy="15" r="1.2" /><circle cx="11" cy="15" r="1.2" />
  </svg></button
>

<style>
  .reorder-handle {
    display: grid;
    place-items: center;
    flex: 0 0 28px;
    width: 28px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    opacity: 0.65;
    cursor: grab;
    touch-action: none;
  }
  .reorder-handle:hover,
  .reorder-handle:focus-visible {
    opacity: 1;
    background: var(--hover);
  }
  .reorder-handle.moving {
    cursor: grabbing;
    opacity: 1;
  }
  .reorder-handle:disabled {
    cursor: default;
    opacity: 0.3;
  }
</style>
