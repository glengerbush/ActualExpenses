<script lang="ts">
  import type { CleanupNotification } from "./budget-automations";
  let {
    notification,
    committed = false,
  }: { notification: CleanupNotification; committed?: boolean } = $props();
  const message = $derived(
    notification.message === "cleanup-no-funds"
      ? committed
        ? "Cleanup saved with warnings. Review the affected pools below."
        : "Some cleanup funds or pool connections need attention."
      : notification.message === "cleanup-up-to-date"
        ? "No cleanup allocation changes are needed."
        : notification.message === "cleanup-applied"
          ? committed
            ? "Cleanup reallocated the available funds."
            : "Cleanup will reallocate the available funds."
          : "Review the cleanup result below.",
  );
</script>

<div
  class:warning={notification.type === "warning"}
  role={notification.type === "warning" ? "alert" : "status"}
>
  <p>{message}</p>
  {#if notification.sourceCount !== undefined || notification.sinkCount !== undefined}<p
    >
      Sources: {notification.sourceCount ?? 0}. Sinks: {notification.sinkCount ??
        0}.
    </p>{/if}{#if notification.pre}<p class="details">
      {notification.pre}
    </p>{/if}
</div>

<style>
  .details {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .warning {
    border-left: 3px solid var(--border);
    padding-left: 1rem;
  }
</style>
