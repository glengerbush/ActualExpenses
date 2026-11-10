<script lang="ts">
  import { formatAmount } from "./money";
  import type { PlanCategory } from "./planning";
  let { category }: { category: PlanCategory } = $props();
  function read(value: string | null | undefined) {
    try {
      const n = value == null ? null : JSON.parse(value);
      return typeof n === "number" && Number.isFinite(n) ? n : null;
    } catch {
      return null;
    }
  }
  const goal = $derived(read(category.goal));
  const long = $derived(read(category.long_goal) === 1);
  const basis = $derived(long ? category.balance : category.budgeted);
  const remaining = $derived(goal === null ? 0 : goal - basis);
</script>

{#if goal !== null}<span class="goal-progress"
    ><span>{long ? "Goal" : "Automation"}: {formatAmount(goal)}</span><span
      >{remaining > 0
        ? `${formatAmount(remaining)} to ${long ? "goal" : "fund"}`
        : remaining < 0
          ? `${formatAmount(-remaining)} above ${long ? "goal" : "plan"}`
          : "Fully funded"}</span
    ></span
  >{/if}

<style>
  .goal-progress {
    display: block;
    margin-top: 0.4rem;
    font-size: 0.875em;
    line-height: 1.5;
    white-space: normal;
    min-width: 8rem;
  }
  .goal-progress span {
    display: block;
  }
</style>
