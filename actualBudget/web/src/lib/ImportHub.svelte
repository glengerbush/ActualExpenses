<script lang="ts">
  import { untrack } from "svelte";
  import type { Account, Budget } from "./api";
  import type { ImportSource } from "./imports-api";
  import BudgetForm from "./BudgetForm.svelte";
  import ImportBudget from "./ImportBudget.svelte";
  import TransactionImport from "./TransactionImport.svelte";
  let {
    budgets,
    budget,
    accounts,
    initialSource = "actual",
    transactionsReady = false,
    disconnected,
    onstart,
    oncreated,
    onclose,
    onchanged,
  }: {
    budgets: Budget[];
    budget: Budget | null;
    accounts: Account[];
    initialSource?: ImportSource;
    transactionsReady?: boolean;
    disconnected: boolean;
    onstart: (budget: Budget) => Promise<void>;
    oncreated: (budget: Budget) => Promise<void>;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  // Entry state is intentionally captured: changing destinations remounts this workspace.
  let kind = $state<"budget" | "transactions">(
    untrack(() => (transactionsReady ? "transactions" : "budget")),
  );
  let destination = $state(untrack(() => (budget ? "existing" : "new")));
  let selected = $state(untrack(() => budget?.id ?? budgets[0]?.id ?? ""));
  let creating = $state(false);
</script>

<section aria-labelledby="import-hub-title">
  <h1 id="import-hub-title">Import</h1>
  <div class="import-kinds" role="group" aria-label="Import file type">
    <button
      aria-pressed={kind === "budget"}
      onclick={() => {
        kind = "budget";
        creating = false;
      }}>Budget export</button
    >
    <button
      aria-pressed={kind === "transactions"}
      onclick={() => {
        kind = "transactions";
        creating = false;
      }}>Transaction file</button
    >
  </div>
  {#if kind === "budget"}
    <ImportBudget
      {budgets}
      {initialSource}
      {oncreated}
      oncancel={onclose}
      embedded
    />
  {:else if transactionsReady && budget}
    <p class="destination">Importing into <strong>{budget.name}</strong>.</p>
    <TransactionImport budgetId={budget.id} {accounts} {onchanged} {onclose} />
  {:else if creating}
    <BudgetForm
      oncreated={(value) => void onstart(value)}
      oncancel={() => (creating = false)}
    />
  {:else}
    <p>CSV, TSV, OFX, QFX, QIF, or XML.</p>
    <fieldset class="destination-options" disabled={disconnected}>
      <legend>Import transactions into</legend>
      <label
        ><input type="radio" bind:group={destination} value="new" /> A new budget</label
      >
      <label
        ><input
          type="radio"
          bind:group={destination}
          value="existing"
          disabled={!budgets.length}
        /> An existing budget</label
      >
      {#if destination === "existing"}<label for="transaction-import-budget"
          >Budget</label
        >
        <select id="transaction-import-budget" bind:value={selected}>
          {#each budgets as value (value.id)}<option value={value.id}
              >{value.name}</option
            >{/each}
        </select>
      {/if}
    </fieldset>
    <button
      class="primary"
      disabled={disconnected || (destination === "existing" && !selected)}
      onclick={() => {
        if (destination === "new") creating = true;
        else {
          const target = budgets.find((b) => b.id === selected);
          if (target) void onstart(target);
        }
      }}
    >
      {destination === "new"
        ? "Name new budget"
        : "Continue to transaction import"}
    </button>
    {#if disconnected}<p role="status">Reconnect to import a file.</p>{/if}
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
  .destination-options {
    max-width: 32rem;
    display: grid;
    gap: 0.75rem;
    padding: 0;
    border: 0;
    margin: 1.5rem 0;
  }
  .destination-options label:has(input) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
  }
  .destination-options input {
    width: auto;
  }
  .destination {
    margin-top: 1.5rem;
  }
</style>
