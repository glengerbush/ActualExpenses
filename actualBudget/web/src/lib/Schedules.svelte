<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onMount } from "svelte";
  import { request, errorMessage, uuid, type Account } from "./api";
  import {
    references,
    mutate,
    requestCancellation,
    type Reference,
  } from "./register-api";
  import { historyOwner } from "./history-owner";
  import { nextEditClock, resetCorrectedClock } from "./edit-clock";
  import { captureAmountFormat, formatAmount, today } from "./money";
  import {
    schedulesKey,
    readScheduleDraft,
    type Schedule,
    type SchedulePage,
    type ScheduleDraft,
  } from "./schedules";
  import type { Condition } from "./register-query";
  import type { RuleAction as Action } from "./rules";
  import RegisterCondition from "./RegisterCondition.svelte";
  import RuleAction from "./RuleAction.svelte";
  import DateInput from "./DateInput.svelte";
  import ScheduleMatches from "./ScheduleMatches.svelte";
  import type { ScheduleDiscovery, ScheduleSuggestion } from "./schedules";
  import type { Transaction } from "./register-api";
  import { foregroundSchedules, readScheduleJob } from "./schedule-foreground";
  let {
    budgetId,
    disconnected,
    onclose,
    onchanged,
  }: {
    budgetId: string;
    disconnected: boolean;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  let view = $state.raw<SchedulePage | null>(null),
    draft = $state<ScheduleDraft | null>(null),
    accounts = $state.raw<Account[]>([]),
    payees = $state.raw<Reference[]>([]),
    categories = $state.raw<Reference[]>([]),
    groups = $state.raw<Reference[]>([]);
  let busy = $state(false),
    error = $state(""),
    notice = $state(""),
    broken = $state(false),
    offset = $state(0),
    filter = $state("");
  let automaticPending = $state(false),
    automaticError = $state("");
  let dates = $state<string[]>([]),
    details = $state<{
      schedule_id: string;
      occurrences: {
        id: string;
        date: string;
        status: string;
        transaction_id: string | null;
      }[];
      next_from: string | null;
      snapshot: string;
    } | null>(null);
  type MatchContext = {
    schedule: Schedule | null;
    conditions: Condition[];
    snapshot: string;
    epoch: string;
    linked: boolean;
  };
  let matchingContext = $state.raw<MatchContext | null>(null),
    discovery = $state.raw<ScheduleDiscovery | null>(null),
    discoveryOffset = $state(0);
  let checkpoint: string | null = null;
  let conditions = $derived((draft?.body.conditions ?? []) as Condition[]),
    actions = $derived((draft?.body.actions ?? []) as Action[]);
  let editing = $derived(
    ["create", "update"].includes(String(draft?.body.action)),
  );
  function save(next: ScheduleDraft | null) {
    if (localStorage.getItem(schedulesKey(budgetId)) !== checkpoint)
      throw new Error(
        "Another tab changed this schedule draft. Reload to review its saved work.",
      );
    const raw = next ? JSON.stringify(next) : null;
    if (raw === null) localStorage.removeItem(schedulesKey(budgetId));
    else localStorage.setItem(schedulesKey(budgetId), raw);
    checkpoint = raw;
    draft = next;
  }
  function field(key: string, value: unknown) {
    if (!draft || draft.pending || busy) return;
    try {
      const body = { ...$state.snapshot(draft.body), [key]: value };
      const next = { ...$state.snapshot(draft), body };
      if (key === "conditions") {
        const hadSelection =
          !!next.transactions?.length ||
          (Array.isArray(body.transaction_ids) &&
            body.transaction_ids.length > 0);
        delete body.transaction_ids;
        delete next.transactions;
        matchingContext = null;
        notice = hadSelection
          ? "Payment selection cleared. Match existing payments again to link them."
          : "";
      }
      save(next);
      dates = [];
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function refresh() {
    if (disconnected) {
      error =
        "Reconnect to load confirmed schedules. Saved work stays on this device.";
      return;
    }
    busy = true;
    error = "";
    try {
      const loaded = (
        await request<{ data: SchedulePage }>(
          `/budgets/${budgetId}/schedules?limit=50&offset=${offset}${filter ? "&account_id=" + filter : ""}`,
        )
      ).data;
      const [a, p, c, g] = await Promise.all([
        request<{ data: Account[] }>(
          `/budgets/${budgetId}/accounts?snapshot=${loaded.snapshot}`,
        ),
        references(budgetId, "payees", { includeTransfers: true }),
        references(budgetId, "categories"),
        references(budgetId, "category-groups"),
      ]);
      view = loaded;
      const held = readScheduleJob(budgetId);
      automaticPending = !!held;
      automaticError = held?.review ?? "";
      accounts = a.data;
      payees = p;
      categories = c;
      groups = g;
      details = null;
      matchingContext = null;
      discovery = null;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    try {
      draft = readScheduleDraft(budgetId);
      checkpoint = localStorage.getItem(schedulesKey(budgetId));
    } catch (e) {
      broken = true;
      error = errorMessage(e);
      return;
    }
    void refresh();
  });
  function open(
    action: string,
    row?: Schedule,
    extra: Record<string, unknown> = {},
  ) {
    if (!view || busy || broken) return;
    if (draft) {
      error = "Finish or discard the saved schedule work first.";
      return;
    }
    try {
      const body: Record<string, unknown> = {
        action,
        ...(row
          ? {
              id: row.id,
              ...(["post", "skip"].includes(action)
                ? { occurrence_date: row.next_date }
                : {}),
            }
          : {}),
        ...extra,
      };
      if (action === "create")
        Object.assign(body, {
          id: uuid(),
          name: "",
          posts_transaction: false,
          conditions: [
            {
              field: "date",
              op: "is",
              value: {
                start: view.today,
                frequency: "monthly",
                interval: 1,
                endMode: "never",
              },
            },
            {
              field: "account",
              op: "is",
              value:
                accounts.find((a) => !a.closed && !a.off_budget)?.id ?? null,
            },
            { field: "amount", op: "is", value: -100 },
          ],
          actions: [],
        });
      if (action === "update" && row)
        Object.assign(body, {
          name: row.name ?? "",
          posts_transaction: row.posts_transaction,
          completed: row.completed,
          conditions: row.conditions,
          actions: row.actions.filter(
            (a) => a.op !== "link-schedule" || a.value !== row.id,
          ),
          custom_upcoming_length: row.custom_upcoming_length,
        });
      save({
        version: 1,
        epoch: view.epoch,
        snapshot: view.snapshot,
        body,
        original: row ? structuredClone(row) : null,
        format: captureAmountFormat(),
        pending: null,
      });
      dates = [];
      notice = "";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function discard() {
    if (draft?.pending || busy) return;
    try {
      save(null);
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function previewDates() {
    if (!draft) return;
    busy = true;
    error = "";
    try {
      const date = conditions.find((c) => c.field === "date");
      if (!date) throw new Error("Add a date condition first.");
      dates = (
        await request<{ data: string[] }>("/schedules/preview-dates", {
          config: date.value,
          from: view?.today ?? today(),
          count: 12,
        })
      ).data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function send(cancel = false) {
    if (!draft || busy || broken) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (disconnected)
        throw new Error(
          "Reconnect before sending. Your schedule work is saved.",
        );
      if (!draft.pending) {
        if (!view || view.epoch !== draft.epoch)
          throw new Error(
            "This budget was restored. Keep any needed values, discard this draft and reopen the current schedule.",
          );
        const body: Record<string, unknown> = {
          ...$state.snapshot(draft.body),
          snapshot: draft.snapshot,
        };
        if (draft.original && body.action === "update") {
          for (const key of [
            "name",
            "posts_transaction",
            "completed",
            "conditions",
            "actions",
            "custom_upcoming_length",
          ]) {
            const original =
              key === "actions"
                ? draft.original.actions.filter(
                    (a) =>
                      a.op !== "link-schedule" ||
                      a.value !== draft?.original?.id,
                  )
                : draft.original[key as keyof Schedule];
            if (
              JSON.stringify(body[key as keyof typeof body]) ===
              JSON.stringify(original)
            )
              delete body[key as keyof typeof body];
          }
          if (Array.isArray(body.actions))
            body.actions = [
              { op: "link-schedule", value: draft.original.id },
              ...body.actions,
            ];
          if (body.custom_upcoming_length === null) {
            delete body.custom_upcoming_length;
            Object.assign(body, { use_global_upcoming_length: true });
          }
        }
        save({
          ...$state.snapshot(draft),
          pending: {
            path: `/budgets/${budgetId}/schedules/commands`,
            method: "POST",
            body,
            key: uuid(),
            historyOwner: await historyOwner(),
            edit_clock: nextEditClock(),
            budgetEpoch: draft.epoch,
          },
        });
      }
      if (cancel) requestCancellation(draft.pending!);
      save($state.snapshot(draft));
      try {
        await mutate(draft.pending!, cancel);
      } catch (e) {
        if (
          e instanceof Error &&
          "code" in e &&
          e.code === "operation_canceled"
        ) {
          resetCorrectedClock();
          save({ ...$state.snapshot(draft), pending: null });
          notice =
            "The saved request was cancelled. Review its retained fields before preparing a new request.";
          return;
        }
        throw e;
      }
      save(null);
      notice = cancel
        ? "The original schedule request was already confirmed."
        : "Schedule change saved.";
      await refresh();
      await onchanged();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function recoverAutomatic(mode: "retry" | "cancel") {
    if (busy) return;
    busy = true;
    error = "";
    notice = "";
    try {
      const committed = await foregroundSchedules(budgetId, mode);
      notice = committed
        ? "Automatic schedule entries confirmed."
        : "Saved automatic schedule request cancelled.";
      await refresh();
      await onchanged();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function matchPayments(row?: Schedule, linked = false) {
    if (!view || busy) return;
    if (
      draft &&
      (!editing || draft.pending || draft.body.action !== "create")
    ) {
      error = "Finish the saved schedule work before matching another entry.";
      return;
    }
    matchingContext = {
      schedule: row ?? null,
      conditions: structuredClone(
        row?.conditions ?? $state.snapshot(conditions),
      ),
      snapshot: row ? view.snapshot : (draft?.snapshot ?? view.snapshot),
      epoch: row ? view.epoch : (draft?.epoch ?? view.epoch),
      linked,
    };
  }
  function stagePayments(selection: {
    action: "attach" | "link" | "unlink";
    ids: string[];
    transactions: Transaction[];
    schedule: Schedule | null;
    snapshot: string;
    epoch: string;
  }) {
    if (selection.action === "attach") {
      if (
        !draft ||
        draft.pending ||
        draft.body.action !== "create" ||
        draft.snapshot !== selection.snapshot
      ) {
        error =
          "Reopen the original schedule draft before selecting these payments.";
        return;
      }
      try {
        save({
          ...$state.snapshot(draft),
          body: {
            ...$state.snapshot(draft.body),
            transaction_ids: selection.ids,
          },
          transactions: selection.transactions,
        });
        matchingContext = null;
        notice = `${selection.ids.length} payments selected. Save the schedule to link them atomically.`;
      } catch (e) {
        error = errorMessage(e);
      }
      return;
    }
    if (draft) {
      error = "Finish the saved schedule work first.";
      return;
    }
    if (!selection.schedule) return;
    try {
      save({
        version: 1,
        epoch: selection.epoch,
        snapshot: selection.snapshot,
        body: {
          action: selection.action,
          id: selection.schedule.id,
          transaction_ids: selection.ids,
        },
        original: selection.schedule,
        transactions: selection.transactions,
        format: captureAmountFormat(),
        pending: null,
      });
      matchingContext = null;
      notice = "";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function discover(offset = 0) {
    if (!view || busy) return;
    busy = true;
    error = "";
    try {
      discovery = (
        await request<{ data: ScheduleDiscovery }>(
          `/budgets/${budgetId}/schedules/discovery?snapshot=${view.snapshot}&offset=${offset}&limit=20${filter ? "&account_id=" + filter : ""}`,
        )
      ).data;
      discoveryOffset = offset;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function suggestion(row: ScheduleSuggestion) {
    if (!discovery || draft || busy) return;
    try {
      save({
        version: 1,
        epoch: discovery.epoch,
        snapshot: discovery.snapshot,
        body: {
          action: "create",
          id: row.id,
          name: "",
          posts_transaction: false,
          conditions: row.conditions,
          actions: [],
        },
        original: null,
        format: captureAmountFormat(),
        pending: null,
      });
      matchingContext = {
        schedule: null,
        conditions: structuredClone(row.conditions),
        snapshot: discovery.snapshot,
        epoch: discovery.epoch,
        linked: false,
      };
      discovery = null;
      notice =
        "Review the suggested conditions and choose existing payments to link. No money is posted when creating a manual schedule.";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function occurrences(row: Schedule, from?: string) {
    if (!view) return;
    busy = true;
    error = "";
    try {
      details = (
        await request<{ data: NonNullable<typeof details> }>(
          `/budgets/${budgetId}/schedules/${row.id}/occurrences?limit=30&snapshot=${view.snapshot}${from ? "&from=" + from : ""}`,
        )
      ).data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  const horizons = [
    ["7", "One week"],
    ["14", "Two weeks"],
    ["currentMonth", "This month"],
    ["oneMonth", "One month"],
    ["2-month", "Two months"],
    ["3-month", "Three months"],
    ["6-month", "Six months"],
    ["1-year", "One year"],
  ];
</script>

<section class="schedules" aria-labelledby="schedule-title">
  <header>
    <div>
      <h1 id="schedule-title">{t("Scheduled transactions")}</h1>
      <p>
        Plan recurring bills, income and transfers. Only confirmed entries
        affect account balances.
      </p>
    </div>
  </header>
  {#if error}<p role="alert" class="error">{error}</p>{/if}{#if notice}<p
      role="status"
    >
      {notice}
    </p>{/if}
  <div class="toolbar">
    <button onclick={refresh} disabled={busy || disconnected}
      >Refresh schedules</button
    ><button
      onclick={() => open("create")}
      disabled={busy || !!draft || broken || !view}>Create schedule</button
    ><button
      onclick={() => open("catch-up", undefined, { through: view?.today })}
      disabled={busy || !!draft || !view}>Post overdue automatic entries</button
    ><button
      onclick={() =>
        open("settings", undefined, { upcoming_length: view?.upcoming_length })}
      disabled={busy || !!draft || !view}>Upcoming settings</button
    ><button onclick={() => discover()} disabled={busy || !!draft || !view}
      >Find recurring payments</button
    >
  </div>
  {#if automaticPending}<section aria-label="Automatic schedule recovery">
      <h2>Automatic schedule work needs confirmation</h2>
      <p>
        {automaticError ||
          "The original request is saved. Retry checks its original outcome before sending it again."}
      </p>
      <p>
        Expense capture remains available. For a long overdue period, cancel the
        saved command and use an earlier “Post through” date below.
      </p>
      <div class="toolbar">
        <button onclick={() => recoverAutomatic("retry")} disabled={busy}
          >Retry automatic schedule request</button
        ><button onclick={() => recoverAutomatic("cancel")} disabled={busy}
          >Cancel automatic schedule request</button
        >
      </div>
    </section>{/if}
  {#if draft}<section class="editor" aria-labelledby="schedule-editor">
      <h2 id="schedule-editor">
        {editing
          ? draft.original
            ? "Edit schedule"
            : "Create schedule"
          : draft.body.action === "catch-up"
            ? "Post overdue automatic entries"
            : draft.body.action === "post"
              ? "Post occurrence"
              : draft.body.action === "skip"
                ? "Skip next occurrence"
                : draft.body.action === "delete"
                  ? "Delete schedule"
                  : draft.body.action === "link"
                    ? "Link selected payments"
                    : draft.body.action === "unlink"
                      ? "Unlink selected payments"
                      : "Upcoming settings"}
      </h2>
      {#if draft.pending}<p>
          This original request is saved. Retry checks the same request;
          cancellation requires a confirmed server outcome.
        </p>{/if}
      <fieldset disabled={busy || !!draft.pending}>
        {#if editing}<label
            >Schedule name<input
              value={String(draft.body.name ?? "")}
              oninput={(e) => field("name", e.currentTarget.value)}
              maxlength="255"
            /></label
          ><label class="inline"
            ><input
              type="checkbox"
              checked={draft.body.posts_transaction === true}
              onchange={(e) =>
                field("posts_transaction", e.currentTarget.checked)}
            />Automatically post when due</label
          >{#if draft.original}<label class="inline"
              ><input
                type="checkbox"
                checked={draft.body.completed === true}
                onchange={(e) => field("completed", e.currentTarget.checked)}
              />Completed</label
            >{/if}
          <h3>Conditions</h3>
          <p>
            Include one date and an amount. Negative amounts are payments;
            positive amounts are income.
          </p>
          {#each conditions as condition, index (index)}{#if condition.field !== "account"}<div
                class="condition"
              >
                <label
                  >Condition field {index + 1}<select
                    value={condition.field}
                    onchange={(e) =>
                      field(
                        "conditions",
                        conditions.map((c, i) =>
                          i === index
                            ? {
                                field: e.currentTarget.value,
                                op: "is",
                                value:
                                  e.currentTarget.value === "amount"
                                    ? 0
                                    : e.currentTarget.value === "date"
                                      ? today()
                                      : null,
                              }
                            : c,
                        ),
                      )}
                    >{#if condition.field === "notes"}<option
                        value="notes"
                        disabled>Historical notes</option
                      >{/if}{#each ["date", "payee", "amount", "category"] as name (name)}<option
                        value={name}>{name}</option
                      >{/each}</select
                  ></label
                ><label
                  >Condition operator {index + 1}<select
                    value={condition.op}
                    onchange={(e) =>
                      field(
                        "conditions",
                        conditions.map((c, i) =>
                          i === index
                            ? {
                                ...c,
                                op: e.currentTarget.value,
                                value:
                                  e.currentTarget.value === "isbetween"
                                    ? { num1: 0, num2: 0 }
                                    : c.value,
                              }
                            : c,
                        ),
                      )}
                    ><option value="is">Exactly</option
                    >{#if ["date", "amount"].includes(condition.field)}<option
                        value="isapprox">Approximately</option
                      >{/if}{#if condition.field === "amount"}<option
                        value="isbetween">Between</option
                      >{/if}</select
                  ></label
                ><RegisterCondition
                  {condition}
                  {index}
                  {accounts}
                  {payees}
                  {categories}
                  {groups}
                  rules={true}
                  posting={true}
                  amountFormat={draft.format}
                  onchange={(value) =>
                    field(
                      "conditions",
                      conditions.map((c, i) => (i === index ? value : c)),
                    )}
                /><button
                  onclick={() =>
                    field(
                      "conditions",
                      conditions.filter((_, i) => i !== index),
                    )}>Remove condition {index + 1}</button
                >
              </div>{/if}{/each}<button
            onclick={() =>
              field("conditions", [
                ...conditions,
                { field: "payee", op: "is", value: null },
              ])}>{t("Add condition")}</button
          >
          {#if draft.body.transfer_account_id}<label
              >Transfer destination<select
                value={String(draft.body.transfer_account_id ?? "")}
                onchange={(e) =>
                  field(
                    "transfer_account_id",
                    e.currentTarget.value || undefined,
                  )}
                ><option value="">Use payee condition</option
                >{#each accounts.filter((a) => !a.closed) as a (a.id)}<option
                    value={a.id}>{a.name}</option
                  >{/each}</select
              ></label
            >
          {/if}
          {#if draft.body.action === "create"}<button
              onclick={() => matchPayments()}
              disabled={busy}>Match existing payments</button
            >{/if}
          {#if draft.body.transaction_ids}<p>
              {(draft.body.transaction_ids as string[]).length} existing payments
              will link when this schedule is saved. Linking does not merge or delete
              them.
            </p>{/if}
          <h3>Additional rule actions</h3>
          {#each actions as action, index (index)}<div class="condition">
              <RuleAction
                {action}
                {index}
                {accounts}
                {payees}
                {categories}
                schedules={view?.schedules.map((s) => ({
                  id: s.id,
                  name: s.name ?? "Unnamed schedule",
                })) ?? []}
                format={draft.format}
                onchange={(value) =>
                  field(
                    "actions",
                    actions.map((a, i) => (i === index ? value : a)),
                  )}
              /><button
                onclick={() =>
                  field(
                    "actions",
                    actions.filter((_, i) => i !== index),
                  )}>Remove action {index + 1}</button
              >
            </div>{/each}<button
            onclick={() =>
              field("actions", [
                ...actions,
                { op: "set", field: "category", value: null },
              ])}>{t("Add action")}</button
          >
          <label
            >Upcoming preview<select
              value={String(draft.body.custom_upcoming_length ?? "")}
              onchange={(e) =>
                field("custom_upcoming_length", e.currentTarget.value || null)}
              ><option value="">Use budget setting</option
              >{#each horizons as [value, label] (value)}<option {value}
                  >{label}</option
                >{/each}</select
            ></label
          >
          <button onclick={previewDates}>Preview calendar</button
          >{#if dates.length}<ol class="dates">
              {#each dates as date, index (index)}<li>{date}</li>{/each}
            </ol>{/if}
        {:else if draft.body.action === "catch-up"}<p>
            Posts due automatic entries through this day, up to500 new
            occurrences atomically. If more are overdue, choose an earlier day,
            confirm that batch, then continue. Pending manual occurrences are
            not posted.
          </p>
          <DateInput
            id="schedule-through"
            label="Post through"
            bind:value={
              () => String(draft?.body.through ?? ""),
              (value) => field("through", value)
            }
          />
        {:else if draft.body.action === "settings"}<label
            >Upcoming preview<select
              value={String(draft.body.upcoming_length ?? "7")}
              onchange={(e) => field("upcoming_length", e.currentTarget.value)}
              >{#each horizons as [value, label] (value)}<option {value}
                  >{label}</option
                >{/each}</select
            ></label
          >
        {:else if ["link", "unlink"].includes(String(draft.body.action))}<p>
            {(draft.body.transaction_ids as string[]).length} selected payments ·
            {draft.original?.name ?? "Schedule"}. Paired legs change their links
            together. Amounts, dates and categories are retained.
          </p>
        {:else}<p>
            {draft.original?.name ?? "Scheduled transaction"} · {String(
              draft.body.occurrence_date ?? draft.original?.next_date ?? "",
            )}
          </p>
          {#if draft.body.action === "post"}<label class="inline"
              ><input
                type="checkbox"
                checked={draft.body.today === true}
                onchange={(e) => field("today", e.currentTarget.checked)}
              />Use today’s date</label
            >{:else if draft.body.action === "delete"}<p>
              Stops future occurrences and removes the linked rule. Existing
              transactions remain.
            </p>{:else}<p>
              Advances to the next occurrence when one exists. A final or
              one-time occurrence cannot be skipped.
            </p>{/if}{/if}
      </fieldset>
      {#if draft.transactions?.length}<details open>
          <summary>Captured payment selection</summary>
          <ul>
            {#each draft.transactions as row (row.id)}<li>
                {row.date} · {formatAmount(row.amount)} · {row.account_name} · {row.payee_name ??
                  "No payee"} · {row.category_name ??
                  "No category"}{row.transfer_id ? " · Paired entry" : ""}
              </li>{/each}
          </ul>
        </details>{/if}
      <div class="toolbar">
        <button class="primary" onclick={() => send()} disabled={busy || broken}
          >{draft.pending
            ? "Retry original request"
            : editing
              ? "Save schedule"
              : "Confirm schedule change"}</button
        >{#if draft.pending}<button onclick={() => send(true)} disabled={busy}
            >Cancel saved request</button
          >{:else}<button onclick={discard} disabled={busy}
            >Discard draft</button
          >{/if}
      </div>
    </section>{/if}
  {#if matchingContext}{#key matchingContext}<ScheduleMatches
        {budgetId}
        context={matchingContext}
        {accounts}
        {payees}
        onstage={stagePayments}
        onclose={() => (matchingContext = null)}
      />{/key}{/if}
  {#if discovery}<section aria-labelledby="schedule-discovery-title">
      <h2 id="schedule-discovery-title">Suggested recurring payments</h2>
      <p>
        Patterns from confirmed history as of {discovery.as_of}. Review a
        suggestion and its matching payments before creating a schedule. Nothing
        is posted by this read.
      </p>
      <ul>
        {#each discovery.suggestions as row (row.id)}<li>
            <strong
              >{payees.find((p) => p.id === row.payee_id)?.name ?? "No payee"} · {formatAmount(
                row.amount,
              )}</strong
            >
            <p>
              {accounts.find((a) => a.id === row.account_id)?.name ?? "Account"} ·
              {String(row.date.frequency)}{row.date.interval
                ? ` every ${String(row.date.interval)}`
                : ""} · Starts {String(row.date.start)}
            </p>
            <button onclick={() => suggestion(row)} disabled={busy || !!draft}
              >Review suggestion</button
            >
          </li>{:else}<li>
            No recurring pattern was found in the confirmed history.
          </li>{/each}
      </ul>
      <div class="toolbar">
        <button
          disabled={busy || discoveryOffset === 0}
          onclick={() => discover(Math.max(0, discoveryOffset - 20))}
          >Previous suggestions</button
        ><button
          disabled={busy || discovery.next_offset === null}
          onclick={() => discover(discovery!.next_offset!)}
          >Next suggestions</button
        >
      </div>
    </section>{/if}
  {#if view}<div class="schedule-list">
      {#each view.schedules as row (row.id)}<article>
          <div>
            <h2>
              {row.name ??
                payees.find((p) => p.id === row.payee_id)?.name ??
                "Unnamed schedule"}
            </h2>
            <p>
              {row.next_date ?? "No next date"} · {row.status}
              · {row.posts_transaction ? "Automatic" : "Manual"}
            </p>
            {#if typeof row.amount === "number"}<p class="amount">
                {formatAmount(row.amount)}
              </p>{/if}
          </div>
          <div class="toolbar">
            <button
              onclick={() => open("update", row)}
              disabled={!!draft || busy}
              >{t("Edit")} {row.name ?? "schedule"}</button
            ><button onclick={() => occurrences(row)} disabled={busy}
              >Occurrences</button
            ><button
              onclick={() => matchPayments(row)}
              disabled={busy || !!draft}>Match payments</button
            ><button
              onclick={() => matchPayments(row, true)}
              disabled={busy || !!draft}>Linked payments</button
            >{#if !row.completed}<button
                onclick={() =>
                  open("post", row, { occurrence_date: row.next_date })}
                disabled={!!draft ||
                  busy ||
                  row.status === "paid" ||
                  !row.next_date}>Post now</button
              ><button
                onclick={() => open("skip", row)}
                disabled={!!draft || busy || !row.next_date}>Skip next</button
              >{/if}<button
              onclick={() => open("delete", row)}
              disabled={!!draft || busy}>{t("Delete")}</button
            >
          </div>
        </article>{:else}<p>
          No schedules in this account. Create a schedule to plan a bill, income
          or transfer.
        </p>{/each}
    </div>
    <div class="toolbar">
      <button
        disabled={busy || offset === 0}
        onclick={() => {
          offset = Math.max(0, offset - 50);
          void refresh();
        }}>Previous schedules</button
      ><button
        disabled={busy || view.next_offset === null}
        onclick={() => {
          offset = view!.next_offset!;
          void refresh();
        }}>Next schedules</button
      >
    </div>
    {#if details}<section>
        <h2>Occurrences</h2>
        <ol>
          {#each details.occurrences as occurrence (occurrence.id)}<li>
              {occurrence.date} · {occurrence.status}{occurrence.transaction_id
                ? " · Confirmed transaction"
                : ""}
            </li>{/each}
        </ol>
        {#if details.next_from}<button
            disabled={busy}
            onclick={() => {
              const row = view?.schedules.find(
                (s) => s.id === details?.schedule_id,
              );
              if (row) void occurrences(row, details!.next_from!);
            }}>More occurrences</button
          >{/if}
      </section>{/if}{/if}
</section>

<style>
  .inline {
    min-height: 44px;
  }
  .inline input[type="checkbox"] {
    width: 1.25rem;
    height: 1.25rem;
    min-height: 1.25rem;
    padding: 0;
    flex: 0 0 1.25rem;
  }

  .schedules {
    max-width: 78rem;
  }
  header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 1rem;
  }
  header p {
    max-width: 65ch;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
    margin: 1rem 0;
  }
  .editor {
    margin: 1.5rem 0;
    padding: 1rem;
    border: 1px solid var(--border);
  }
  fieldset {
    border: 0;
    padding: 0;
    min-width: 0;
  }
  label {
    display: grid;
    gap: 0.35rem;
    max-width: 32rem;
    margin: 0.75rem 0;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }
  .inline input {
    width: auto;
  }
  .condition {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.75rem;
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
    margin-bottom: 1rem;
  }
  .condition :global(label) {
    min-width: 10rem;
    flex: 1;
  }
  .schedule-list article {
    padding: 1.2rem 0;
    border-top: 1px solid var(--border);
  }
  .schedule-list h2 {
    font-size: 1.2rem;
    margin: 0;
  }
  .schedule-list p {
    margin: 0.35rem 0;
  }
  .amount {
    font-variant-numeric: tabular-nums;
  }
  .dates {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 2rem;
  }
  select,
  input {
    max-width: 100%;
    min-width: 0;
  }
  @media (max-width: 40rem) {
    header {
      flex-direction: column;
    }
    .condition {
      display: block;
    }
    .toolbar > * {
      flex: 1 1 12rem;
    }
    .editor {
      padding: 0.75rem;
    }
  }
</style>
