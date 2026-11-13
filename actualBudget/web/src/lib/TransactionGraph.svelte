<script lang="ts">
  import { formatAmount } from "./money";
  let {
    rows,
    accounts = [],
    payees = [],
    categories = [],
  }: {
    rows: Record<string, unknown>[];
    accounts?: { id: string; name: string }[];
    payees?: { id: string; name: string }[];
    categories?: { id: string; name: string }[];
  } = $props();
  const fields = [
    ["id", "Entry"],
    ["date", "Date"],
    ["amount", "Amount"],
    ["account_id", "Account"],
    ["payee_id", "Payee"],
    ["category_id", "Category"],
    ["notes", "Notes"],
    ["deleted", "Deleted"],
    ["is_parent", "Split parent"],
    ["parent_id", "Parent"],
    ["transfer_id", "Transfer peer"],
    ["schedule_id", "Schedule"],
    ["imported_id", "Bank identity"],
    ["imported_payee", "Bank payee"],
  ];
  function display(key: string, value: unknown) {
    if (value === null || value === undefined || value === "") return "None";
    if (key === "amount") return formatAmount(Number(value));
    if (typeof value === "boolean") return value ? "Yes" : "No";
    const refs =
      key === "account_id"
        ? accounts
        : key === "payee_id"
          ? payees
          : key === "category_id"
            ? categories
            : [];
    return refs.find((r) => r.id === value)?.name ?? String(value);
  }
</script>

{#each rows as row, i (String(row.id ?? i))}<article>
    <h4>Entry {i + 1}</h4>
    <dl>
      {#each fields as [key, label] (key)}{#if row[key] !== undefined}<div>
            <dt>{label}</dt>
            <dd>{display(key, row[key])}</dd>
          </div>{/if}{/each}
    </dl>
  </article>{/each}

<style>
  article {
    border-top: 1px solid var(--border);
    padding-block: 0.5rem;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
    gap: 0.75rem;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
