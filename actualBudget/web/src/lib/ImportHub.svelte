<script lang="ts">
  import type { Account, Budget } from "./api";
  import type { ImportSource } from "./imports-api";
  import ImportBudget from "./ImportBudget.svelte";
  import TransactionImport from "./TransactionImport.svelte";
  let {
    budget,
    accounts,
    initialSource = "actual",
    transactionsReady = false,
    onkindchange,
    oncreated,
    onclose,
    onchanged,
  }: {
    budget: Budget;
    accounts: Account[];
    initialSource?: ImportSource;
    transactionsReady?: boolean;
    onkindchange: (kind: "budget" | "transactions") => void;
    oncreated: (budget: Budget) => Promise<void>;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  const kind = $derived(transactionsReady ? "transactions" : "budget");
</script>

<section aria-labelledby="import-hub-title">
  <h1 id="import-hub-title">Import</h1>
  <div class="import-kinds" role="group" aria-label="Import file type">
    <button
      aria-pressed={kind === "budget"}
      onclick={() => onkindchange("budget")}>Budget export</button
    >
    <button
      aria-pressed={kind === "transactions"}
      onclick={() => onkindchange("transactions")}>Transaction file</button
    >
  </div>
  {#if kind === "budget"}
    <ImportBudget
      {budget}
      {initialSource}
      {oncreated}
      oncancel={onclose}
      embedded
    />
  {:else}
    <TransactionImport budgetId={budget.id} {accounts} {onchanged} {onclose} />
  {/if}
</section>

<style>
  .import-kinds {
    display: flex;
    gap: 0.5rem;
    margin: 1rem 0 1.5rem;
  }
  .import-kinds button[aria-pressed="true"] {
    background: var(--hover);
    border-color: var(--line);
    font-weight: 650;
  }
</style>
