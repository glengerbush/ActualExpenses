<script lang="ts">
  import type { Account } from "./api";
  import Register from "./Register.svelte";
  import { t } from "./i18n.svelte";
  let {
    account,
    budgetId,
    onchanged,
    onorganize,
  }: {
    account: Account;
    budgetId: string;
    onchanged: () => Promise<void>;
    onorganize: () => void;
  } = $props();
</script>

<section aria-labelledby="account-title">
  <div class="page-heading">
    <h1 id="account-title">{account.name}</h1>
    <button onclick={onorganize}>{t("Organize accounts")}</button>
  </div>
  {#if account.closed}<p role="status">
      {t("Closed account. History is retained.")}
    </p>{/if}
  <p class="muted">{account.off_budget ? t("Off budget") : t("On budget")}</p>
  {#if account.notes}<dl class="account-summary">
      <div>
        <dt>{t("Notes")}</dt>
        <dd class="notes">{account.notes}</dd>
      </div>
    </dl>{/if}
</section>
{#key budgetId + ":" + account.id}<Register
    {budgetId}
    accountId={account.id}
    {onchanged}
  />{/key}
