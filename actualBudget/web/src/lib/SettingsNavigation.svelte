<script lang="ts">
  let {
    active,
    onchange,
  }: {
    active: "general" | "import" | "recovery";
    onchange: (value: "settings" | "import" | "recovery") => void;
  } = $props();
</script>

<nav class="settings-navigation" aria-label="Settings sections">
  {#each [{ id: "general", view: "settings", label: "General" }, { id: "import", view: "import", label: "Import" }, { id: "recovery", view: "recovery", label: "Recovery" }] as item (item.id)}
    <button
      class:current={active === item.id}
      aria-current={active === item.id ? "page" : undefined}
      onclick={() => onchange(item.view as "settings" | "import" | "recovery")}
      >{item.label}</button
    >
  {/each}
</nav>

<style>
  .settings-navigation {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin-bottom: 1.5rem;
    border-bottom: 1px solid var(--line);
    padding-bottom: 0.5rem;
  }
  button {
    background: transparent;
    border: 0;
  }
  button.current {
    background: var(--hover);
    font-weight: 650;
  }
</style>
