<script lang="ts">
  import { onMount } from "svelte";
  import { request, errorMessage, uuid, type Account } from "./api";
  import {
    mutate,
    requestCancellation,
    draftKey,
    type Reference,
    type Transaction,
  } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import { formatAmountInput } from "./money";
  import { displayDate } from "./dates";
  import {
    conversionKey,
    readConversion,
    type ConversionContext,
    type ConversionIntent,
  } from "./future-conversion";
  import type { ConversionPreview } from "./schedules";
  let {
    budgetId,
    context,
    accounts,
    payees,
    categories,
    oncomplete,
    onclose,
  }: {
    budgetId: string;
    context: ConversionContext;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    oncomplete: (notice: string) => Promise<void> | void;
    onclose: () => void;
  } = $props();
  let intent = $state<ConversionIntent | null>(null),
    busy = $state(false),
    error = $state(""),
    notice = $state(""),
    broken = $state(false);
  let checkpoint: string | null = null;
  function save(next: ConversionIntent | null) {
    if (localStorage.getItem(conversionKey(budgetId)) !== checkpoint)
      throw new Error(
        "Another tab changed this conversion. Reload to review its saved request.",
      );
    const raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(conversionKey(budgetId));
    else localStorage.setItem(conversionKey(budgetId), raw);
    checkpoint = raw;
    intent = next;
  }
  onMount(() => {
    try {
      checkpoint = localStorage.getItem(conversionKey(budgetId));
      const old = readConversion(budgetId);
      if (old) intent = old;
      else
        save({
          ...structuredClone(context),
          version: 1,
          scheduleId: uuid(),
          preview: null,
          pending: null,
          confirmed: false,
        });
      if (!intent?.pending && !intent?.confirmed && !intent?.preview)
        void preview();
    } catch (e) {
      broken = true;
      error = errorMessage(e);
    }
  });
  async function preview() {
    if (!intent || intent.pending || busy) return;
    busy = true;
    error = "";
    try {
      let snapshot = intent.snapshot;
      if (snapshot === null) {
        const cp = (
          await request<{ data: { snapshot: string; epoch: string } }>(
            `/budgets/${budgetId}/checkpoint`,
          )
        ).data;
        if (cp.epoch !== intent.epoch)
          throw new Error(
            "This budget was restored. Keep the original fields, close this conversion and review the current budget before preparing another request.",
          );
        snapshot = cp.snapshot;
        save({ ...$state.snapshot(intent), snapshot });
      }
      const result = (
        await request<{ data: ConversionPreview }>(
          `/budgets/${budgetId}/schedules/convert-preview`,
          { snapshot, ...$state.snapshot(intent.source) },
        )
      ).data;
      if (result.epoch !== intent.epoch)
        throw new Error(
          "The budget generation changed. Keep this work and review the current budget.",
        );
      save({ ...$state.snapshot(intent), preview: result });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function sourceName(
    row: Record<string, unknown>,
    kind: "payee" | "category",
  ) {
    const refs = kind === "payee" ? payees : categories;
    const id = row[kind + "_id"];
    const inline = row["new_" + kind] as { name?: string } | undefined;
    return String(
      row[kind + "_name"] ??
        inline?.name ??
        refs.find((r) => r.id === id)?.name ??
        `No ${kind}`,
    );
  }
  function amount(value: unknown) {
    return typeof value === "number" && intent
      ? formatAmountInput(value, intent.format)
      : "Unknown amount";
  }
  async function finish() {
    if (!intent) return;
    let message = "Future transaction scheduled. No purchase was added now.";
    if (intent.sourceDraftRaw) {
      const key = intent.sourceDraftKey ?? draftKey(budgetId);
      if (localStorage.getItem(key) === intent.sourceDraftRaw)
        localStorage.removeItem(key);
      else if (localStorage.getItem(key) !== null)
        message +=
          " Another tab changed the original draft; it was kept for review.";
    }
    await oncomplete(message);
    save(null);
  }
  async function send(cancel = false) {
    if (!intent || busy || broken) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (intent.confirmed) {
        await finish();
        return;
      }
      if (!intent.pending) {
        if (!intent.preview?.is_future)
          throw new Error("Review a future date before converting.");
        if (intent.preview.protected_count)
          throw new Error(
            "Unlock the complete affected graph, then close and reopen conversion to review its current state.",
          );
        for (const row of intent.preview.affected_transactions)
          observeVersions(row.field_versions);
        const pending = {
          path: `/budgets/${budgetId}/schedules/commands`,
          method: "POST" as const,
          body: {
            action: "convert",
            id: intent.scheduleId,
            snapshot: intent.snapshot,
            ...$state.snapshot(intent.source),
          },
          key: uuid(),
          historyOwner: await historyOwner(),
          edit_clock: intent.clock ?? nextEditClock(),
          budgetEpoch: intent.epoch,
        };
        save({ ...$state.snapshot(intent), pending });
      }
      if (cancel) requestCancellation(intent.pending!);
      save($state.snapshot(intent));
      try {
        await mutate(intent.pending!, cancel);
      } catch (e) {
        if (
          e instanceof Error &&
          "code" in e &&
          e.code === "operation_canceled"
        ) {
          resetCorrectedClock();
          save({ ...$state.snapshot(intent), pending: null });
          notice =
            "The saved conversion was cancelled. Your original fields remain; close and reopen to review a new conversion.";
          return;
        }
        throw e;
      }
      save({ ...$state.snapshot(intent), pending: null, confirmed: true });
      await finish();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function close() {
    if (!intent || intent.pending || busy) return;
    try {
      save(null);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section class="conversion" aria-labelledby="future-conversion-title">
  <h2 id="future-conversion-title">Schedule a future transaction</h2>
  <p>
    Replace the future entry with a single scheduled occurrence. The schedule
    posts once when due; its forecast is excluded from balances until then.
  </p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p
      role="status"
    >
      {notice}
    </p>{/if}
  {#if intent}
    {#if intent.pending}<p role="status">
        The original conversion request is saved. Retry keeps its original
        fields, clock and budget generation. Cancellation requires a confirmed
        server outcome.
      </p>{/if}
    {#if intent.preview}{@const p = intent.preview}{@const row =
        p.transaction as Record<string, unknown>}
      <dl>
        <div>
          <dt>Date</dt>
          <dd>{displayDate(String(row.date), intent.dateFormat)}</dd>
        </div>
        <div>
          <dt>Amount</dt>
          <dd>{amount(row.amount)}</dd>
        </div>
        <div>
          <dt>Account</dt>
          <dd>
            {row.account_name ??
              accounts.find((a) => a.id === row.account_id)?.name ??
              "Account unavailable"}
          </dd>
        </div>
        <div>
          <dt>Payee</dt>
          <dd>{sourceName(row, "payee")}</dd>
        </div>
        <div>
          <dt>Category</dt>
          <dd>{sourceName(row, "category")}</dd>
        </div>
        <div>
          <dt>Notes</dt>
          <dd>{row.notes || "No notes"}</dd>
        </div>
      </dl>
      {#if intent.source.transfer_account_id}<p>
          Transfer destination: {accounts.find(
            (a) => a.id === intent!.source.transfer_account_id,
          )?.name ?? "Account unavailable"}. Both paired legs will post
          together.
        </p>{/if}
      {#if p.is_beyond_window}<p role="status">
          This entry is {p.days_until_transaction} days away, beyond the current {p.upcoming_days}-day
          preview window. It remains saved as a schedule and becomes visible in
          that window later.
        </p>{/if}
      <p>
        Scheduled entries start uncleared. Original saved entries below will be
        removed together; their history remains available.
      </p>
      {#if p.protected_count}<p role="alert">
          {p.protected_count} affected entries are reconciled. Unlock and review the
          complete graph before converting.
        </p>{/if}
      {#if p.affected_transactions.length}<h3>Existing entries replaced</h3>
        <ul>
          {#each p.affected_transactions as old (old.id)}<li>
              <strong
                >{displayDate(old.date, intent.dateFormat)} · {amount(
                  old.amount,
                )} · {old.account_name}</strong
              ><span
                >{old.payee_name ?? "No payee"} · {old.category_name ??
                  "No category"} · {old.notes || "No notes"}</span
              ><span
                >{old.cleared ? "Cleared" : "Uncleared"} · {old.reconciled
                  ? "Reconciled"
                  : "Not reconciled"}{old.is_parent
                  ? " · Split parent"
                  : old.parent_id
                    ? " · Allocation"
                    : ""}{old.transfer_id ? " · Paired transfer" : ""}</span
              >
            </li>{/each}
        </ul>{:else}<p>
          This draft has not been recorded as a purchase. Its original identity
          is retained for safe recovery.
        </p>{/if}
    {/if}
    <div class="toolbar">
      {#if intent.confirmed}<button onclick={() => send()} disabled={busy}
          >Finish confirmed conversion</button
        >{:else if intent.pending}<button
          class="primary"
          onclick={() => send()}
          disabled={busy}>Retry saved conversion</button
        ><button onclick={() => send(true)} disabled={busy}
          >Cancel saved conversion safely</button
        >{:else}<button onclick={preview} disabled={busy || broken}
          >Review captured conversion</button
        ><button
          class="primary"
          onclick={() => send()}
          disabled={busy ||
            broken ||
            !intent.preview?.is_future ||
            !!intent.preview?.protected_count}
          >Confirm schedule conversion</button
        ><button onclick={close} disabled={busy}
          >Keep original transaction</button
        >{/if}
    </div>
  {/if}
</section>

<style>
  .conversion {
    border: 1px solid var(--border);
    padding: 1.25rem;
    margin: 1.5rem 0;
    max-width: 68rem;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
    margin: 1.25rem 0;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
    margin: 1.5rem 0;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0.35rem 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-variant-numeric: tabular-nums;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    padding: 1rem 0;
    border-top: 1px solid var(--border);
  }
  li span {
    display: block;
    margin: 0.35rem 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  @media (max-width: 40rem) {
    .conversion {
      padding: 0.75rem;
    }
    .toolbar > * {
      flex: 1 1 12rem;
    }
  }
</style>
