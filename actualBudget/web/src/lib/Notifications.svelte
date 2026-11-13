<script lang="ts">
  let {
    notice = $bindable(""),
    error = $bindable(""),
  }: {
    notice?: string;
    error?: string;
  } = $props();
  let paused = $state(false);

  $effect(() => {
    if (!notice || paused) return;
    const timer = setTimeout(() => (notice = ""), 5000);
    return () => clearTimeout(timer);
  });
</script>

<div class="notifications">
  <div role="status" aria-atomic="true">
    {#if notice}
      <div class="toast success">
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"><path d="m5 12 4 4L19 6" /></svg
        >
        <span>{notice}</span>
        <button
          aria-label="Dismiss notification"
          title="Dismiss notification"
          onpointerenter={() => (paused = true)}
          onpointerleave={() => (paused = false)}
          onfocus={() => (paused = true)}
          onblur={() => (paused = false)}
          onclick={() => {
            notice = "";
            paused = false;
          }}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"><path d="m6 6 12 12M18 6 6 18" /></svg
          >
        </button>
      </div>
    {/if}
  </div>
  <div role="alert" aria-atomic="true">
    {#if error}
      <div class="toast failure">
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.75"
          ><circle cx="12" cy="12" r="9" /><path d="M12 7v6m0 3v1" /></svg
        >
        <span>{error}</span>
        <button
          aria-label="Dismiss error"
          title="Dismiss error"
          onclick={() => (error = "")}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"><path d="m6 6 12 12M18 6 6 18" /></svg
          >
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .notifications {
    position: fixed;
    bottom: max(1rem, env(safe-area-inset-bottom));
    right: max(1rem, env(safe-area-inset-right));
    z-index: 100;
    width: min(28rem, calc(100vw - 2rem));
    display: grid;
    gap: 0.5rem;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem 0.5rem 1rem;
    border-radius: 10px;
    border: 1px solid currentColor;
    pointer-events: auto;
    overflow-wrap: anywhere;
  }
  .success {
    color: var(--selected-text);
    background: var(--selected);
  }
  .failure {
    color: var(--error);
    background: var(--error-bg);
  }
  span {
    flex: 1;
    min-width: 0;
  }
  svg {
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }
  button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    padding: 0;
    border: 0;
    color: inherit;
    background: transparent;
  }
  button:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }
</style>
