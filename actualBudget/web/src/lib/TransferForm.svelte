<script lang="ts">
  import { onMount, tick } from "svelte";
  import DateInput from "./DateInput.svelte";
  import { displayDate, parseDateInput, validDate } from "./dates";
  import type { DateFormat } from "./preferences/types";
  import {
    request,
    uuid,
    errorMessage,
    RequestError,
    type Account,
  } from "./api";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
    LocalClockError,
  } from "./edit-clock";
  import {
    formatAmount,
    formatAmountInput,
    parseAmount,
    captureAmountFormat,
    today,
    type AmountFormat,
  } from "./money";
  import { preferences } from "./preferences.svelte";
  import { offlineStatus, subscribeOffline } from "./offline/delivery";
  import {
    cancellationRequested,
    requestCancellation,
    type PendingMutation,
    type Transaction,
    type Reference,
  } from "./register-api";
  type Mode = "create" | "edit" | "link" | "close";
  type Draft = {
    version: 1;
    mode: Mode;
    source: string;
    destination: string;
    id: string;
    counterpart: string;
    amount: string;
    date: string;
    dateFormat: DateFormat;
    notes: string;
    category: string;
    cleared: boolean;
    syncDate: boolean;
    epoch: string;
    balance: number;
    format: AmountFormat;
    original: Transaction | null;
    pending: PendingMutation | null;
    protectionToken?: string;
    protectionClock?: string;
    protectedCount?: number;
    confirmedReconciled?: boolean;
  };
  let {
    budgetId,
    accounts,
    categories,
    initial = null,
    mode = "create",
    defaultAccountId = null,
    oncomplete,
    oncancel,
  }: {
    budgetId: string;
    accounts: Account[];
    categories: Reference[];
    initial?: Transaction | null;
    mode?: Mode;
    defaultAccountId?: string | null;
    oncomplete: (notice: string) => void;
    oncancel: () => void;
  } = $props();
  let draft = $state<Draft | null>(null);
  let error = $state("");
  let busy = $state(false);
  let ready = $state(false);
  let offline = $state(!offlineStatus.reachable);
  let clockProblem = $state(false);
  let candidates = $state.raw<Transaction[]>([]);
  let capabilities: string[] = [];
  let saved: string | null = null;
  let alive = true;
  const storageKey = () => `actualBudget.transfers.v1.${budgetId}`;
  let source = $derived(accounts.find((a) => a.id === draft?.source));
  let destination = $derived(accounts.find((a) => a.id === draft?.destination));
  let title = $derived(
    draft?.mode === "edit"
      ? "Edit transfer"
      : draft?.mode === "link"
        ? "Link existing entries"
        : draft?.mode === "close"
          ? "Transfer balance and close account"
          : "Move money",
  );
  function persist() {
    if (!draft) return;
    if (localStorage.getItem(storageKey()) !== saved)
      throw new Error(
        "Another tab changed this transfer. Reload before continuing.",
      );
    const next = JSON.stringify($state.snapshot(draft));
    localStorage.setItem(storageKey(), next);
    saved = next;
  }
  function remove() {
    if (localStorage.getItem(storageKey()) !== saved)
      throw new Error(
        "Another tab changed this transfer. Reload before continuing.",
      );
    localStorage.removeItem(storageKey());
    saved = null;
  }
  async function load() {
    busy = true;
    try {
      saved = localStorage.getItem(storageKey());
      if (saved) {
        const value = JSON.parse(saved) as Draft;
        if (
          value.version !== 1 ||
          !["create", "edit", "link", "close"].includes(value.mode) ||
          !value.format ||
          typeof value.epoch !== "string" ||
          (value.pending &&
            !value.pending.path.startsWith(`/budgets/${budgetId}/`))
        )
          throw new Error(
            "Saved transfer is unreadable. Keep this browser data for recovery.",
          );
        draft = value;
      } else {
        const account =
          accounts.find(
            (a) => a.id === (initial?.account_id ?? defaultAccountId),
          ) ?? accounts.find((a) => !a.closed && !a.deleted);
        draft = {
          version: 1,
          mode,
          source: account?.id ?? "",
          destination: "",
          id: initial?.id ?? uuid(),
          counterpart: initial?.transfer_id ?? uuid(),
          amount: formatAmountInput(initial?.amount ?? 0),
          date: displayDate(
            initial?.date ?? today(),
            preferences.shared.date_format,
          ),
          dateFormat: preferences.shared.date_format,
          notes: initial?.notes ?? "",
          category: initial?.category_id ?? "",
          cleared: initial?.cleared ?? false,
          syncDate: preferences.shared.sync_transfer_date ?? false,
          epoch: preferences.budgetEpoch ?? "",
          balance: account?.balance ?? 0,
          format: captureAmountFormat(),
          original: initial ? structuredClone($state.snapshot(initial)) : null,
          pending: null,
        };
        persist();
      }

      const info = await request<{ data: { capabilities: string[] } }>("/info");
      capabilities = info.data.capabilities;
      if (!capabilities.includes("transfers"))
        throw new Error(
          "This server does not support transfers. Saved work has been kept.",
        );
      if (!draft) return;
      if (draft.mode === "edit" && !draft.pending && !draft.destination) {
        const other = await request<{ data: Transaction }>(
          `/budgets/${budgetId}/transactions/${draft.counterpart}`,
        );
        if (!alive) return;
        draft.destination = other.data.account_id;
        observeVersions(other.data.field_versions);
        persist();
      }
      if (draft.mode === "edit" && !draft.pending && !draft.protectionToken)
        await captureProtection();
      ready = true;
      await tick();
      if (alive) document.getElementById("transfer-title")?.focus();
      if (draft.mode === "link" && draft.destination) await matching();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void load();
    const stop = subscribeOffline(() => {
      offline = !offlineStatus.reachable;
    });
    return () => {
      alive = false;
      stop();
    };
  });
  function edit<K extends keyof Draft>(key: K, value: Draft[K]) {
    if (!draft || draft.pending || busy || offline) return;
    try {
      draft[key] = value;
      if (key === "destination" && draft.mode === "link") {
        draft.counterpart = "";
        draft.protectionToken = undefined;
        draft.protectionClock = undefined;
        draft.protectedCount = undefined;
        draft.confirmedReconciled = false;
      }
      if (key === "source")
        draft.balance = accounts.find((a) => a.id === value)?.balance ?? 0;
      persist();
    } catch (e) {
      error = errorMessage(e);
      ready = false;
    }
  }
  async function captureProtection() {
    if (!draft || draft.pending) return;
    const current = await request<{
      transfer_protection_token: string;
      transfer_protection_clock: string;
      transfer_protected_count: number;
    }>(
      `/budgets/${budgetId}/transactions/${draft.id}/splits${draft.mode === "link" ? `?counterpart_id=${draft.counterpart}` : ""}`,
    );
    if (!alive || !draft) return;
    draft.protectionToken = current.transfer_protection_token;
    draft.protectionClock = current.transfer_protection_clock;
    draft.protectedCount = current.transfer_protected_count;
    draft.confirmedReconciled = false;
    observeVersions({ reconciled: current.transfer_protection_clock });
    persist();
  }
  async function chooseCounterpart(id: string) {
    edit("counterpart", id);
    if (!draft || draft.pending || !id) return;
    busy = true;
    try {
      await captureProtection();
    } catch (e) {
      error = errorMessage(e);
      ready = false;
    } finally {
      busy = false;
    }
  }
  async function matching() {
    if (!draft || draft.pending || offline) return;
    busy = true;
    error = "";
    candidates = [];
    try {
      const found: Transaction[] = [];
      let offset: number | null = 0;
      let snapshot: string | undefined;
      do {
        const page: {
          data: Transaction[];
          next_offset: number | null;
          snapshot: string;
        } = await request(
          `/budgets/${budgetId}/transactions?account_id=${draft.destination}&limit=1000&offset=${offset}${snapshot ? `&snapshot=${snapshot}` : ""}`,
        );
        snapshot = page.snapshot;
        found.push(
          ...page.data
            .flatMap((row) => (row.is_parent ? (row.children ?? []) : [row]))
            .filter(
              (r) =>
                !r.transfer_id &&
                !r.is_parent &&
                !r.starting_balance &&
                r.amount === -(draft?.original?.amount ?? 0),
            ),
        );
        offset = page.next_offset;
      } while (offset !== null);
      if (alive) candidates = found;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function send(cancel = false) {
    if (!draft || busy || offline || !ready) return;
    busy = true;
    error = "";
    clockProblem = false;
    try {
      if (!draft.pending) {
        if (draft.epoch !== preferences.budgetEpoch)
          throw new Error(
            "This budget was restored. Close this unsent draft and review the current accounts before starting again.",
          );
        if (
          (draft.mode === "edit" || draft.mode === "link") &&
          draft.protectedCount &&
          !draft.confirmedReconciled
        )
          throw new Error(
            "Confirm editing reconciled entries in this transfer, or unlock them first.",
          );
        const date = parseDateInput(
          draft.date,
          draft.dateFormat ?? "yyyy-MM-dd",
        );
        if (draft.mode !== "link" && !validDate(date))
          throw new Error("Enter a valid calendar date.");
        let body: Record<string, unknown>;
        let path = `/budgets/${budgetId}/transfers`;
        let method: PendingMutation["method"] = "POST";
        if (draft.mode === "link") {
          const selected = candidates.find((r) => r.id === draft!.counterpart);
          if (!selected)
            throw new Error("Choose a matching entry in the other account.");
          observeVersions(selected.field_versions);
          path += "/link";
          body = { id: draft.id, counterpart_id: draft.counterpart };
        } else if (draft.mode === "close") {
          path = `/budgets/${budgetId}/accounts/${draft.source}/close`;
          body = {
            id: draft.id,
            counterpart_id: draft.counterpart,
            to_account_id: draft.destination,
            date,
            expected_balance: draft.balance,
            category_id: draft.category || null,
          };
        } else {
          const amount = parseAmount(draft.amount, draft.format);
          if (amount === null) throw new Error("Enter an exact amount.");
          if (draft.mode === "edit") {
            path = `/budgets/${budgetId}/transactions/${draft.id}`;
            method = "PATCH";
            body = {};
            for (const [field, value] of Object.entries({
              amount,
              date,
              notes: draft.notes,
              cleared: draft.cleared,
              category_id: draft.category || null,
              account_id: draft.source,
            }))
              if (draft.original?.[field as keyof Transaction] !== value)
                body[field] = value;
            if (Object.keys(body).length === 0)
              throw new Error("Change a transfer field before saving.");
            if (body.date) body.sync_transfer_date = draft.syncDate;
          } else
            body = {
              id: draft.id,
              counterpart_id: draft.counterpart,
              account_id: draft.source,
              to_account_id: draft.destination,
              amount,
              date,
              notes: draft.notes,
              cleared: draft.cleared,
              category_id: draft.category || null,
            };
        }
        if (
          (draft.mode === "edit" || draft.mode === "link") &&
          draft.confirmedReconciled
        ) {
          if (!draft.protectionToken)
            throw new Error(
              "Reopen the transfer to review its current protection.",
            );
          body.confirm_reconciled = true;
          body.transfer_protection_token = draft.protectionToken;
        }
        if (draft.protectionClock)
          observeVersions({ reconciled: draft.protectionClock });
        draft.pending = {
          path,
          method,
          body,
          key: uuid(),
          edit_clock: nextEditClock(draft.original?.field_versions),
          budgetEpoch: draft.epoch,
        };
      }
      // Every dispatch, including retry/cancel, requires a durable immutable intent.
      // A failed write leaves the prior ownership checkpoint intact.
      persist();
      const pending = draft.pending;
      cancel ||= cancellationRequested(pending);
      if (cancel) {
        const capability =
          draft.mode === "close"
            ? "account_cancellation"
            : draft.mode === "edit"
              ? "transaction_cancellation"
              : "transfer_cancellation";
        if (!capabilities.includes(capability))
          throw new Error(
            "This server cannot safely cancel this transfer. Retry the saved request.",
          );
        requestCancellation(pending);
        persist();
      }
      const result = await request<{
        data: Transaction;
        conflicts?: unknown[];
      }>(
        pending.path,
        pending.body,
        pending.key,
        pending.method,
        pending.edit_clock,
        cancel,
        pending.budgetEpoch,
      );
      remove();
      oncomplete(
        result.conflicts?.length
          ? "Transfer saved; newer fields were retained. Review both accounts."
          : cancel
            ? "The transfer was already saved. Both accounts have been refreshed."
            : "Transfer saved. Both accounts updated.",
      );
    } catch (e) {
      if (e instanceof RequestError && e.code === "operation_canceled") {
        const pending = draft.pending;
        draft.pending = null;
        try {
          persist();
          error = "Transfer request canceled. Your fields are kept for review.";
        } catch (failure) {
          draft.pending = pending;
          error = errorMessage(failure);
        }
      } else {
        error = errorMessage(e);
        clockProblem =
          e instanceof LocalClockError ||
          (e instanceof RequestError &&
            ["clock_skew", "clock_overflow", "invalid_clock"].includes(
              e.code ?? "",
            ));
      }
    } finally {
      busy = false;
    }
  }
  function close() {
    if (busy || draft?.pending) return;
    try {
      remove();
      oncancel();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function corrected() {
    try {
      resetCorrectedClock();
      clockProblem = false;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section
  class="transfer-form"
  aria-labelledby="transfer-title"
  aria-busy={busy}
>
  <h3 id="transfer-title" tabindex="-1">{title}</h3>
  <p>
    Both accounts update together. Payments between on-budget accounts do not
    count as income or spending.
  </p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if offline}<p role="status">
      Reconnect to manage transfers. Your saved request is kept.
    </p>{/if}
  {#if busy}<p role="status">Loading or saving transfer…</p>{/if}
  {#if draft}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void send();
      }}
    >
      <fieldset disabled={busy || !!draft.pending || offline || !ready}>
        <div class="transfer-fields">
          <div>
            <label for="transfer-from">From account</label><select
              id="transfer-from"
              value={draft.source}
              disabled={draft.mode === "link" || draft.mode === "close"}
              onchange={(e) => edit("source", e.currentTarget.value)}
              >{#each accounts.filter((a) => (!a.closed && !a.deleted) || a.id === draft?.source) as a (a.id)}<option
                  value={a.id}>{a.name}</option
                >{/each}</select
            >
          </div>
          <div>
            <label for="transfer-to">To account</label><select
              id="transfer-to"
              value={draft.destination}
              disabled={draft.mode === "edit"}
              onchange={(e) => {
                edit("destination", e.currentTarget.value);
                if (draft?.mode === "link") void matching();
              }}
              required
              ><option value="">Choose account</option
              >{#each accounts.filter((a) => a.id !== draft?.source && !a.closed && !a.deleted) as a (a.id)}<option
                  value={a.id}>{a.name}</option
                >{/each}</select
            >
          </div>
          {#if draft.mode === "link"}
            <div>
              <label for="transfer-match">Matching entry</label><select
                id="transfer-match"
                value={draft.counterpart}
                onchange={(e) => void chooseCounterpart(e.currentTarget.value)}
                required
                ><option value="">Choose entry</option
                >{#each candidates as row (row.id)}<option value={row.id}
                    >{row.date} · {formatAmount(row.amount)} · {row.payee_name ??
                      row.notes ??
                      "No payee"}</option
                  >{/each}</select
              >
            </div>
            {#if draft.destination && !candidates.length}<p>
                No unlinked opposite amount found in this account.
              </p>{/if}
          {:else}
            {#if draft.mode === "close"}<p>
                Transfer the remaining balance of <strong
                  >{formatAmount(draft.balance)}</strong
                >
                and close {source?.name}.
              </p>{:else}<div>
                <label for="transfer-amount">Amount</label><input
                  id="transfer-amount"
                  value={draft.amount}
                  inputmode="decimal"
                  oninput={(e) => edit("amount", e.currentTarget.value)}
                  required
                /><span class="hint"
                  >Negative moves money from this account; positive moves it
                  into this account.</span
                >
              </div>{/if}
            <DateInput
              id="transfer-date"
              label="Date"
              format={draft.dateFormat}
              bind:value={() => draft!.date, (value) => edit("date", value)}
              required
            />
            {#if draft.mode === "edit"}<label class="check-label"
                ><input
                  type="checkbox"
                  checked={draft.syncDate}
                  onchange={(e) => edit("syncDate", e.currentTarget.checked)}
                />Sync both transfer dates</label
              >{/if}
            {#if !source?.off_budget && destination?.off_budget}<div>
                <label for="transfer-category">Category</label><select
                  id="transfer-category"
                  value={draft.category}
                  onchange={(e) => edit("category", e.currentTarget.value)}
                  ><option value="">Uncategorized</option
                  >{#each categories.filter((c) => (!c.deleted && !c.hidden && !c.group_hidden) || c.id === draft?.category) as c (c.id)}<option
                      value={c.id}>{c.name}</option
                    >{/each}</select
                >
              </div>{/if}
            {#if draft.mode !== "close"}<label class="notes"
                >Notes<textarea
                  value={draft.notes}
                  maxlength="10000"
                  oninput={(e) => edit("notes", e.currentTarget.value)}
                ></textarea></label
              ><label class="check-label"
                ><input
                  type="checkbox"
                  checked={draft.cleared}
                  onchange={(e) => edit("cleared", e.currentTarget.checked)}
                />Cleared in this account</label
              >{/if}
          {/if}
        </div>
        {#if draft.protectedCount}
          <label class="check-label"
            ><input
              type="checkbox"
              checked={draft.confirmedReconciled ?? false}
              onchange={(e) =>
                edit("confirmedReconciled", e.currentTarget.checked)}
            />Confirm editing reconciled entries in this transfer</label
          >
          <p>
            Linked split allocations are included. Entries remain reconciled.
          </p>
        {/if}
      </fieldset>
      {#if draft.pending}<p>
          The original request is saved. Retry unchanged or cancel it before
          editing.
        </p>{/if}
      <div class="actions">
        <button
          class="primary"
          disabled={busy || offline || !ready}
          type="submit"
          >{draft.pending
            ? "Retry saved transfer"
            : draft.mode === "link"
              ? "Link entries"
              : draft.mode === "close"
                ? "Transfer and close"
                : "Save transfer"}</button
        >
        {#if draft.pending}<button
            type="button"
            disabled={busy || offline || !ready}
            onclick={() => void send(true)}>Cancel pending transfer</button
          >{:else}<button type="button" disabled={busy} onclick={close}
            >Close transfer form</button
          >{/if}
      </div>
      {#if clockProblem && !draft.pending}<button
          type="button"
          disabled={busy}
          onclick={corrected}>Use corrected device time</button
        >{/if}
    </form>
  {:else if !busy}<button onclick={load}>Retry transfer recovery</button>{/if}
</section>

<style>
  .transfer-form {
    padding-block: 1.5rem;
    border-block: 1px solid var(--line);
  }
  .transfer-form p {
    max-width: 65ch;
  }
  .transfer-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    align-content: start;
  }
  .check-label {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .notes {
    grid-column: 1 / -1;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-block-start: 1rem;
  }
  @media (max-width: 700px) {
    .transfer-fields {
      grid-template-columns: 1fr;
    }
  }
</style>
