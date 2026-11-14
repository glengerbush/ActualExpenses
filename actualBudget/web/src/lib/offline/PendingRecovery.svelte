<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage, request } from "../api";
  import { formatAmountInput } from "../money";
  import { t } from "../i18n.svelte";
  import { readCatalog } from "./cache";
  import {
    entries,
    receipts,
    retryEntry,
    archivedEntries,
    archiveRetiredEntry,
    type OutboxEntry,
  } from "./outbox";
  import { deliverPending, subscribeOffline } from "./delivery";
  import { setReachable, offlineStatus } from "./events";
  let { onclose }: { onclose: () => void } = $props();
  let items = $state.raw<OutboxEntry[]>([]);
  let archived = $state.raw<OutboxEntry[]>([]);
  let currentBudget = $state.raw<{ id: string; epoch: string } | null>(null);
  let confirming = $state("");
  const retired = (item: OutboxEntry) =>
    !!currentBudget &&
    (item.budgetId !== currentBudget.id ||
      (!!item.pending.budgetEpoch &&
        item.pending.budgetEpoch !== currentBudget.epoch));
  let names = $state.raw<Record<string, string>>({});
  let loading = $state(true);
  let error = $state("");
  let notice = $state("");
  let busy = $state(false);
  let mounted = true;
  let sequence = 0;
  const watched = new Map<string, string>();
  async function refresh() {
    const current = ++sequence;
    try {
      const [rows, savedArchives, serverBudget] = await Promise.all([
        entries(),
        archivedEntries(),
        offlineStatus.reachable
          ? request<{ data: { id: string; epoch: string } }>("/budget")
              .then((r) => r.data)
              .catch(() => null)
          : null,
      ]);
      for (const row of rows) watched.set(row.key, row.budgetId);
      const resolved = (
        await Promise.all([...new Set(watched.values())].map(receipts))
      )
        .flat()
        .filter((row) => watched.has(row.key))
        .sort((a, b) => a.at.localeCompare(b.at) || a.key.localeCompare(b.key));
      if (!mounted || current !== sequence) return;
      items = rows;
      archived = savedArchives;
      if (serverBudget) currentBudget = serverBudget;
      if (resolved.length) notice = resolved.at(-1)!.message;
      error = "";
    } catch (e) {
      if (mounted && current === sequence) error = errorMessage(e);
    } finally {
      if (mounted && current === sequence) loading = false;
    }
  }
  async function retry(item: OutboxEntry, cancel = false) {
    busy = true;
    error = "";
    try {
      await retryEntry(item, cancel);
      setReachable(navigator.onLine);
      await deliverPending();
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function archive(item: OutboxEntry) {
    busy = true;
    error = "";
    try {
      await archiveRetiredEntry(item);
      confirming = "";
      notice =
        "Saved request archived on this device. It will not be sent. This does not confirm whether an earlier attempt reached the server.";
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    const stop = subscribeOffline(() => void refresh());
    void readCatalog()
      .then((catalog) => {
        if (mounted)
          names = Object.fromEntries(
            (catalog?.budgets ?? []).map((budget) => [budget.id, budget.name]),
          );
      })
      .catch(() => {});
    void refresh();
    return () => {
      mounted = false;
      stop();
    };
  });
</script>

<section aria-labelledby="pending-recovery-heading">
  <div class="page-heading">
    <h1 id="pending-recovery-heading">
      {t("Saved changes on this device")}
    </h1>
  </div>
  <p class="muted">
    {t(
      "Saved work remains available here even when its budget was deleted or is missing from the server.",
    )}
  </p>
  {#if error}<div role="alert">
      <p class="error">{error}</p>
      <button onclick={() => void refresh()}
        >{t("Retry reading saved work")}</button
      >
    </div>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if loading}<p role="status">{t("Reading saved expenses…")}</p>
  {:else if items.length}<div data-testid="pending-expenses">
      {#each items as item (item.key)}<article class="pending-recovery-item">
          <h2>
            {item.row.payee_name || item.row.account_name || t("Expense")}
          </h2>
          <p>
            {names[item.budgetId] || t("Unavailable budget")} ·
            <span class="budget-id">{item.budgetId}</span>
          </p>
          <p class="amount">
            <span id={`pending-amount-${item.key}`}
              >{formatAmountInput(item.row.amount, item.format)}</span
            >
            ·
            <time datetime={item.row.date}>{item.row.date}</time>
          </p>
          {#if item.row.notes}<p class="notes">{item.row.notes}</p>{/if}
          <p role="status">
            {item.blocked ||
              (item.cancelRequested
                ? t("Cancellation waiting for confirmation")
                : t("Saved on this device."))}
          </p>
          <div class="actions">
            <a
              href={`data:application/json;charset=utf-8,${encodeURIComponent(JSON.stringify(item, null, 2))}`}
              download={`saved-expense-${item.key}.json`}
              >Download saved expense</a
            >
            {#if retired(item)}<button
                disabled={busy}
                onclick={() => (confirming = item.key)}
                >Archive retired request</button
              >{/if}
            <button onclick={() => void retry(item)} disabled={busy}
              >{t("Retry saved expense")}</button
            >
            <button onclick={() => void retry(item, true)} disabled={busy}
              >{t("Cancel saved change safely")}</button
            >
          </div>
          {#if confirming === item.key}
            <p>
              This request belongs to an earlier budget identity or version.
              Download its details first. Archiving stops retries and keeps an
              original copy on this device; it does not undo any server changes.
            </p>
            <div class="actions">
              <button disabled={busy} onclick={() => archive(item)}
                >Confirm local archive</button
              ><button disabled={busy} onclick={() => (confirming = "")}
                >Keep pending request</button
              >
            </div>
          {/if}
          <p class="hint">
            {t(
              "Safe cancellation recovers a change already saved, or prevents this original request from being saved later.",
            )}
          </p>
        </article>{/each}
    </div>{:else if !error}<p role="status">
      {t("No saved expenses on this device.")}
    </p>{/if}
  {#if archived.length}<details>
      <summary>Archived saved changes ({archived.length})</summary>
      <p>
        Original requests retained on this device. These are not sent to the
        server.
      </p>
      <ul>
        {#each archived as item (item.key)}<li>
            {item.row.payee_name || "Expense"} · {item.row.date}
            <a
              href={`data:application/json;charset=utf-8,${encodeURIComponent(JSON.stringify(item, null, 2))}`}
              download={`archived-expense-${item.key}.json`}
              >Download archived expense</a
            >
          </li>{/each}
      </ul>
    </details>{/if}
</section>

<style>
  .pending-recovery-item {
    border-top: 1px solid var(--line);
    padding-block: 1.25rem;
    overflow-wrap: anywhere;
  }
  h2 {
    font-size: 1.15rem;
    margin: 0;
  }
  .amount {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    white-space: normal;
  }
  .budget-id {
    color: var(--muted);
  }
  .notes {
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .actions button {
    max-width: 100%;
  }
</style>
