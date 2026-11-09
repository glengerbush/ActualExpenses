<script lang="ts">
  import { onMount } from "svelte";
  import MarkdownNotes from "./MarkdownNotes.svelte";
  import {
    request,
    errorMessage,
    RequestError,
    uuid,
    type Account,
    type AccountGroup,
  } from "./api";
  import { refreshBudget } from "./offline/cache";
  import { offlineStatus, subscribeOffline } from "./offline/events";
  import { nextEditClock } from "./edit-clock";
  import {
    clearAccountDraft,
    readAccountDraft,
    persistAccountDraft,
    newAccountDraft,
    sortedAccounts,
    sortedGroups,
    type AccountDraft,
    type AccountIntent,
  } from "./account-actions";
  import { formatAmount } from "./money";
  import { t } from "./i18n.svelte";
  let {
    budgetId,
    selectedId = "",
    onchanged,
    onclose,
  }: {
    budgetId: string;
    selectedId?: string;
    onchanged: () => Promise<void>;
    onclose: () => void;
  } = $props();
  let accounts = $state.raw<Account[]>([]),
    groups = $state.raw<AccountGroup[]>([]),
    draft = $state.raw<AccountDraft | null>(null);
  let busy = $state(false),
    error = $state(""),
    status = $state(""),
    blocked = $state(false),
    review = $state(false),
    currentEpoch = "";
  let deletedBudget = $state(false);
  let offline = $state(!offlineStatus.reachable);
  let chosen = $derived(accounts.find((a) => a.id === draft?.accountId));
  let chosenGroup = $derived(
    groups.find((g) => g.id === draft?.groupSelection),
  );
  async function snapshot() {
    const cache = await refreshBudget(budgetId);
    accounts = sortedAccounts(cache.accounts);
    groups = sortedGroups(cache.account_groups ?? []);
    currentEpoch = cache.epoch;
  }
  async function load() {
    busy = true;
    error = "";
    try {
      const saved = readAccountDraft(budgetId);
      if (saved) draft = saved;
      await snapshot();
      if (!saved) {
        draft = newAccountDraft(
          budgetId,
          currentEpoch,
          accounts.find((a) => a.id === selectedId) ?? accounts[0] ?? null,
        );
        persistAccountDraft(draft);
      } else status = t("Recovered your saved account changes.");
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function update(patch: Partial<AccountDraft>) {
    if (!draft || draft.pending || busy) return;
    try {
      const next = { ...draft, ...patch };
      persistAccountDraft(next, draft);
      draft = next;
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function selectAccount(id: string) {
    const a = accounts.find((a) => a.id === id) ?? null;
    update({
      accountId: id,
      name: a?.name ?? "",
      notes: a?.notes ?? "",
      groupId: a?.group_id ?? "",
      beforeId: "",
      confirmName: "",
      initial: a,
    });
  }
  function selectGroup(id: string) {
    update({
      groupSelection: id,
      groupName: groups.find((g) => g.id === id)?.name ?? "",
      groupBefore: "",
    });
  }
  async function send(pending: AccountIntent) {
    if (!draft || busy) return;
    error = "";
    status = "";
    let acknowledged = false;
    try {
      const next = { ...draft, pending };
      persistAccountDraft(next, draft);
      draft = next;
      busy = true;
      const response = await request<{ conflicts?: { field: string }[] }>(
        pending.path,
        pending.body,
        pending.key,
        pending.method,
        pending.clock,
        false,
        pending.epoch,
      );
      acknowledged = true;
      await snapshot();
      await onchanged();
      const fresh = newAccountDraft(
        budgetId,
        currentEpoch,
        accounts.find((a) => a.id === draft?.accountId) ?? accounts[0] ?? null,
      );
      const retained = pending.path.includes("/account-groups")
        ? {
            ...fresh,
            name: draft.name,
            notes: draft.notes,
            groupId: draft.groupId,
            initial: draft.initial,
            groupSelection: groups.some((g) => g.id === draft!.groupSelection)
              ? draft.groupSelection
              : "",
            groupName:
              groups.find((g) => g.id === draft!.groupSelection)?.name ?? "",
          }
        : {
            ...fresh,
            groupSelection: draft.groupSelection,
            groupName: draft.groupName,
            groupBefore: draft.groupBefore,
          };
      persistAccountDraft(retained, draft);
      draft = retained;
      blocked = false;
      review = false;
      status = response.conflicts?.length
        ? t("A newer edit kept these fields:") +
          " " +
          response.conflicts.map((v) => v.field).join(", ")
        : t(pending.message);
    } catch (e) {
      if (e instanceof RequestError && !e.uncertain && !acknowledged) {
        blocked = e.code === "budget_restored" || e.code === "budget_deleted";
        if (!blocked && draft) {
          const next = { ...draft, pending: null };
          try {
            persistAccountDraft(next, draft);
            draft = next;
          } catch (storage) {
            error = errorMessage(storage);
          }
        }
      }
      error ||= errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function action(
    path: string,
    body: Record<string, unknown>,
    method: AccountIntent["method"],
    message: string,
    versions?: Record<string, string>,
  ) {
    if (!draft || draft.pending || busy) return;
    if (!offlineStatus.reachable) {
      error = t("Reconnect before changing accounts or groups.");
      return;
    }
    try {
      await send({
        path: `/budgets/${budgetId}/${path}`,
        body,
        method,
        message,
        key: uuid(),
        clock: nextEditClock(versions),
        epoch: draft.epoch,
      });
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function saveAccount(event: SubmitEvent) {
    event.preventDefault();
    if (!draft || !chosen) return;
    const patch: Record<string, unknown> = {};
    if (draft.name !== draft.initial?.name) patch.name = draft.name;
    if (draft.notes !== (draft.initial?.notes ?? "")) patch.notes = draft.notes;
    if (draft.groupId !== (draft.initial?.group_id ?? ""))
      patch.group_id = draft.groupId || null;
    if (!Object.keys(patch).length) {
      status = t("No account changes.");
      return;
    }
    void action(
      `accounts/${chosen.id}`,
      patch,
      "PATCH",
      "Account saved.",
      chosen.field_versions,
    );
  }
  function saveGroup(event: SubmitEvent) {
    event.preventDefault();
    if (!draft) return;
    void action(
      draft.groupSelection
        ? `account-groups/${draft.groupSelection}`
        : "account-groups",
      draft.groupSelection
        ? { name: draft.groupName }
        : { id: uuid(), name: draft.groupName },
      draft.groupSelection ? "PATCH" : "POST",
      draft.groupSelection ? "Group saved." : "Group created.",
      chosenGroup?.field_versions,
    );
  }
  async function reviewCurrent() {
    busy = true;
    try {
      const { data: budget } = await request<{
        data: { deleted: boolean; epoch: string };
      }>(`/budgets/${budgetId}?include_deleted=true`);
      deletedBudget = budget.deleted;
      if (deletedBudget) {
        accounts = [];
        groups = [];
        currentEpoch = budget.epoch;
      } else await snapshot();
      review = true;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function retire(reapply: boolean) {
    if (!draft || !blocked || !review) return;
    try {
      localStorage.setItem(
        `actualBudget.retiredAccounts.v1.${draft.pending?.key ?? uuid()}`,
        JSON.stringify(draft),
      );
      const next = reapply
        ? { ...draft, epoch: currentEpoch, pending: null }
        : newAccountDraft(budgetId, currentEpoch, accounts[0] ?? null);
      persistAccountDraft(next, draft);
      draft = next;
      blocked = false;
      review = false;
      error = "";
      status = t(
        reapply
          ? "Review the retained fields, then save a new change."
          : "Saved change discarded.",
      );
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function done() {
    if (draft?.pending || busy) return;
    try {
      clearAccountDraft(budgetId);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(() => {
    void load();
    return subscribeOffline(() => {
      offline = !offlineStatus.reachable;
    });
  });
</script>

<section class="form-section" aria-labelledby="organize-title" aria-busy={busy}>
  <div class="page-heading">
    <h1 id="organize-title">{t("Organize accounts")}</h1>
    <button onclick={done} disabled={busy || !!draft?.pending}
      >{t("Done")}</button
    >
  </div>
  {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
  {#if status}<p role="status">{status}</p>{/if}
  {#if offline}<p role="status">
      {t(
        "Reconnect before changing accounts or groups. Your saved changes are kept.",
      )}
    </p>{/if}
  {#if busy}<p role="status">{t("Saving or loading account changes…")}</p>{/if}
  {#if draft?.pending}
    <p>
      {t(
        "The outcome is unconfirmed. Retry this saved request before changing its details.",
      )}
    </p>
    {#if blocked}<button onclick={reviewCurrent} disabled={busy}
        >{t("Review current accounts")}</button
      >
      {#if review}<p>
          {t("Current accounts:")}
          {accounts.map((a) => a.name).join(", ") || t("None")}
        </p>
        <div class="actions">
          <button onclick={() => retire(false)}
            >{t("Discard saved change")}</button
          ><button disabled={deletedBudget} onclick={() => retire(true)}
            >{t("Keep fields for a new change")}</button
          >
        </div>{/if}
    {:else}<button
        onclick={() => draft?.pending && send(draft.pending)}
        disabled={busy}>{t("Retry saved change")}</button
      >{/if}
  {/if}
  {#if draft}
    {#if accounts.length}
      <fieldset disabled={busy || !!draft.pending}>
        <label for="manage-account">{t("Account to organize")}</label><select
          id="manage-account"
          bind:value={() => draft!.accountId, (v) => selectAccount(v)}
          >{#each accounts as a (a.id)}<option value={a.id}
              >{a.name}{a.closed ? " — " + t("Closed") : ""}</option
            >{/each}</select
        >
      </fieldset>
      {#if chosen}
        <p class="muted">
          {chosen.off_budget ? t("Off budget") : t("On budget")} · {formatAmount(
            chosen.balance,
          )}{chosen.closed ? " · " + t("Closed") : ""}
        </p>
        <form onsubmit={saveAccount}>
          <fieldset disabled={busy || !!draft.pending}>
            <label for="manage-name">{t("Account name")}</label><input
              id="manage-name"
              required
              maxlength="120"
              bind:value={() => draft!.name, (v) => update({ name: v })}
            />
            <label for="manage-notes">{t("Account notes")}</label><textarea
              id="manage-notes"
              rows="3"
              maxlength="10000"
              bind:value={() => draft!.notes, (v) => update({ notes: v })}
            ></textarea>
            {#if draft.notes}
              <details>
                <summary>{t("Preview")}</summary>
                <MarkdownNotes text={draft.notes} />
              </details>
            {/if}
            <label for="manage-group">{t("Account group")}</label><select
              id="manage-group"
              bind:value={() => draft!.groupId, (v) => update({ groupId: v })}
              ><option value="">{t("Ungrouped")}</option
              >{#each groups as g (g.id)}<option value={g.id}>{g.name}</option
                >{/each}</select
            >
            <button class="primary" disabled={offline}
              >{t("Save account")}</button
            >
          </fieldset>
        </form>
        <fieldset disabled={busy || !!draft.pending}>
          <label for="account-before">{t("Place account before")}</label><select
            id="account-before"
            bind:value={() => draft!.beforeId, (v) => update({ beforeId: v })}
            ><option value="">{t("End of list")}</option
            >{#each accounts.filter((a) => a.id !== chosen!.id && a.off_budget === chosen!.off_budget) as a (a.id)}<option
                value={a.id}>{a.name}</option
              >{/each}</select
          >
          <button
            disabled={offline}
            onclick={() =>
              action(
                `accounts/${chosen!.id}/move`,
                { before_id: draft!.beforeId || null },
                "POST",
                "Account moved.",
                chosen!.field_versions,
              )}>{t("Move account")}</button
          >
          <h2>{t(chosen.closed ? "Reopen account" : "Close account")}</h2>
          <p>
            {chosen.closed
              ? t("History is retained. Reopen this account to use it again.")
              : chosen.balance !== 0
                ? t(
                    "The balance must be zero before closing without a transfer.",
                  )
                : t(
                    "Closing keeps transaction history. An account with no transactions is removed.",
                  )}
          </p>
          <button
            disabled={offline || (!chosen.closed && chosen.balance !== 0)}
            onclick={() =>
              action(
                `accounts/${chosen!.id}/${chosen!.closed ? "reopen" : "close"}`,
                {},
                "POST",
                chosen!.closed ? "Account reopened." : "Account closed.",
                chosen!.field_versions,
              )}>{t(chosen.closed ? "Reopen account" : "Close account")}</button
          >
          {#if !chosen.closed}<h2>{t("Delete account")}</h2>
            <p>
              {t(
                "Delete this account and all its transactions. Historical totals will change.",
              )}
            </p>
            <label for="confirm-account-delete"
              >{t("Type account name to delete")}:
              <strong>{chosen.name}</strong></label
            ><input
              id="confirm-account-delete"
              aria-label={t("Type account name to delete")}
              autocomplete="off"
              bind:value={
                () => draft!.confirmName, (v) => update({ confirmName: v })
              }
            />
            <button
              disabled={offline || draft.confirmName !== chosen.name}
              onclick={() =>
                action(
                  `accounts/${chosen!.id}`,
                  { confirm_name: draft!.confirmName },
                  "DELETE",
                  "Account and transactions deleted.",
                  chosen!.field_versions,
                )}>{t("Delete account and transactions")}</button
            >{/if}
        </fieldset>
      {/if}
    {:else}<p>{t("No accounts to organize.")}</p>{/if}
    <h2>{t("Account groups")}</h2>
    <form onsubmit={saveGroup}>
      <fieldset disabled={busy || !!draft.pending}>
        <label for="group-selection">{t("Group to organize")}</label><select
          id="group-selection"
          bind:value={() => draft!.groupSelection, (v) => selectGroup(v)}
          ><option value="">{t("New group")}</option
          >{#each groups as g (g.id)}<option value={g.id}>{g.name}</option
            >{/each}</select
        >
        <label for="group-name">{t("Group name")}</label><input
          id="group-name"
          required
          maxlength="120"
          bind:value={() => draft!.groupName, (v) => update({ groupName: v })}
        />
        <button disabled={offline}
          >{t(draft.groupSelection ? "Save group" : "Create group")}</button
        >
      </fieldset>
    </form>
    {#if chosenGroup}<fieldset disabled={busy || !!draft.pending}>
        <label for="group-before">{t("Place group before")}</label><select
          id="group-before"
          bind:value={
            () => draft!.groupBefore, (v) => update({ groupBefore: v })
          }
          ><option value="">{t("End of list")}</option
          >{#each groups.filter((g) => g.id !== chosenGroup!.id) as g (g.id)}<option
              value={g.id}>{g.name}</option
            >{/each}</select
        >
        <button
          disabled={offline}
          onclick={() =>
            action(
              `account-groups/${chosenGroup!.id}/move`,
              { before_id: draft!.groupBefore || null },
              "POST",
              "Group moved.",
              chosenGroup!.field_versions,
            )}>{t("Move group")}</button
        >
        <p>
          {t("Deleting a group keeps its accounts and leaves them ungrouped.")}
        </p>
        <button
          disabled={offline}
          onclick={() =>
            action(
              `account-groups/${chosenGroup!.id}`,
              {},
              "DELETE",
              "Group deleted. Accounts kept.",
              chosenGroup!.field_versions,
            )}>{t("Delete group")}</button
        >
      </fieldset>{/if}
  {:else if !busy}<button onclick={load}>{t("Retry account recovery")}</button
    >{/if}
</section>

<style>
  h2 {
    margin-top: 2rem;
  }
  form,
  fieldset {
    margin-bottom: 1.5rem;
  }
  p {
    max-width: 65ch;
  }
</style>
