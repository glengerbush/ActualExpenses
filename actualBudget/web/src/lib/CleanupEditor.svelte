<script lang="ts">
  type Row = Record<string, unknown>;
  let {
    rows,
    pools,
    disabled = false,
    onchange,
  }: {
    rows: Row[];
    pools: { id: string; name: string }[];
    disabled?: boolean;
    onchange: (rows: Row[]) => void;
  } = $props();
  function field(index: number, key: string, value: unknown) {
    const next = structuredClone(rows);
    next[index][key] = value;
    onchange(next);
  }
  function role(index: number, value: string) {
    const next = structuredClone(rows);
    next[index] = {
      role: value,
      groupId: value === "overspend" ? (pools[0]?.id ?? null) : null,
      ...(value === "sink" ? { weight: 1 } : {}),
    };
    onchange(next);
  }
</script>

<fieldset {disabled}>
  <legend>Month-end cleanup</legend>
  <p>
    Sources release available balances. Sinks receive pooled funds by weight. An
    overspending pool covers deficits before its sinks. Create named pools
    before opening this editor.
  </p>
  {#each rows as row, index (index)}<div class="cleanup-row">
      <label
        >Cleanup role<select
          value={String(row.role)}
          onchange={(e) => role(index, e.currentTarget.value)}
          ><option value="source">Source</option><option value="sink"
            >Sink</option
          ><option value="overspend">Cover overspending</option></select
        ></label
      ><label
        >Pool<select
          value={String(row.groupId ?? "")}
          onchange={(e) =>
            field(index, "groupId", e.currentTarget.value || null)}
          >{#if row.role !== "overspend"}<option value=""
              >Global available funds</option
            >{:else}<option value="">Choose a named pool</option
            >{/if}{#each pools as pool (pool.id)}<option value={pool.id}
              >{pool.name}</option
            >{/each}</select
        ></label
      >{#if row.role === "sink"}<label
          >Share weight<input
            type="text"
            inputmode="numeric"
            min="1"
            step="1"
            value={String(row.weight ?? 1)}
            oninput={(e) => field(index, "weight", e.currentTarget.value)}
          /></label
        >{/if}<button
        onclick={() => onchange(rows.filter((_, i) => i !== index))}
        >Remove cleanup role</button
      >
    </div>{/each}
  <button
    onclick={() =>
      onchange([...structuredClone(rows), { role: "source", groupId: null }])}
    disabled={rows.length >= 256}>Add cleanup role</button
  >
</fieldset>

<style>
  fieldset {
    border: 0;
    padding: 0;
    min-width: 0;
  }
  .cleanup-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: end;
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: 1rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    flex: 1 1 12rem;
    min-width: 0;
  }
  input,
  select {
    width: 100%;
    box-sizing: border-box;
  }
  button {
    min-height: 44px;
  }
</style>
