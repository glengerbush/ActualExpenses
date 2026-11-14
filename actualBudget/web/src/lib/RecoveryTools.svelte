<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { errorMessage, RequestError, request, type Budget } from "./api";
  import { formatAmount } from "./money";
  import { preferences } from "./preferences.svelte";
  import {
    offlineStatus,
    subscribeOffline,
    setReachable,
  } from "./offline/events";
  import {
    checkConnection,
    readRepair,
    prepareRepair,
    saveRepair,
    completeRepair,
    cancelRepair,
    sendRepair,
    resetDownloadedData,
    type RecoveryPreview,
    type RepairOperation,
    type RecoveryResult,
  } from "./recovery";
  let {
    budget,
    onclose,
    onreset,
    onchanged,
  }: {
    budget: Budget | null;
    onclose: () => void;
    onreset: () => void | Promise<void>;
    onchanged: () => void | Promise<void>;
  } = $props();
  let preview = $state.raw<RecoveryPreview | null>(null),
    pending = $state.raw<RepairOperation | null>(null),
    confirmed = $state.raw<RecoveryResult | null>(null);
  let busy = $state(true),
    offline = $state(!offlineStatus.reachable),
    error = $state(""),
    notice = $state(""),
    approved = $state(false),
    confirmReset = $state(false),
    storageBlocked = $state(false);
  let mounted = true;
  async function inspect() {
    if (!budget || pending || busy) return;
    busy = true;
    error = "";
    notice = "";
    try {
      const { data } = await request<{ data: RecoveryPreview }>(
        `/budgets/${budget.id}/recovery`,
      );
      if (mounted) {
        preview = data;
        approved = false;
      }
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function apply(cancel = false) {
    if (!budget || busy || storageBlocked) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (!pending) {
        if (!preview?.can_apply || !approved)
          throw new Error(
            "Review the complete preview and confirm the repair first.",
          );
        const operation = prepareRepair(budget.id, preview);
        await saveRepair(operation);
        pending = operation;
      }
      if (cancel) pending = await cancelRepair(pending);
      const result = await sendRepair(pending);
      await completeRepair(pending);
      pending = null;
      confirmed = result;
      preview = null;
      notice = `Repair confirmed: ${result.repaired_fields} fields repaired and ${result.recalculated_accounts} account totals recalculated. The pre-repair backup is available below.`;
      await onchanged();
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        pending
      ) {
        try {
          await completeRepair(pending);
          pending = null;
          preview = null;
          notice =
            "Saved repair was safely canceled. No delayed request can apply it.";
        } catch (storage) {
          error = errorMessage(storage);
        }
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function reset() {
    busy = true;
    error = "";
    try {
      await resetDownloadedData();
      confirmReset = false;
      await onreset();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function reconnect() {
    busy = true;
    error = "";
    try {
      await checkConnection();
      await onreset();
    } catch (e) {
      setReachable(false, errorMessage(e));
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  const readable = $derived(
    !!budget &&
      preferences.hasUsableShared &&
      preferences.budgetId === budget.id,
  );
  function amount(value: number) {
    return readable ? formatAmount(value) : "••••";
  }
  function shown(value: unknown) {
    return !readable ? "••••" : value === null ? "None" : String(value);
  }
  onMount(() => {
    if (budget)
      void readRepair(budget.id)
        .then((value) => {
          if (mounted) pending = value;
        })
        .catch((e) => {
          storageBlocked = true;
          error = errorMessage(e);
        })
        .finally(() => {
          if (mounted) busy = false;
        });
    else busy = false;
    const stop = subscribeOffline(() => {
      offline = !offlineStatus.reachable;
    });
    return () => {
      mounted = false;
      stop();
    };
  });
</script>

<section aria-labelledby="recovery-title">
  <div class="page-heading">
    <div>
      <h1 id="recovery-title">Recovery tools</h1>
      <p class="muted">
        Repair supported data problems and rebuild downloaded views without
        discarding saved purchases.
      </p>
    </div>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if busy}<p role="status">
      Working… Keep this page open until the result is confirmed.
    </p>{/if}
  <section aria-labelledby="connection-title">
    <h2 id="connection-title">Connection and downloaded data</h2>
    <p>
      {offline
        ? "Disconnected. Saved work stays on this device."
        : "Connected views can be refreshed from the authoritative server."}
    </p>
    <p>
      Reconnect refreshes the budget and its checkpoint. Reset removes only
      downloaded budget lists, references and transaction views on this browser.
      Saved expenses, requests, delivery receipts, drafts and device settings
      stay intact. Offline views must download again when connected.
    </p>
    <div class="actions">
      <button onclick={reconnect} disabled={busy}>Reconnect and refresh</button
      ><button onclick={() => (confirmReset = true)} disabled={busy}
        >Reset downloaded data</button
      >
    </div>
    {#if confirmReset}<div class="reset-confirmation">
        <h3>Reset downloaded views on this browser?</h3>
        <p>
          Saved work remains on this device and syncs when connected. This also
          preserves unconfirmed budget and repair requests. No server records
          are deleted.
        </p>
        <div class="actions">
          <button onclick={reset} disabled={busy}>Confirm cache reset</button
          ><button onclick={() => (confirmReset = false)} disabled={busy}
            >Keep downloaded data</button
          >
        </div>
      </div>{/if}
  </section>
  <section aria-labelledby="repair-title">
    <h2 id="repair-title">Inspect and repair this budget</h2>
    {#if !budget}<p>
        Open a budget to inspect its data. Connection recovery and
        downloaded-cache reset are available here without selecting a budget.
      </p>
    {:else if pending}<p role="status">
        An unconfirmed repair request is saved for {budget.name}. Retry recovers
        its original result; safe cancellation prevents an uncommitted delayed
        request from applying. The request identity, preview and epoch cannot be
        changed.
        {#if pending.cancelRequested}Cancellation is saved on this browser.
          Retrying sends a cancellation fence with the original request details.{/if}
      </p>
      <div class="actions">
        <button
          onclick={() => apply()}
          disabled={busy || offline || storageBlocked}
          >{pending.cancelRequested
            ? "Retry cancellation"
            : "Retry saved repair"}</button
        ><button
          onclick={() => apply(true)}
          disabled={busy || offline || storageBlocked}
          >Cancel saved repair safely</button
        >
      </div>
    {:else}<p>
        Preview reads every supported check without changing financial records.
        Apply first commits a recoverable native backup, then repairs the
        reviewed fields and recalculates ledger totals together. Budget plans
        already calculate from authoritative transactions on demand.
      </p>
      <button onclick={inspect} disabled={busy || offline || storageBlocked}
        >Inspect budget</button
      >{/if}
    {#if confirmed}<p>
        <a
          href={`/api/v1/budgets/${budget?.id}/backups/${confirmed.backup.id}/download`}
          >Download pre-repair backup</a
        >. Restore through Settings → Backups; restoring rotates the budget
        epoch and retains original receipt evidence.
      </p>{/if}
    {#if preview}<div class="preview">
        <h3>Complete repair preview</h3>
        <p>
          Budget: {preview.name}. {preview.repairs.length} supported field repairs.
          {preview.backup_restorable
            ? "The original data passes the native restore validator."
            : "Automatic apply is blocked because the original data cannot be restored normally."}
        </p>
        <h4>Account totals</h4>
        {#if !readable}<p role="status">
            Display preferences are unavailable. Reconnect and refresh before
            reviewing amounts or approving a new repair. Saved original requests
            can still retry unchanged.
          </p>{/if}
        <p class="hint">
          On narrow screens, focus or scroll the table sideways to review every
          balance.
        </p>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (A scrollable comparison needs focus so keyboard users can scroll every balance.) -->
        <div
          class="table-wrap"
          role="region"
          aria-label="Account balance comparison"
          tabindex="0"
        >
          <table>
            <caption
              >Current cached totals and recalculated ledger totals</caption
            ><thead
              ><tr
                ><th>{t("Account")}</th><th>Cached total</th><th
                  >Recalculated total</th
                ><th>Cached cleared / uncleared</th><th
                  >Recalculated cleared / uncleared</th
                ></tr
              ></thead
            ><tbody
              >{#each preview.accounts as account (account.id)}<tr
                  data-testid="recovery-account"
                  ><th>{account.name}</th><td>{amount(account.before.total)}</td
                  ><td>{amount(account.after.total)}</td><td
                    >{amount(account.before.cleared)} / {amount(
                      account.before.uncleared,
                    )}</td
                  ><td
                    >{amount(account.after.cleared)} / {amount(
                      account.after.uncleared,
                    )}</td
                  ></tr
                >{/each}</tbody
            >
          </table>
        </div>
        <h4>Supported field repairs</h4>
        {#if preview.repairs.length}<ol>
            {#each preview.repairs as repair (repair.id + repair.field)}<li>
                <p>{repair.reason}</p>
                <p class="hint">
                  Transaction {repair.id}, {repair.field}: {shown(
                    repair.before,
                  )} → {shown(repair.after)}
                </p>
              </li>{/each}
          </ol>{:else}<p>No supported field repairs are needed.</p>{/if}
        <h4>Issues requiring review</h4>
        {#if preview.issues.length}<ul>
            {#each preview.issues as issue, index (index)}<li>
                <p>{issue.domain}: {issue.message}</p>
                <p class="hint">
                  {issue.id}{issue.blocking
                    ? " — blocks automatic apply"
                    : " — graph kept unchanged"}
                </p>
              </li>{/each}
          </ul>{:else}<p>
            No unsupported inconsistencies found by the registered checks.
          </p>{/if}
        {#if preview.can_apply}<label class="repair-confirm">
            <input type="checkbox" bind:checked={approved} />
            I reviewed and approve this repair
          </label>
          <p class="hint">Changed data requires a new preview.</p>
          <button
            class="primary"
            onclick={() => apply()}
            disabled={busy ||
              offline ||
              !approved ||
              !readable ||
              storageBlocked}>Apply reviewed repair</button
          >{:else}<p role="status">
            No safely applicable repair. Existing purchases are retained; review
            any listed issues.
          </p>{/if}
      </div>{/if}
  </section>
</section>

<style>
  section section {
    border-top: 1px solid var(--line);
    margin-top: 2rem;
    padding-top: 1rem;
  }
  p {
    max-width: 72ch;
  }
  .reset-confirmation,
  .preview {
    margin-top: 1.5rem;
  }
  .hint {
    overflow-wrap: anywhere;
  }
  li p {
    margin-block: 0.4rem;
  }
  .preview h4 {
    margin-top: 1.5rem;
  }
  td {
    font-variant-numeric: tabular-nums;
  }
</style>
