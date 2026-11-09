<script lang="ts">
  import { onMount } from "svelte";
  import { type Account, type AccountGroup, errorMessage } from "./api";
  import { sortedAccounts, sortedGroups } from "./account-actions";
  import { formatAmount } from "./money";
  import { preferences } from "./preferences.svelte";
  import { t } from "./i18n.svelte";
  let {
    budgetId,
    accounts,
    groups,
    selectedId,
    disabled,
    onselect,
  }: {
    budgetId: string;
    accounts: Account[];
    groups: AccountGroup[];
    selectedId: string | null;
    disabled: boolean;
    onselect: (a: Account | null) => void;
  } = $props();
  let showClosed = $state(false),
    opened = $state<Record<string, boolean>>({}),
    error = $state("");
  const key = () => `actualBudget.accountVisibility.v1.${budgetId}`;
  let ordered = $derived(sortedAccounts(accounts));
  function persist() {
    try {
      localStorage.setItem(key(), JSON.stringify({ showClosed, opened }));
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function visibility(value: boolean) {
    showClosed = value;
    persist();
  }
  function toggle(id: string, value: boolean) {
    if ((opened[id] ?? true) === value) return;
    opened = { ...opened, [id]: value };
    persist();
  }
  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(key()) ?? "{}");
      if (typeof saved.showClosed === "boolean") showClosed = saved.showClosed;
      if (
        saved.opened &&
        Object.values(saved.opened).every((v) => typeof v === "boolean")
      )
        opened = saved.opened;
    } catch (e) {
      error = errorMessage(e);
    }
  });
</script>

<nav class="account-navigation" aria-label={t("Accounts")}>
  <button
    class={{ current: selectedId === null }}
    aria-current={selectedId === null ? "page" : undefined}
    onclick={() => onselect(null)}
    {disabled}>{t("All accounts")}</button
  >
  {#each [false, true] as off (off)}
    {@const side = off ? "off" : "on"}
    {#if ordered.some((a) => a.off_budget === off && !a.closed)}
      <p class="section-label">{t(off ? "Off budget" : "On budget")}</p>
      {#each [null, ...sortedGroups(groups)] as group (group?.id ?? "ungrouped")}
        {@const members = ordered.filter(
          (a) =>
            !a.closed &&
            a.off_budget === off &&
            (a.group_id ?? null) === (group?.id ?? null),
        )}
        {#if members.length}
          {#if group}<details
              open={opened[`group-${group.id}-${side}`] ?? true}
              ontoggle={(e) =>
                toggle(`group-${group.id}-${side}`, e.currentTarget.open)}
            >
              <summary>{group.name}</summary
              >{#each members as item (item.id)}{@render accountButton(
                  item,
                )}{/each}
            </details>
          {:else}{#each members as item (item.id)}{@render accountButton(
                item,
              )}{/each}{/if}
        {/if}
      {/each}
    {/if}
  {/each}
  {#if accounts.some((a) => a.closed)}<label class="closed-toggle"
      ><input
        type="checkbox"
        checked={showClosed}
        onchange={(e) => visibility(e.currentTarget.checked)}
      />{t("Show closed accounts")}</label
    >
    {#if showClosed}{#each ordered.filter((a) => a.closed) as item (item.id)}{@render accountButton(
          item,
        )}{/each}{/if}
  {/if}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
</nav>
{#snippet accountButton(item: Account)}<button
    class={{ current: selectedId === item.id }}
    aria-current={selectedId === item.id ? "page" : undefined}
    onclick={() => onselect(item)}
    {disabled}
    ><span>{item.name}{item.closed ? " — " + t("Closed") : ""}</span><span
      class="nav-amount"
      >{preferences.hasUsableShared ? formatAmount(item.balance) : "—"}</span
    ></button
  >{/snippet}

<style>
  nav.account-navigation {
    flex-direction: column;
    align-items: stretch;
  }
  nav.account-navigation button {
    max-width: none;
    width: 100%;
  }
  details {
    min-width: 0;
  }
  .section-label {
    font-size: 0.875rem;
    font-weight: 600;
    margin: 1.5rem 0 0.5rem;
  }
  summary {
    cursor: pointer;
    min-height: 44px;
    padding: 0.75rem 0;
    overflow-wrap: anywhere;
  }
  .closed-toggle {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 44px;
    margin-top: 1rem;
  }
  .closed-toggle input {
    width: auto;
  }
  button span {
    overflow-wrap: anywhere;
    min-width: 0;
  }
</style>
