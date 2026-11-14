<script lang="ts">
  let {
    name,
  }: {
    name:
      | "budget"
      | "transactions"
      | "reports"
      | "payees"
      | "rules"
      | "manage"
      | "settings"
      | "chevron";
  } = $props();
  const paths = {
    budget: "m3 7 9-4 9 4-9 4-9-4Zm0 5 9 4 9-4M3 17l9 4 9-4",
    transactions: "M4 7h16m-4-4 4 4-4 4M20 17H4m4-4-4 4 4 4",
    reports: "M4 20V10h4v10m4 0V4h4v16m4 0v-7h2v7M2 20h21",
    payees:
      "M16 21v-3a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v3m20 0v-3a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75",
    rules: "M4 6h16M4 12h16M4 18h16M8 3v6m8 0v6m-8 0v6",
    manage: "M4 7h16v14H4V7Zm-1-4h18v4H3V3Zm6 8h6",
    settings:
      "m9 3-1 3-3 1-2 4 2 2-1 3 3 3 3-1 2 2 4-2 1-3 3-1v-4l-3-1-1-3-4-2-2 2-3-1Z",
    chevron: "m6 9 6 6 6-6",
  };
</script>

<svg
  aria-hidden="true"
  width="22"
  height="22"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d={paths[name]} />
  {#if name === "payees"}<circle cx="9" cy="6" r="4" />{/if}
  {#if name === "settings"}<circle cx="12" cy="12" r="3" />{/if}
</svg>

<style>
  svg {
    flex: 0 0 22px;
  }
</style>
