<script lang="ts">
  import { historyOwner } from "./history-owner";
  import { onMount, tick } from "svelte";
  import {
    uuid,
    errorMessage,
    RequestError,
    request,
    type Account,
  } from "./api";
  import {
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import {
    captureAmountFormat,
    formatAmount,
    formatAmountInput,
    parseAmount,
  } from "./money";
  import { mutate, type Transaction, type Reference } from "./register-api";
  import {
    readSplit,
    writeSplit,
    type SplitDraft,
    SplitOwnershipError,
  } from "./split-storage";
  import { t } from "./i18n.svelte";
  let {
    budgetId,
    accounts,
    payees,
    categories,
    disconnected,
    active = $bindable(false),
    oncomplete,
  }: {
    budgetId: string;
    accounts: Account[];
    payees: Reference[];
    categories: Reference[];
    disconnected: boolean;
    active?: boolean;
    oncomplete: () => Promise<void>;
  } = $props();
  let draft = $state<SplitDraft | null>(null);
  let busy = $state(false);
  let error = $state("");
  let recovery = $state(false);
  let snapshotChanged = $state(false);
  let savingDraft = $state(false);
  let heading = $state<HTMLHeadingElement>();
  async function focusEditor() {
    await tick();
    heading?.focus();
  }
  const pageSize = 20;
  let page = $state(0);
  let jumpTo = $state("1");
  let jumpIndex = $derived(Number(jumpTo));
  let pageStart = $derived(
    Math.min(
      page * pageSize,
      Math.max(
        0,
        Math.floor(((draft?.children.length ?? 1) - 1) / pageSize) * pageSize,
      ),
    ),
  );
  let originalIds = $derived(
    new Set(draft?.parent.children?.map((child) => child.id) ?? []),
  );
  let saved: SplitDraft | null = null;
  let writes: Promise<void> = Promise.resolve();
  onMount(() => {
    void recover();
  });
  async function recover() {
    busy = true;
    try {
      saved = await readSplit(budgetId);
      draft = saved;
      active = !!draft;
      recovery = false;
      error = "";
    } catch (e) {
      error = errorMessage(e);
      recovery = true;
      active = true;
    } finally {
      busy = false;
    }
  }
  export async function open(parent: Transaction) {
    if (busy) return;
    active = true;
    snapshotChanged = false;
    page = 0;
    jumpTo = "1";
    busy = true;
    error = "";
    try {
      const existing = await readSplit(budgetId);
      if (existing) {
        saved = existing;
        draft = existing;
        active = true;
        await focusEditor();
        return;
      }
      const current = await request<{
        data: Transaction;
        snapshot: string;
        epoch: string;
        reconciliation_token: string;
        transfer_protection_token: string;
        transfer_protection_clock: string;
        transfer_protected_count: number;
        transfer_connected: boolean;
      }>(`/budgets/${budgetId}/transactions/${parent.id}/splits`);
      parent = current.data;
      const format = captureAmountFormat();
      const children = (
        parent.children?.length
          ? parent.children
          : [
              { ...parent, id: uuid(), amount: 0 },
              { ...parent, id: uuid(), amount: 0 },
            ]
      ).map((c) => ({
        id: c.id,
        amount: formatAmountInput(c.amount, format),
        payee_id:
          (parent.children?.length
            ? (c.retained_payee_id ?? c.payee_id)
            : c.payee_id) ?? "",
        category_id: c.category_id ?? "",
        notes: parent.children?.length ? c.notes : "",
        transfer_account_id: c.transfer_account_id ?? "",
        counterpart_id: c.transfer_id ?? "",
      }));
      const next: SplitDraft = {
        budgetId,
        revision: uuid(),
        parent,
        snapshot: current.snapshot,
        epoch: current.epoch,
        reconciliationToken: current.reconciliation_token,
        transferProtectionToken: current.transfer_connected
          ? current.transfer_protection_token
          : undefined,
        transferProtectionClock: current.transfer_protection_clock,
        transferProtectedCount: current.transfer_protected_count,
        format,
        children,
        fields: {
          account_id: parent.account_id,
          amount: formatAmountInput(parent.amount, format),
          date: parent.date,
          payee_id: parent.retained_payee_id ?? parent.payee_id ?? "",
          notes: parent.notes,
          cleared: parent.cleared,
        },
        pending: null,
      };
      saved = await writeSplit(budgetId, null, next);
      draft = saved;
      active = true;
      await focusEditor();
    } catch (e) {
      error = errorMessage(e);
      if (!draft) active = false;
    } finally {
      busy = false;
    }
  }
  function persist() {
    if (!draft || busy || draft.pending) return;
    savingDraft = true;
    const value = $state.snapshot(draft);
    writes = writes.then(async () => {
      if (recovery) return;
      try {
        saved = await writeSplit(budgetId, saved, value);
        if (draft) draft.revision = saved!.revision;
      } catch (e) {
        error = errorMessage(e);
        recovery = true;
      } finally {
        savingDraft = false;
      }
    });
  }
  async function refreshSnapshot() {
    if (!draft || draft.pending || busy) return;
    busy = true;
    try {
      await writes;
      const current = await request<{
        data: Transaction;
        snapshot: string;
        epoch: string;
      }>(`/budgets/${budgetId}/transactions/${draft.parent.id}/splits`);
      const signature = (row: Transaction) =>
        JSON.stringify([
          row.field_versions,
          row.children?.map((c) => [c.id, c.field_versions]),
        ]);
      if (
        current.epoch !== draft.epoch ||
        signature(current.data) !== signature(draft.parent)
      )
        throw new Error(
          "This transaction changed. Your draft is retained. Copy any needed values, then discard this draft and reopen the current split.",
        );
      draft.snapshot = current.snapshot;
      saved = await writeSplit(budgetId, saved, $state.snapshot(draft));
      draft = saved;
      snapshotChanged = false;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function send(
    action = "replace",
    allocation = "exact",
    detach: string[] = [],
    cancel = false,
  ) {
    if (!draft || busy || recovery) return;
    busy = true;
    error = "";
    try {
      await writes;
      if (recovery) throw new Error("Recover saved split before continuing.");
      if (!draft.pending) {
        if (disconnected)
          throw new Error(
            "Connect before changing splits. Your draft is saved.",
          );
        if (action === "unsplit") {
          const original = draft.parent.children ?? [];
          const changed =
            draft.children.length !== original.length ||
            draft.children.some((child, index) => {
              const previous = original[index];
              return (
                !previous ||
                child.id !== previous.id ||
                parseAmount(child.amount || "0", draft!.format) !==
                  previous.amount ||
                (child.payee_id || null) !==
                  (previous.retained_payee_id ?? previous.payee_id) ||
                (child.category_id || null) !== previous.category_id ||
                child.notes !== previous.notes ||
                (child.transfer_account_id || null) !==
                  (previous.transfer_account_id ?? null)
              );
            });
          if (changed)
            throw new Error(
              "Save split edits before unsplitting. Your draft is retained.",
            );
        }
        const protectedRows = [
          draft.parent,
          ...(draft.parent.children ?? []),
        ].filter((row) => row.reconciled);
        if (
          (protectedRows.length || draft.transferProtectedCount) &&
          !draft.confirmedReconciled
        )
          throw new Error(
            "Confirm editing this reconciled split, or unlock it first.",
          );
        for (const child of draft.parent.children ?? [])
          observeVersions(child.field_versions);
        const fields = {
          ...draft.fields,
          amount: parseAmount(draft.fields.amount, draft.format),
          payee_id: draft.fields.payee_id || null,
        };
        if (draft.transferProtectionClock)
          observeVersions({ reconciled: draft.transferProtectionClock });
        const original = new Map(
          (draft.parent.children ?? []).map((child) => [child.id, child]),
        );
        const present = new Set(draft.children.map((child) => child.id));
        const changes =
          action === "replace"
            ? {
                upsert: draft.children
                  .map((child) => ({
                    ...child,
                    amount: parseAmount(child.amount || "0", draft!.format),
                    payee_id: child.transfer_account_id
                      ? null
                      : child.payee_id || null,
                    transfer_account_id: child.transfer_account_id || undefined,
                    counterpart_id: child.transfer_account_id
                      ? child.counterpart_id
                      : undefined,
                    category_id: child.category_id || null,
                  }))
                  .filter((child) => {
                    const previous = original.get(child.id);
                    return (
                      !previous ||
                      child.amount !== previous.amount ||
                      child.notes !== previous.notes ||
                      child.payee_id !==
                        (previous.transfer_id
                          ? null
                          : (previous.retained_payee_id ??
                            previous.payee_id)) ||
                      child.category_id !== previous.category_id ||
                      child.transfer_account_id !==
                        (previous.transfer_account_id ?? undefined) ||
                      child.counterpart_id !==
                        (previous.transfer_id ?? undefined)
                    );
                  }),
                remove: [...original.keys()].filter((id) => !present.has(id)),
              }
            : undefined;
        draft.pending = {
          path: `/budgets/${budgetId}/transactions/${draft.parent.id}/splits`,
          method: "POST",
          key: uuid(),
          historyOwner: await historyOwner(),
          edit_clock: nextEditClock(draft.parent.field_versions),
          budgetEpoch: draft.epoch,
          body: {
            snapshot: draft.snapshot,
            ...(protectedRows.length || draft.transferProtectedCount
              ? {
                  confirm_reconciled: true,
                  ...(draft.transferProtectionToken
                    ? {
                        transfer_protection_token:
                          draft.transferProtectionToken,
                      }
                    : draft.reconciliationToken
                      ? { reconciliation_token: draft.reconciliationToken }
                      : {
                          reconciled_versions: Object.fromEntries(
                            protectedRows.map((row) => [
                              row.id,
                              row.field_versions?.reconciled,
                            ]),
                          ),
                        }),
                }
              : {}),
            action,
            allocation,
            detach,
            fields,
            children: [],
            ...(changes ? { changes } : {}),
          },
        };
      }
      saved = await writeSplit(budgetId, saved, $state.snapshot(draft));
      draft = saved;
      const result = await mutate<Transaction>(draft!.pending!, cancel);
      observeVersions(result.data.field_versions);
      await writeSplit(budgetId, saved, null);
      saved = null;
      draft = null;
      await oncomplete();
      active = false;
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof SplitOwnershipError) recovery = true;
      if (e instanceof RequestError && e.code === "snapshot_changed")
        snapshotChanged = true;
      if (e instanceof RequestError && !e.uncertain && draft) {
        if (e.code === "operation_canceled") {
          resetCorrectedClock();
          await writeSplit(budgetId, saved, null);
          saved = null;
          draft = null;
          active = false;
        } else if (!["clock_skew", "clock_overflow"].includes(e.code ?? "")) {
          // Definite rejection permits correction; uncertain requests stay immutable.
          draft.pending = null;
          saved = await writeSplit(budgetId, saved, $state.snapshot(draft));
          draft = saved;
        }
      }
    } finally {
      busy = false;
    }
  }
  function setTransfer(
    child: SplitDraft["children"][number],
    destination: string,
  ) {
    const original = draft?.parent.children?.find((row) => row.id === child.id);
    child.transfer_account_id = destination;
    child.counterpart_id = destination
      ? original?.transfer_account_id === destination
        ? (original.transfer_id ?? uuid())
        : uuid()
      : "";
    child.payee_id = "";
    if (
      destination &&
      accounts.find((a) => a.id === destination)?.off_budget ===
        accounts.find((a) => a.id === draft?.fields.account_id)?.off_budget
    )
      child.category_id = "";
    persist();
  }
  async function close() {
    if (!draft || draft.pending || busy) return;
    try {
      await writes;
      await writeSplit(budgetId, saved, null);
      saved = null;
      draft = null;
      active = false;
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if snapshotChanged}<button
    onclick={refreshSnapshot}
    disabled={busy || disconnected}>{t("Refresh unchanged transaction")}</button
  >{/if}
{#if recovery}<button onclick={recover} disabled={busy}
    >{t("Recover saved split")}</button
  >{/if}
{#if draft}
  <section class="split-editor" aria-labelledby="split-title" aria-busy={busy}>
    <h3 id="split-title" tabindex="-1" bind:this={heading}>
      {t("Split transaction")}
    </h3>
    <p>
      {t("Transaction total")}: {formatAmount(draft.parent.amount)}. {t(
        "Negative amounts are expenses; positive amounts are income.",
      )}
    </p>
    {#if disconnected}<p role="status">
        {t("Connect before changing splits. Your draft is saved.")}
      </p>{/if}
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
      oninput={() => void persist()}
      onchange={() => void persist()}
    >
      {#if draft.transferProtectedCount || draft.parent.reconciled || draft.parent.children?.some((row) => row.reconciled)}
        <label class="checkbox"
          ><input
            type="checkbox"
            bind:checked={draft.confirmedReconciled}
            disabled={busy || !!draft.pending || recovery}
          />{t("Confirm editing this reconciled split")}</label
        >
        <p>Linked transfer entries are included. Entries remain reconciled.</p>
        <p class="hint">
          {t(
            "This changes settled entries. The transaction and its splits stay locked.",
          )}
        </p>
      {/if}
      <fieldset disabled={busy || !!draft.pending || recovery}>
        <legend>{t("Transaction")}</legend>
        <div class="split-fields">
          <label
            >{t("Account")}<select bind:value={draft.fields.account_id}
              >{#each accounts as a (a.id)}<option value={a.id}>{a.name}</option
                >{/each}</select
            ></label
          >
          <label
            >{t("Total amount")}<input
              bind:value={draft.fields.amount}
              inputmode="decimal"
              required
            /></label
          >
          <label
            >{t("Date")}<input
              type="date"
              bind:value={draft.fields.date}
              required
            /></label
          >
          <label
            >{t("Payee")}<select bind:value={draft.fields.payee_id}
              ><option value="">{t("No payee")}</option
              >{#if draft.fields.payee_id && !payees.some((p) => p.id === draft!.fields.payee_id)}<option
                  value={draft.fields.payee_id}>{t("Removed payee")}</option
                >{/if}{#each payees as p (p.id)}<option value={p.id}
                  >{p.name}</option
                >{/each}</select
            ></label
          >
          <label
            >{t("Notes")}<input
              bind:value={draft.fields.notes}
              maxlength="10000"
            /></label
          >
          <label class="checkbox"
            ><input type="checkbox" bind:checked={draft.fields.cleared} />{t(
              "Cleared",
            )}</label
          >
        </div>
      </fieldset>
      {#if draft.children.length > pageSize}
        <nav class="split-pages" aria-label={t("Split pages")}>
          <p role="status">
            {t("Splits {{start}}–{{end}} of {{total}}", {
              start: pageStart + 1,
              end: Math.min(pageStart + pageSize, draft.children.length),
              total: draft.children.length,
            })}
          </p>
          <div class="split-actions">
            <button
              type="button"
              disabled={busy || pageStart === 0}
              onclick={() => {
                page = pageStart / pageSize - 1;
              }}>{t("Previous splits")}</button
            >
            <button
              type="button"
              disabled={busy || pageStart + pageSize >= draft.children.length}
              onclick={() => {
                page = pageStart / pageSize + 1;
              }}>{t("Next splits")}</button
            >
            <label
              >{t("Go to split")}<input
                inputmode="numeric"
                bind:value={jumpTo}
                oninput={(event) => event.stopPropagation()}
                onchange={(event) => event.stopPropagation()}
              /></label
            >
            <button
              type="button"
              disabled={busy ||
                !Number.isInteger(jumpIndex) ||
                jumpIndex < 1 ||
                jumpIndex > draft.children.length}
              onclick={() => {
                page = Math.floor((jumpIndex - 1) / pageSize);
              }}>{t("Show split")}</button
            >
          </div>
        </nav>
      {/if}
      {#each draft.children.slice(pageStart, pageStart + pageSize) as child, offset (child.id)}
        {@const index = pageStart + offset}
        <fieldset disabled={busy || !!draft.pending || recovery}>
          <legend>{t("Split")} {index + 1}</legend>
          <div class="split-fields">
            <label
              >{t("Amount")}<input
                aria-label={`Split ${index + 1} amount`}
                bind:value={child.amount}
                inputmode="decimal"
                required
              /></label
            >
            <label
              >Transfer to<select
                aria-label={`Split ${index + 1} transfer to`}
                value={child.transfer_account_id ?? ""}
                onchange={(event) =>
                  setTransfer(child, event.currentTarget.value)}
              >
                <option value="">No transfer</option>
                {#each accounts.filter((a) => a.id !== draft?.fields.account_id && ((!a.closed && !a.deleted) || a.id === child.transfer_account_id)) as account (account.id)}
                  <option value={account.id}>{account.name}</option>
                {/each}
              </select></label
            >
            <label
              >{t("Payee")}<select
                disabled={!!child.transfer_account_id}
                aria-label={`Split ${index + 1} payee`}
                bind:value={child.payee_id}
                ><option value="">{t("No payee")}</option
                >{#if child.payee_id && !payees.some((p) => p.id === child.payee_id)}<option
                    value={child.payee_id}>{t("Removed payee")}</option
                  >{/if}{#each payees as p (p.id)}<option value={p.id}
                    >{p.name}</option
                  >{/each}</select
              ></label
            >
            <label
              >{t("Category")}<select
                aria-label={`Split ${index + 1} category`}
                disabled={!!child.transfer_account_id &&
                  accounts.find((a) => a.id === child.transfer_account_id)
                    ?.off_budget ===
                    accounts.find((a) => a.id === draft?.fields.account_id)
                      ?.off_budget}
                bind:value={child.category_id}
                ><option value="">{t("Uncategorized")}</option
                >{#each categories as c (c.id)}<option value={c.id}
                    >{c.name}</option
                  >{/each}</select
              ></label
            >
            <label
              >{t("Notes")}<input
                aria-label={`Split ${index + 1} notes`}
                bind:value={child.notes}
                maxlength="10000"
              /></label
            >
          </div>
          {#if child.transfer_account_id}<p class="hint">
              Saving keeps an opposite entry in the selected account. Removing
              this split also removes its paired entry. A paired split
              allocation stays, unlinked.
            </p>{/if}
          <button
            type="button"
            onclick={() => {
              if (draft) {
                draft.children = draft.children.filter(
                  (c) => c.id !== child.id,
                );
                void persist();
              }
            }}>{t("Remove split")}</button
          >
          {#if originalIds.has(child.id)}<button
              type="button"
              onclick={() => void send("unsplit", "exact", [child.id])}
              disabled={disconnected}>{t("Unsplit this child")}</button
            >{/if}
        </fieldset>
      {/each}
      {#if draft.pending}
        <p role="status">
          {t(
            "The outcome is unconfirmed. Retry the saved request before changing its details.",
          )}
        </p>
        <button type="button" onclick={() => void send()} disabled={busy}
          >{t("Retry saved split")}</button
        >
        <button
          type="button"
          onclick={() => void send("replace", "exact", [], true)}
          disabled={busy}>{t("Cancel saved split")}</button
        >
      {:else}
        <p role="status">
          {savingDraft
            ? t("Saving split draft…")
            : t("Split draft saved on this device.")}
        </p>
        <div class="split-actions">
          <button
            type="button"
            disabled={busy || draft.children.length >= 100000}
            onclick={() => {
              if (draft) {
                draft.children.push({
                  id: uuid(),
                  amount: "0",
                  payee_id: draft.fields.payee_id,
                  category_id: "",
                  notes: "",
                });
                page = Math.floor((draft.children.length - 1) / pageSize);
                void persist();
              }
            }}>{t("Add split")}</button
          >
          <button class="primary" disabled={busy || disconnected || recovery}
            >{t("Save splits")}</button
          >
          <button
            type="button"
            disabled={busy || disconnected || recovery}
            onclick={() => void send("replace", "auto")}
            >{t("Distribute and save")}</button
          >
          {#if draft.parent.is_parent}<button
              type="button"
              disabled={busy || disconnected || recovery}
              onclick={() => void send("recombine")}>{t("Recombine")}</button
            ><button
              type="button"
              disabled={busy || disconnected || recovery}
              onclick={() => void send("unsplit")}
              >{t("Unsplit all children")}</button
            >{/if}
          <button type="button" onclick={close} disabled={busy}
            >{t("Discard split draft")}</button
          >
        </div>
        <p class="hint">
          {t(
            "Distribute divides the remainder evenly among empty splits, otherwise proportionally. Recombine keeps the original total; unsplit turns allocations into separate transactions.",
          )}
        </p>
      {/if}
    </form>
  </section>
{/if}

<style>
  .split-editor {
    margin-block: 1.5rem;
    padding-block: 1rem;
    border-block: 1px solid var(--border);
  }
  .split-fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: 1rem;
  }
  fieldset {
    min-width: 0;
    margin-block: 1rem;
    padding: 1rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 0;
  }
  input,
  select {
    max-width: 100%;
    min-width: 0;
  }
  .split-pages {
    margin-block: 1rem;
  }
  .split-pages .split-actions {
    align-items: end;
  }
  .split-pages input {
    width: 8rem;
  }
  .split-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .checkbox {
    flex-direction: row;
    align-items: center;
    align-self: end;
    min-height: 44px;
  }
  .checkbox input {
    width: 1.25rem;
    height: 1.25rem;
    min-height: 0;
  }
</style>
