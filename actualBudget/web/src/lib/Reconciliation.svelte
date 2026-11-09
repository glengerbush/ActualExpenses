<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    request,
    uuid,
    RequestError,
    errorMessage,
    type Account,
  } from "./api";
  import {
    mutate,
    requestCancellation,
    type PendingMutation,
    type Transaction,
  } from "./register-api";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
    validClock,
  } from "./edit-clock";
  import { preferences } from "./preferences.svelte";
  import {
    formatAmount,
    formatAmountInput,
    parseAmount,
    today,
    formatDate,
    captureAmountFormat,
    type AmountFormat,
  } from "./money";
  import DateInput from "./DateInput.svelte";
  import { parseDateInput, validDate } from "./dates";
  import { dateFormats, type DateFormat } from "./preferences/types";
  import { t } from "./i18n.svelte";
  interface Progress {
    edit_clock: string;
    statement_balance: number;
    difference: number;
    balances: { total: number; cleared: number; uncleared: number };
    locked_count: number;
    ready_count: number;
    uncleared_count: number;
    snapshot: string;
    last_reconciled: string | null;
  }
  interface Draft {
    epoch: string;
    format: AmountFormat;
    target: string;
    date: string;
    dateFormat: DateFormat;
    pending: PendingMutation | null;
    clockRejected?: boolean;
    epochRejected?: boolean;
  }
  let {
    budgetId,
    account,
    snapshot,
    disconnected,
    disabled = false,
    unlocking = null,
    onunlocked,
    onchanged,
  }: {
    budgetId: string;
    account: Account;
    snapshot: string;
    disconnected: boolean;
    disabled?: boolean;
    unlocking?: Transaction | null;
    onunlocked: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  let draft = $state<Draft | null>(null);
  let progress = $state<Progress | null>(null);
  let busy = $state(false),
    error = $state(""),
    notice = $state(""),
    broken = $state(false),
    clockProblem = $state(false);
  const key = () => `actualBudget.reconciliation.v1.${budgetId}.${account.id}`;
  let stored: string | null = null;
  let mounted = false;
  function save(next: Draft | null) {
    if (localStorage.getItem(key()) !== stored)
      throw new Error(
        "Another tab changed this reconciliation. Reload before continuing.",
      );
    const raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(key());
    else localStorage.setItem(key(), raw);
    stored = raw;
    draft = next;
  }
  onMount(() => {
    try {
      stored = localStorage.getItem(key());
      if (stored) {
        const saved = JSON.parse(stored) as Draft;
        // Drafts created before date preferences used ISO exclusively.
        saved.dateFormat ??= "yyyy-MM-dd";
        if (
          typeof saved.epoch !== "string" ||
          typeof saved.target !== "string" ||
          typeof saved.date !== "string" ||
          !dateFormats.includes(saved.dateFormat) ||
          (saved.pending &&
            (!new RegExp(
              `^/budgets/${budgetId}/(accounts/${account.id}/reconciliation|transactions/[0-9a-f-]{36}/unlock)$`,
            ).test(saved.pending.path) ||
              saved.pending.method !== "POST" ||
              !saved.pending.key ||
              !validClock(saved.pending.edit_clock ?? "") ||
              !saved.pending.budgetEpoch))
        )
          throw new Error(
            "Saved reconciliation is unreadable. Keep browser data for recovery.",
          );
        draft = saved;
        void refresh();
      }
    } catch (e) {
      error = errorMessage(e);
      broken = true;
    }
    mounted = true;
    return () => {
      mounted = false;
    };
  });
  $effect(() => {
    snapshot;
    untrack(() => {
      if (mounted && draft && !draft.pending && !busy) void refresh();
    });
  });
  async function refresh() {
    if (disconnected || busy || draft?.pending) return;
    busy = true;
    error = "";
    try {
      if (draft && draft.epoch !== preferences.budgetEpoch)
        throw new Error(
          "This budget was restored. Keep pending work; exit reconciliation to compare a new statement against the current budget.",
        );
      const target = draft ? parseAmount(draft.target, draft.format) : null;
      const q = target === null ? "" : `?statement_balance=${target}`;
      const result = await request<{ data: Progress }>(
        `/budgets/${budgetId}/accounts/${account.id}/reconciliation${q}`,
      );
      if (!mounted) return;
      progress = result.data;
      observeVersions({ reconciliation: result.data.edit_clock });
      if (!draft)
        save({
          epoch: preferences.budgetEpoch!,
          format: captureAmountFormat(),
          target: formatAmountInput(result.data.statement_balance),
          date: today(),
          dateFormat: preferences.shared.date_format,
          pending: null,
        });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      if (mounted) busy = false;
    }
  }
  function edit(field: "target" | "date", value: string) {
    if (!draft || draft.pending) return;
    try {
      save({ ...draft, [field]: value });
      progress = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function send(action?: "lock" | "adjust" | "unlock", cancel = false) {
    if (busy || broken) return;
    busy = true;
    error = "";
    notice = "";
    let acknowledged = false;
    try {
      if (!draft?.pending) {
        if (draft && draft.epoch !== preferences.budgetEpoch)
          throw new Error(
            "This budget was restored. Exit reconciliation before starting a new change.",
          );
        if (disconnected || !preferences.budgetEpoch)
          throw new Error("Reconnect before reconciling.");
        let body: Record<string, unknown>, path: string;
        if (action === "unlock") {
          if (!unlocking) return;
          observeVersions(unlocking.field_versions);
          body = { reconciled_version: unlocking.field_versions?.reconciled };
          path = `/budgets/${budgetId}/transactions/${unlocking.id}/unlock`;
        } else {
          if (!draft || !progress)
            throw new Error("Compare the statement before continuing.");
          const date = parseDateInput(draft.date, draft.dateFormat);
          if (!validDate(date)) throw new Error(t("Enter a valid date."));
          body = {
            action,
            statement_balance: progress.statement_balance,
            snapshot: progress.snapshot,
            date,
            ...(action === "adjust" ? { transaction_id: uuid() } : {}),
          };
          path = `/budgets/${budgetId}/accounts/${account.id}/reconciliation`;
        }
        const pending: PendingMutation = {
          path,
          body,
          method: "POST",
          key: uuid(),
          edit_clock: nextEditClock(account.field_versions),
          budgetEpoch: preferences.budgetEpoch,
        };
        save({
          ...(draft ?? {
            epoch: preferences.budgetEpoch!,
            format: captureAmountFormat(),
            target: formatAmountInput(account.balance),
            date: today(),
            dateFormat: preferences.shared.date_format,
          }),
          pending,
          clockRejected: false,
          epochRejected: false,
        });
      }
      const pending = draft!.pending!;
      if (cancel) requestCancellation(pending);
      await mutate(pending, cancel);
      acknowledged = true;
      notice = cancel
        ? "The original change was already saved."
        : "Reconciliation change saved.";
      save({
        ...draft!,
        pending: null,
        clockRejected: false,
        epochRejected: false,
      });
      onunlocked();
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof LocalClockError) clockProblem = true;
      if (e instanceof RequestError && !e.uncertain && draft?.pending) {
        if (
          [
            "clock_skew",
            "clock_overflow",
            "invalid_clock",
            "budget_restored",
          ].includes(e.code ?? "")
        ) {
          try {
            save({
              ...draft,
              clockRejected: true,
              epochRejected: e.code === "budget_restored",
            });
          } catch (storage) {
            error = errorMessage(storage);
          }
        } else {
          try {
            save({
              ...draft,
              pending: null,
              clockRejected: false,
              epochRejected: false,
            });
          } catch (storage) {
            error = errorMessage(storage);
          }
          if (e.code === "operation_canceled") {
            error = "";
            notice = "Change canceled before saving.";
            onunlocked();
          }
        }
      }
    } finally {
      busy = false;
    }
    if (acknowledged) {
      await onchanged();
      await refresh();
    }
  }
  function resetClock() {
    try {
      resetCorrectedClock();
      clockProblem = false;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function close() {
    try {
      if (draft?.pending) return;
      save(null);
      progress = null;
      error = "";
      onunlocked();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  let last = $derived.by(() => {
    if (!account.last_reconciled) return null;
    const date = new Date(Number(account.last_reconciled));
    return formatDate(
      `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`,
    );
  });
</script>

<section
  class="reconciliation"
  aria-label={t("Reconciliation")}
  aria-busy={busy}
>
  {#if !draft && !unlocking}
    <div class="reconcile-heading">
      <button
        onclick={refresh}
        disabled={disconnected || disabled || busy || broken}
        >{t("Reconcile account")}</button
      >
      {#if last}<span class="hint">{t("Last reconciled")}: {last}</span>{/if}
    </div>
  {/if}
  {#if unlocking && !draft?.pending}
    <div class="unlock-confirmation">
      <p>
        {t("Unlock this reconciled transaction?")}
        {formatAmount(unlocking.amount)} · {unlocking.payee_name ??
          t("No payee")} · {formatDate(unlocking.date)}
      </p>
      <p class="hint">
        {t("Cleared status stays unchanged. Reconcile again to lock it.")}
      </p>
      <button
        onclick={() => send("unlock")}
        disabled={busy || disconnected || disabled}
        >{t("Confirm unlock")}</button
      >
      <button onclick={onunlocked} disabled={busy}>{t("Keep locked")}</button>
    </div>
  {/if}
  {#if draft}
    <h3>{t("Compare with your statement")}</h3>
    <p class="hint">
      {t(
        "Use a negative balance for credit cards and loans. Cleared entries across all dates are compared.",
      )}
    </p>
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void refresh();
      }}
    >
      <fieldset disabled={busy || !!draft.pending || disconnected || disabled}>
        <label for="reconcile-target"
          >{t("Statement balance")}<input
            id="reconcile-target"
            inputmode="decimal"
            value={draft.target}
            oninput={(e) => edit("target", e.currentTarget.value)}
            required
          /></label
        >
        <DateInput
          id="reconcile-date"
          label={t("Adjustment date")}
          bind:value={() => draft!.date, (value) => edit("date", value)}
          format={draft.dateFormat}
        />
        <button>{t("Compare statement")}</button>
      </fieldset>
    </form>
    {#if progress}
      <dl class="reconcile-balances">
        <div>
          <dt>{t("Statement")}</dt>
          <dd>{formatAmount(progress.statement_balance)}</dd>
        </div>
        <div>
          <dt>{t("Cleared")}</dt>
          <dd>{formatAmount(progress.balances.cleared)}</dd>
        </div>
        <div>
          <dt>{t("Difference")}</dt>
          <dd data-testid="reconciliation-difference">
            {formatAmount(progress.difference)}
          </dd>
        </div>
      </dl>
      <p role="status">
        {progress.difference === 0
          ? t("All reconciled. Lock cleared transactions to finish.")
          : t(
              "Match entries to your statement and mark them cleared, or create an adjustment.",
            )}
      </p>
      <p class="hint">
        {progress.ready_count}
        {t("cleared entries ready to lock")} · {progress.locked_count}
        {t("already locked")} · {progress.uncleared_count}
        {t("uncleared")}
      </p>
      <button
        class="primary"
        onclick={() => send(progress!.difference === 0 ? "lock" : "adjust")}
        disabled={busy || !!draft.pending || disconnected || disabled}
        >{progress.difference === 0
          ? t("Lock transactions")
          : t("Create reconciliation transaction")}</button
      >
    {/if}
    {#if draft.pending}
      <p role="status">
        {draft.epochRejected
          ? t(
              "This budget was restored. Cancel the saved change safely before starting reconciliation again.",
            )
          : draft.clockRejected
            ? t(
                "Correct the device clock, then retry unchanged or cancel safely before creating a new request.",
              )
            : t(
                "Saved request awaiting confirmation. Retry preserves the original change.",
              )}
      </p>
      <button onclick={() => send()} disabled={busy}
        >{t("Retry reconciliation")}</button
      >
      <button onclick={() => send(undefined, true)} disabled={busy}
        >{t("Cancel reconciliation change safely")}</button
      >
    {:else}<button onclick={close} disabled={busy}
        >{t("Exit reconciliation")}</button
      >{/if}
  {/if}
  {#if clockProblem}<button onclick={resetClock} disabled={busy}
      >{t("Clock corrected — resume editing")}</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
</section>

<style>
  .reconciliation {
    margin-block: 1rem;
    padding-block: 1rem;
    border-block: 1px solid var(--line);
  }
  .reconcile-heading,
  fieldset {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 1rem;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin-block: 1rem;
  }
  label {
    display: grid;
    gap: 0.5rem;
  }
  .reconcile-balances {
    display: flex;
    flex-wrap: wrap;
    gap: 2rem;
    margin-block: 1rem;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  h3 {
    margin-block: 0 1rem;
  }
  .hint {
    max-width: 70ch;
  }
  @media (max-width: 700px) {
    fieldset > label {
      width: 100%;
    }
    .reconcile-balances {
      gap: 1rem;
    }
  }
</style>
