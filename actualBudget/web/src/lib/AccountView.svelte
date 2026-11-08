<script lang="ts">
  import type { Account } from "./api";
  import { formatAmount } from "./money";
  let { account }: { account: Account } = $props();
</script>

<section aria-labelledby="account-title">
  <h1 id="account-title">{account.name}</h1>
  <p class="muted">{account.off_budget ? "Off budget" : "On budget"} account</p>
  <dl class="account-summary">
    <div>
      <dt>Current balance</dt>
      <dd data-testid="account-balance">{formatAmount(account.balance)}</dd>
    </div>
    {#if account.notes}<div>
        <dt>Notes</dt>
        <dd class="notes">{account.notes}</dd>
      </div>{/if}
  </dl>
</section>
