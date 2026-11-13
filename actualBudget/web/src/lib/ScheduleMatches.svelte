<script lang="ts">
  import { onMount } from "svelte";
  import { request, errorMessage, type Account } from "./api";
  import type { Reference, Transaction } from "./register-api";
  import { formatAmount } from "./money";
  import type { Condition } from "./register-query";
  import type { Schedule, ScheduleMatches, SchedulePage } from "./schedules";
  let {
    budgetId,
    context,
    accounts,
    payees,
    onstage,
    onclose,
  }: {
    budgetId: string;
    context: {
      schedule: Schedule | null;
      conditions: Condition[];
      snapshot: string;
      epoch: string;
      linked: boolean;
    };
    accounts: Account[];
    payees: Reference[];
    onstage: (value: {
      action: "attach" | "link" | "unlink";
      ids: string[];
      transactions: Transaction[];
      schedule: Schedule | null;
      snapshot: string;
      epoch: string;
    }) => void;
    onclose: () => void;
  } = $props();
  let page = $state.raw<ScheduleMatches | null>(null),
    selected = $state.raw<Transaction[]>([]),
    targets = $state.raw<Schedule[]>([]),
    target = $state(""),
    nextTargets = $state<number | null>(0),
    offset = $state(0),
    busy = $state(false),
    error = $state("");
  const selectedIds = $derived(new Set(selected.map((row) => row.id)));
  async function load() {
    busy = true;
    error = "";
    try {
      const data = (
        await request<{ data: ScheduleMatches }>(
          context.linked
            ? `/budgets/${budgetId}/schedules/${context.schedule!.id}/transactions?snapshot=${context.snapshot}&offset=${offset}&limit=30`
            : `/budgets/${budgetId}/schedules/matches`,
          context.linked
            ? undefined
            : {
                snapshot: context.snapshot,
                conditions: context.conditions,
                offset,
                limit: 30,
              },
        )
      ).data;
      if (data.epoch !== context.epoch)
        throw new Error(
          "This budget was restored. Close these matches and reopen the current schedules.",
        );
      page = data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    targets = context.schedule ? [context.schedule] : [];
    target = context.schedule?.id ?? "";
    void load();
  });
  async function moreTargets() {
    if (nextTargets === null) return;
    busy = true;
    try {
      const data = (
        await request<{ data: SchedulePage }>(
          `/budgets/${budgetId}/schedules?limit=100&offset=${nextTargets}&snapshot=${context.snapshot}`,
        )
      ).data;
      const known = new Set(targets.map((s) => s.id));
      targets = [...targets, ...data.schedules.filter((s) => !known.has(s.id))];
      nextTargets = data.next_offset;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function select(row: Transaction, checked: boolean) {
    selected = checked
      ? [...selected, row]
      : selected.filter((s) => s.id !== row.id);
  }
  function stage(action: "attach" | "link" | "unlink") {
    if (!page || !selected.length) return;
    const schedule =
      action === "attach"
        ? null
        : action === "unlink"
          ? context.schedule
          : (targets.find((s) => s.id === target) ?? null);
    if (action !== "attach" && !schedule) {
      error = "Choose a schedule first.";
      return;
    }
    onstage({
      action,
      ids: selected.map((s) => s.id),
      transactions: structuredClone(selected),
      schedule,
      snapshot: context.snapshot,
      epoch: context.epoch,
    });
  }
</script>

<section aria-labelledby="schedule-match-title" class="matches">
  <header>
    <h2 id="schedule-match-title">
      {context.linked ? "Linked payments" : "Matching payments"}
    </h2>
    <button onclick={onclose}>Close payment matches</button>
  </header>
  <p>
    Review exact dates, amounts and status before selecting. Linking never
    merges or deletes payments. A paired leg shares its link with its
    counterpart; split allocations keep independent links.
  </p>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if page}<p>
      {page.total_count}
      {page.total_count === 1 ? "payment" : "payments"} · {selected.length} selected
      (maximum 500). Reconciled entries require unlocking before their graph can change.
    </p>
    <ul>
      {#each page.transactions as row (row.id)}<li>
          <label class="select"
            ><input
              type="checkbox"
              aria-label={`Select payment ${row.date} ${row.notes || row.payee_name || row.id}`}
              checked={selectedIds.has(row.id)}
              disabled={busy ||
                (selected.length >= 500 && !selectedIds.has(row.id))}
              onchange={(e) => select(row, e.currentTarget.checked)}
            /><span
              ><strong>{row.date} · {formatAmount(row.amount)}</strong><span
                >{row.account_name} · {row.payee_name ?? "No payee"} · {row.category_name ??
                  "No category"}</span
              ><span>{row.notes || "No notes"}</span><span
                >{row.is_parent
                  ? " · Split parent"
                  : row.parent_id
                    ? " · Split allocation"
                    : ""}{row.transfer_id
                  ? ` · Paired with ${accounts.find((a) => a.id === row.transfer_account_id)?.name ?? "another account"}`
                  : ""}{row.schedule_id ? " · Already linked" : ""}</span
              ></span
            ></label
          >{#if row.children?.length}<details>
              <summary>{row.children.length} allocations</summary>
              <ul>
                {#each row.children as child (child.id)}<li>
                    {formatAmount(child.amount)} · {child.payee_name ??
                      payees.find((p) => p.id === child.payee_id)?.name ??
                      "No payee"} · {child.category_name ?? "No category"} · {child.notes ||
                      "No notes"}{child.transfer_id
                      ? " · Transfer allocation"
                      : ""}
                  </li>{/each}
              </ul>
            </details>{/if}
        </li>{:else}<li>
          No payments in this page match the captured conditions.
        </li>{/each}
    </ul>
    <div class="toolbar">
      <button
        disabled={busy || offset === 0}
        onclick={() => {
          offset = Math.max(0, offset - 30);
          void load();
        }}>Previous payments</button
      ><button
        disabled={busy || page.next_offset === null}
        onclick={() => {
          offset = page!.next_offset!;
          void load();
        }}>Next payments</button
      >
    </div>
    {#if context.schedule}<label
        >Link selected payments to<select bind:value={target} disabled={busy}
          >{#each targets as row (row.id)}<option value={row.id}
              >{row.name ??
                payees.find((p) => p.id === row.payee_id)?.name ??
                "Unnamed schedule"} · {accounts.find(
                (a) => a.id === row.account_id,
              )?.name ?? "Account"} · {row.next_date ?? "No next date"}</option
            >{/each}</select
        ></label
      >{#if nextTargets !== null}<button onclick={moreTargets} disabled={busy}
          >Load more target schedules</button
        >{/if}
      <div class="toolbar">
        <button
          onclick={() => stage("link")}
          disabled={busy || !selected.length}>Review selected links</button
        >{#if context.linked}<button
            onclick={() => stage("unlink")}
            disabled={busy || !selected.length}>Review selected unlinks</button
          >{/if}
      </div>
    {:else}<button
        onclick={() => stage("attach")}
        disabled={busy || !selected.length}>Use selected payments</button
      >{/if}{/if}
</section>

<style>
  .matches {
    margin: 2rem 0;
    padding: 1rem;
    border: 1px solid var(--border);
  }
  header,
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    gap: 0.7rem;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
  }
  .toolbar {
    justify-content: start;
    margin: 1rem 0;
  }
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    padding: 1rem 0;
    border-bottom: 1px solid var(--border);
  }
  .select {
    display: flex;
    gap: 0.8rem;
    align-items: start;
    min-height: 44px;
  }
  .select > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .select span span {
    display: block;
    margin: 0.35rem 0;
    font-weight: 400;
  }
  input[type="checkbox"] {
    width: 1.25rem;
    min-width: 1.25rem;
    height: 1.25rem;
    min-height: 1.25rem;
    padding: 0;
    flex: 0 0 1.25rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
  }
  details {
    margin: 0.75rem 0;
  }
  details li {
    font-size: 0.95em;
  }
  select {
    max-width: 100%;
  }
  @media (max-width: 40rem) {
    .matches {
      padding: 0.75rem;
    }
    .toolbar > * {
      flex: 1 1 12rem;
    }
  }
</style>
