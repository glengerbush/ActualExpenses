<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { BankPreferences, BankMapping } from "./banking";
  let {
    value,
    disabled = false,
    onchange,
  }: {
    value: BankPreferences;
    disabled?: boolean;
    onchange: (value: BankPreferences) => void;
  } = $props();
  const flags = [
    ["import_transactions", "Import transactions"],
    ["import_pending", "Include pending transactions"],
    ["import_notes", "Import bank notes"],
    ["update_dates", "Update dates on matched transactions"],
  ] as const;
  const fields = ["date", "payee", "notes"] as const;
  function flag(key: (typeof flags)[number][0], checked: boolean) {
    onchange({ ...value, [key]: checked });
  }
  function mapping(
    kind: "payment" | "deposit",
    field: keyof BankMapping,
    text: string,
  ) {
    onchange({
      ...value,
      mappings: {
        ...value.mappings,
        [kind]: { ...value.mappings[kind], [field]: text },
      },
    });
  }
</script>

<fieldset {disabled}>
  <legend>Refresh preferences</legend>
  <div class="flags">
    {#each flags as [key, label] (key)}
      <label class="check"
        ><input
          type="checkbox"
          checked={value[key]}
          onchange={(e) => flag(key, e.currentTarget.checked)}
        />{t(label)}</label
      >
    {/each}
  </div>
  <p>
    Turning off transaction imports refreshes provider balances only. Pending
    rows keep their original bank identity when they become booked. Leave date
    updates off to preserve dates on matched transactions.
  </p>
  <details>
    <summary>Custom bank fields</summary>
    <p>
      Use the source field names for payments and deposits. Defaults are date,
      payeeName and notes. A missing configured value stops that account's
      refresh.
    </p>
    {#each ["payment", "deposit"] as kind (kind)}
      <fieldset>
        <legend>{t(kind === "payment" ? "Payments" : "Deposits")}</legend>
        <div class="mapping">
          {#each fields as field (field)}
            <label
              >{t(kind === "payment" ? "Payment" : "Deposit")}
              {t(field)} field
              <input
                value={value.mappings[kind as "payment" | "deposit"][field]}
                maxlength="120"
                required
                oninput={(e) =>
                  mapping(
                    kind as "payment" | "deposit",
                    field,
                    e.currentTarget.value,
                  )}
              />
            </label>
          {/each}
        </div>
      </fieldset>
    {/each}
  </details>
</fieldset>

<style>
  fieldset {
    margin: 0;
    padding: 1rem 0;
    border: 0;
    border-top: 1px solid var(--line);
    min-width: 0;
  }
  legend {
    font-weight: 650;
    padding: 0 0.5rem 0 0;
  }
  p {
    color: var(--muted);
    max-width: 70ch;
  }
  .flags {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 18rem), 1fr));
    gap: 0.5rem 1.5rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 44px;
  }
  .check input {
    width: auto;
    flex: none;
  }
  .mapping {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: 1rem;
  }
  .mapping label {
    display: grid;
    gap: 0.4rem;
  }
  summary {
    cursor: pointer;
    min-height: 44px;
    align-content: center;
    font-weight: 600;
  }
</style>
