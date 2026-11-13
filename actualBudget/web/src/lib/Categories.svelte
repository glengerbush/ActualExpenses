<script lang="ts">
  import {
    categoryShade,
    groupColor,
    type CategoryLaunch,
  } from "./category-colors";
  import CategoryColor from "./CategoryColor.svelte";
  import { historyOwner } from "./history-owner";
  import { onMount, tick } from "svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { formatAmount, formatDate } from "./money";
  import type { Transaction, PendingMutation } from "./register-api";
  import { t } from "./i18n.svelte";
  import { offlineStatus, subscribeOffline } from "./offline/events";
  let {
    budgetId,
    onclose,
    launch,
    onchanged,
  }: {
    budgetId: string;
    onclose: () => void;
    launch?: CategoryLaunch;
    onchanged?: () => Promise<void>;
  } = $props();
  type Category = {
    color: string | null;
    resolved_color: string;
    id: string;
    name: string;
    group_id: string | null;
    is_income: boolean;
    deleted: boolean;
    sort_order: number | null | undefined;
    field_versions: Record<string, string>;
  };
  type Kind = "categories" | "category-groups";
  type Draft = {
    new_id?: string;
    color?: string | null;
    kind: Kind;
    id: string | null;
    name: string;
    group_id: string;
    is_income: boolean;
    sort_order: number | null | undefined;
    epoch: string;
    original: Category | null;
  };
  let categories = $state.raw<Category[]>([]),
    groups = $state.raw<Category[]>([]);
  let draft = $state<Draft | null>(null),
    pending = $state<PendingMutation | null>(null);
  let loading = $state(true),
    busy = $state(false),
    error = $state(""),
    notice = $state(""),
    epoch = $state(""),
    ready = $state(false),
    storageError = $state("");
  let deleting = $state(false),
    replacement = $state(""),
    retired = $state(false);
  let history = $state.raw<{
    id: string;
    name: string;
    data: Transaction[];
    total: number;
    next_offset: number | null;
    snapshot: string;
  } | null>(null);
  let editor = $state<HTMLInputElement>();
  let offline = $state(!offlineStatus.reachable);
  let clockRecovery = $state(false);
  const cancelKey = (operation: PendingMutation) =>
    `actualBudget.categories.cancel.${operation.key}`;
  const writer = uuid();
  let savedRaw: string | null = null;
  let savedPayload = "";
  const storageKey = $derived(`actualBudget.categories.v1.${budgetId}`);
  const automaticColor = $derived(
    draft
      ? draft.kind === "category-groups"
        ? groupColor(draft.id ?? draft.new_id!)
        : categoryShade(
            groups.find((g) => g.id === draft?.group_id)?.resolved_color ??
              groupColor("ungrouped"),
            draft.id ?? draft.new_id!,
          )
      : "#397C8C",
  );
  const visible = $derived(categories.filter((c) => !c.deleted));
  const retained = $derived([
    ...categories
      .filter((row) => row.deleted)
      .map((row) => ({ ...row, key: `category/${row.id}`, group: false })),
    ...groups
      .filter((row) => row.deleted)
      .map((row) => ({ ...row, key: `group/${row.id}`, group: true })),
  ]);
  async function rows(kind: Kind, snapshot: string) {
    let out: Category[] = [];
    let offset: number | null = 0;
    do {
      const page: { data: Category[]; next_offset: number | null } =
        await request(
          `/budgets/${budgetId}/${kind}?limit=1000&offset=${offset}&include_deleted=true&snapshot=${snapshot}`,
        );
      out.push(...page.data);
      offset = page.next_offset;
    } while (offset !== null);
    return out;
  }
  async function load() {
    loading = true;
    error = "";
    try {
      const { data } = await request<{
        data: { epoch: string; snapshot: string; cursor?: string };
      }>(`/budgets/${budgetId}/checkpoint`);
      const snapshot = data.snapshot ?? data.cursor!;
      const [c, g] = await Promise.all([
        rows("categories", snapshot),
        rows("category-groups", snapshot),
      ]);
      categories = c;
      groups = g;
      epoch = data.epoch;
      for (const row of [...c, ...g]) observeVersions(row.field_versions);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
  function ownsSavedDraft() {
    try {
      if (!ready) return false;
      if (localStorage.getItem(storageKey) !== savedRaw) {
        storageError = t(
          "Another tab changed the saved category draft. Reload this page before continuing.",
        );
        return false;
      }
      return true;
    } catch (e) {
      storageError = errorMessage(e);
      return false;
    }
  }
  function persist() {
    if (!ownsSavedDraft()) return false;
    try {
      const payload = JSON.stringify({ draft, pending, deleting, replacement });
      if (payload === savedPayload || (!savedRaw && !draft && !pending))
        return true;
      if (!draft && !pending) {
        localStorage.removeItem(storageKey);
        savedRaw = null;
        savedPayload = payload;
        return true;
      }
      const next = JSON.stringify({
        draft,
        pending,
        deleting,
        replacement,
        revision: uuid(),
        owner: writer,
      });
      localStorage.setItem(storageKey, next);
      savedRaw = next;
      savedPayload = payload;
      storageError = "";
      return true;
    } catch (e) {
      storageError = errorMessage(e);
      return false;
    }
  }
  function storageChanged(event: StorageEvent) {
    if (
      event.storageArea === localStorage &&
      (event.key === storageKey || event.key === null)
    )
      ownsSavedDraft();
  }
  onMount(() =>
    subscribeOffline(() => {
      offline = !offlineStatus.reachable;
    }),
  );
  onMount(() => {
    try {
      savedRaw = localStorage.getItem(storageKey);
      if (savedRaw) {
        const state = JSON.parse(savedRaw);
        draft = state.draft;
        if (draft && !draft.id && !draft.new_id)
          draft.new_id =
            typeof state.pending?.body?.id === "string"
              ? state.pending.body.id
              : uuid();
        pending = state.pending;
        deleting = !!state.deleting;
        replacement = state.replacement ?? "";
      }
      savedPayload = JSON.stringify({ draft, pending, deleting, replacement });
    } catch (e) {
      storageError = errorMessage(e);
      void load();
      return;
    }
    ready = true;
    void load().then(async () => {
      if (launch && !draft && !pending && !error) {
        const row = launch.id
          ? (launch.kind === "categories" ? categories : groups).find(
              (r) => r.id === launch.id,
            )
          : undefined;
        if (launch.id && !row) {
          error = "This category or group is no longer available.";
          return;
        }
        await begin(launch.kind, row);
      }
    });
  });
  $effect(() => {
    if (ready) {
      JSON.stringify({ draft, pending, deleting, replacement });
      persist();
    }
  });
  async function begin(kind: Kind, row?: Category) {
    if (
      pending ||
      !epoch ||
      storageError ||
      !offlineStatus.reachable ||
      !ownsSavedDraft()
    )
      return;
    history = null;
    deleting = false;
    error = "";
    notice = "";
    draft = {
      kind,
      new_id: row?.id ?? uuid(),
      id: row?.id ?? null,
      name: row?.name ?? "",
      group_id: row?.group_id ?? launch?.groupId ?? "",
      color: row?.color ?? null,
      is_income:
        row?.is_income ??
        groups.find((g) => g.id === launch?.groupId)?.is_income ??
        launch?.income ??
        false,
      sort_order: row
        ? row.sort_order
        : (kind === "categories" ? categories : groups).length,
      epoch,
      original: row ?? null,
    };
    if (!persist()) return;
    await tick();
    editor?.focus();
  }
  function closeEditor() {
    if (pending || storageError || !ownsSavedDraft()) return;
    draft = null;
    deleting = false;
    error = "";
    persist();
    if (launch) onclose();
  }
  async function send() {
    if (!pending || busy || storageError || !persist()) return;
    busy = true;
    error = "";
    retired = false;
    const operation = pending;
    try {
      const result = await request<{
        data: Category;
        conflicts?: { field: string }[];
      }>(
        operation.path,
        operation.body,
        operation.key,
        operation.method,
        operation.edit_clock,
        localStorage.getItem(cancelKey(operation)) === "true",
        operation.budgetEpoch,
        operation.historyOwner,
      );
      if (!ownsSavedDraft()) return;
      pending = null;
      draft = null;
      deleting = false;
      if (!persist()) return;
      notice = result.conflicts?.length
        ? t("A newer edit was kept. Review the current category.")
        : t("Saved.");
      await load();
      await onchanged?.();
      if (launch && !result.conflicts?.length) onclose();
      if (operation.budgetEpoch && epoch && operation.budgetEpoch !== epoch)
        notice = t(
          "Original request confirmed. The current budget has since been restored; review its categories.",
        );
    } catch (e) {
      error = errorMessage(e);
      if (e instanceof RequestError) {
        retired = e.code === "budget_restored" || e.code === "budget_deleted";
        if (
          !e.uncertain &&
          e.code === "operation_canceled" &&
          ownsSavedDraft()
        ) {
          pending = null;
          retired = false;
          clockRecovery = true;
          if (!persist()) return;
          error = "";
          notice = t(
            "Request canceled. Your fields are retained. Use corrected device time before saving a new change.",
          );
          await load();
          if (draft && epoch && ownsSavedDraft()) {
            draft.epoch = epoch;
            persist();
          }
        }
      }
    } finally {
      busy = false;
    }
  }
  async function save() {
    if (!draft || busy || pending || storageError || !ownsSavedDraft()) return;
    if (!offlineStatus.reachable) {
      error = t("Reconnect before changing categories or groups.");
      return;
    }
    try {
      const owner = await historyOwner();
      if (!draft || pending || busy) return;
      const values = {
        name: draft.name,
        color: draft.color ?? null,
        group_id: draft.group_id || null,
        is_income: draft.is_income,
        sort_order: draft.sort_order ?? null,
      };
      let body: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(values)) {
        if (key === "group_id" && draft.kind === "category-groups") continue;
        if (!draft.original || draft.original[key as keyof Category] !== value)
          body[key] = value;
      }
      if (!draft.id) body.id = draft.new_id ?? uuid();
      if (deleting) body = { replacement_id: replacement || null };
      if (!Object.keys(body).length) {
        closeEditor();
        return;
      }
      pending = {
        path: `/budgets/${budgetId}/${draft.kind}${draft.id ? `/${draft.id}` : ""}`,
        method: deleting ? "DELETE" : draft.id ? "PATCH" : "POST",
        key: uuid(),
        historyOwner: owner,
        edit_clock: nextEditClock(draft.original?.field_versions),
        budgetEpoch: draft.epoch,
        body,
      };
      if (persist()) await send();
    } catch (e) {
      clockRecovery = e instanceof LocalClockError;
      error = errorMessage(e);
    }
  }
  function useCorrectedTime() {
    if (pending || busy || storageError || !ownsSavedDraft()) return;
    try {
      resetCorrectedClock();
      clockRecovery = false;
      error = "";
      notice = t(
        "Corrected device time is ready. Review your fields, then save a new change.",
      );
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function cancel() {
    if (!pending || busy || storageError || !ownsSavedDraft()) return;
    const original = pending;
    busy = true;
    try {
      const { data } = await request<{ data: { capabilities: string[] } }>(
        "/info",
      );
      if (!data.capabilities.includes("category_cancellation"))
        throw new Error(
          "This server cannot safely cancel this request. Retry it unchanged.",
        );
      if (pending !== original || !ownsSavedDraft()) return;
      localStorage.setItem(cancelKey(original), "true");
      busy = false;
      await send();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function recover() {
    if (!pending || !retired) return;
    await cancel();
  }
  async function drill(id: string, name: string, more = false) {
    loading = true;
    error = "";
    try {
      const page = await request<{
        data: Transaction[];
        total: number;
        next_offset: number | null;
        snapshot: string;
      }>(
        `/budgets/${budgetId}/categories/${id}/transactions?limit=100&offset=${more ? (history?.next_offset ?? 0) : 0}${more ? `&snapshot=${history?.snapshot}` : ""}`,
      );
      history = {
        id,
        name,
        ...page,
        data: more ? [...history!.data, ...page.data] : page.data,
      };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
</script>

<svelte:window onstorage={storageChanged} />
{#if !launch}<div class="page-heading">
    <div>
      <h1>{t("Categories")}</h1>
      <p class="muted">
        {t("Organize spending and income. History stays connected.")}
      </p>
    </div>
    <button onclick={onclose}>{t("Done")}</button>
  </div>{/if}
{#if offline}<p role="status">
    {t("Reconnect before changing categories or groups.")}
  </p>{/if}
{#if storageError}<p role="alert">
    {t("Draft could not be saved on this device.")}
    {storageError}
    <button onclick={() => location.reload()}>{t("Reload page")}</button>
  </p>{/if}
{#if error}<div role="alert">
    <p>{error}</p>
    {#if pending}<button onclick={send} disabled={busy || !!storageError}
        >{t("Retry unchanged request")}</button
      >{#if retired}<button onclick={recover} disabled={busy || !!storageError}
          >{t("Keep original and review current categories")}</button
        >{/if}{:else}<button onclick={load} disabled={loading}
        >{t("Reload categories")}</button
      >{/if}
  </div>{/if}
{#if notice}<p role="status">{notice}</p>{/if}
{#if clockRecovery && !pending}
  <button onclick={useCorrectedTime} disabled={busy || !!storageError}
    >{t("Use corrected device time")}</button
  >
{/if}
{#if pending}
  <button onclick={cancel} disabled={busy || offline || !!storageError}
    >{t("Cancel pending request and keep fields")}</button
  >
{/if}
{#if pending && !error}<p role="status">
    {t("A saved request needs confirmation.")}
    <button onclick={send} disabled={busy || !!storageError}
      >{t("Retry unchanged request")}</button
    >
  </p>{/if}
{#if loading}<p role="status">{t("Loading categories…")}</p>{/if}
{#if draft}
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
    class="category-editor"
  >
    <h2>
      {deleting
        ? t("Delete and reassign")
        : draft.id
          ? t("Edit category or group")
          : draft.kind === "categories"
            ? draft.group_id
              ? `New category in ${groups.find((g) => g.id === draft?.group_id)?.name ?? "group"}`
              : t("New category")
            : t("New group")}
    </h2>
    <fieldset disabled={busy || !!pending || loading || !!storageError}>
      <legend class="sr-only">{t("Category details")}</legend>
      {#if deleting}
        <p>
          {t("Delete")} <strong>{draft.name}</strong>? {t(
            "Transactions keep their amounts.",
          )}
        </p>
        {#if draft.kind === "category-groups"}<p>
            {t(
              "Every category in this group will be deleted and its transactions reassigned.",
            )}
          </p>{/if}
        <label for="replacement">{t("Reassign transactions to")}</label><select
          id="replacement"
          bind:value={replacement}
          ><option value="">{t("Uncategorized")}</option
          >{#each categories.filter((c) => !c.deleted && c.is_income === draft!.is_income && c.id !== draft!.id && (draft!.kind !== "category-groups" || c.group_id !== draft!.id)) as row (row.id)}<option
              value={row.id}>{row.name}</option
            >{/each}</select
        >
      {:else}
        <label for="category-name">{t("Name")}</label><input
          id="category-name"
          bind:this={editor}
          bind:value={draft.name}
          maxlength="120"
          required
        />
        {#if !draft.id && !(launch && draft.kind === "categories")}<label
            for="category-type">{t("Type")}</label
          ><select
            id="category-type"
            bind:value={draft.is_income}
            onchange={() => {
              if (draft) draft.group_id = "";
            }}
            ><option value={false}>{t("Expense")}</option><option value={true}
              >{t("Income")}</option
            ></select
          >{:else}<p class="muted">
            {draft.is_income ? t("Income") : t("Expense")}
          </p>{/if}
        {#if draft.kind === "categories" && (!launch || draft.id)}<label
            for="category-group">{t("Group")}</label
          ><select id="category-group" bind:value={draft.group_id}
            ><option value="">{t("Ungrouped")}</option
            >{#each groups.filter((g) => !g.deleted && (g.is_income === draft!.is_income || g.id === draft!.group_id)) as group (group.id)}<option
                value={group.id}>{group.name}</option
              >{/each}</select
          >{/if}
        <label for="category-color"
          >{draft.kind === "categories"
            ? "Category shade"
            : "Group color"}</label
        >
        <input
          id="category-color"
          type="color"
          value={draft.color ?? automaticColor}
          oninput={(e) => {
            if (draft) draft.color = e.currentTarget.value;
          }}
        />
        {#if draft.kind === "categories"}<button
            type="button"
            onclick={() => {
              if (draft) draft.color = null;
            }}>Use automatic shade</button
          >{/if}
        <label for="category-order">{t("Order")}</label><input
          id="category-order"
          type="number"
          step="any"
          min="-9007199254740991"
          max="9007199254740991"
          bind:value={draft.sort_order}
        />
        <p class="hint">{t("Lower numbers appear first within the group.")}</p>
      {/if}
      <div class="category-actions">
        <button type="submit" class="primary" disabled={offline}
          >{busy
            ? t("Saving…")
            : deleting
              ? t("Delete and reassign")
              : t("Save")}</button
        ><button type="button" onclick={closeEditor}>{t("Cancel")}</button
        >{#if draft.id && !deleting}<button
            type="button"
            disabled={offline}
            onclick={() => {
              if (!offlineStatus.reachable || !ownsSavedDraft()) return;
              deleting = true;
              replacement = "";
            }}>{t("Merge or delete")}</button
          >{/if}
      </div>
    </fieldset>
  </form>
{:else if history}
  <section aria-label={t("Category transactions")}>
    <div class="page-heading">
      <h2>{history.name}</h2>
      <button onclick={() => (history = null)}>{t("Back to categories")}</button
      >
    </div>
    <p>{t("Total")}: <strong>{formatAmount(history.total)}</strong></p>
    {#if !history.data.length}<p>
        {t("No transactions in this category.")}
      </p>{:else}<div class="category-history">
        {#each history.data as tx (tx.id)}<article>
            <div>
              <strong>{formatDate(tx.date)} · {tx.account_name}</strong>
              <p>
                {tx.payee_name ?? t("No payee")} · {tx.category_name ??
                  t("Uncategorized")}
              </p>
            </div>
            <strong class="amount">{formatAmount(tx.amount)}</strong>
          </article>{/each}
      </div>{/if}{#if history.next_offset !== null}<button
        disabled={loading}
        onclick={() => history && drill(history.id, history.name, true)}
        >{t("Load more")}</button
      >{/if}
  </section>
{:else if !launch}
  <div class="category-actions">
    <button
      onclick={() => begin("categories")}
      disabled={offline ||
        loading ||
        busy ||
        !!pending ||
        !epoch ||
        !!storageError}>{t("New category")}</button
    ><button
      onclick={() => begin("category-groups")}
      disabled={offline ||
        loading ||
        busy ||
        !!pending ||
        !epoch ||
        !!storageError}>{t("New group")}</button
    ><button
      onclick={() => drill("uncategorized", t("Uncategorized"))}
      disabled={loading}>{t("Uncategorized transactions")}</button
    >
  </div>
  {#each [false, true] as income (income)}<section class="category-section">
      <h2>{income ? t("Income") : t("Expenses")}</h2>
      {#each [...groups.filter((g) => !g.deleted && (g.is_income === income || visible.some((c) => c.group_id === g.id && c.is_income === income))), null] as group (group?.id ?? "ungrouped")}
        {@const children = visible.filter(
          (c) => c.is_income === income && c.group_id === (group?.id ?? null),
        )}
        {#if group || children.length}<div class="category-group">
            <div class="group-heading">
              <h3>
                {group?.name ?? t("Ungrouped")}
              </h3>
              {#if group}<CategoryColor
                  {budgetId}
                  id={group.id}
                  kind="category-groups"
                  name={group.name}
                  color={group.resolved_color}
                  disabled={busy || loading || offline || !!draft || !!pending}
                  onchanged={load}
                /><button
                  onclick={() => begin("category-groups", group)}
                  disabled={offline || !!pending || !!storageError}
                  >{t("Edit group")}
                  <span class="sr-only">{group.name}</span></button
                >{/if}
            </div>

            {#each children as row (row.id)}<div class="category-row">
                <div>
                  <CategoryColor
                    {budgetId}
                    id={row.id}
                    kind="categories"
                    name={row.name}
                    color={row.resolved_color}
                    automatic={!row.color}
                    disabled={busy ||
                      loading ||
                      offline ||
                      !!draft ||
                      !!pending}
                    onchanged={load}
                  />
                  <strong>{row.name}</strong>
                </div>
                <div class="category-actions">
                  <button onclick={() => drill(row.id, row.name)}
                    >{t("Transactions")}
                    <span class="sr-only">{row.name}</span></button
                  ><button
                    onclick={() => begin("categories", row)}
                    disabled={offline || !!pending || !!storageError}
                    >{t("Edit")} <span class="sr-only">{row.name}</span></button
                  >
                </div>
              </div>{/each}
            {#if !children.length}<p class="muted">
                {t("No categories in this group.")}
              </p>{/if}
          </div>{/if}
      {/each}
    </section>{/each}
{/if}

<style>
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  .category-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }
  .category-editor {
    max-width: 42rem;
  }
  .category-editor fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    min-width: 0;
  }
  .category-editor input,
  .category-editor select {
    max-width: 100%;
  }
  .category-editor .category-actions {
    margin-top: 1.5rem;
  }
  .category-section {
    margin-top: 2rem;
  }
  .category-group {
    margin-bottom: 1.5rem;
  }
  .group-heading,
  .category-row,
  .category-history article {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }
  .group-heading {
    border-bottom: 1px solid var(--line);
    padding: 0.5rem 0;
  }
  .group-heading h3 {
    margin: 0;
  }
  .category-row,
  .category-history article {
    padding: 0.75rem 0;
    border-bottom: 1px solid var(--line);
  }
  .category-row > div:first-child {
    min-width: 0;
  }
  .amount {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .category-history p {
    margin: 0.25rem 0;
  }
  .category-row strong,
  .group-heading h3 {
    overflow-wrap: anywhere;
  }
  @media (max-width: 700px) {
    .category-row {
      align-items: flex-start;
      flex-direction: column;
    }
    .category-row .category-actions {
      width: 100%;
    }
    .category-row .category-actions button {
      flex: 1;
    }
    .category-history article {
      align-items: flex-start;
    }
  }
</style>
