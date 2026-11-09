<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import {
    LocalClockError,
    nextEditClock,
    observeVersions,
    resetCorrectedClock,
  } from "./edit-clock";
  import { offlineStatus, subscribeOffline } from "./offline/delivery";
  import {
    readReferenceDraft,
    saveReferenceDraft,
    clearReferenceDraft,
    referenceCancelKey,
    type ReferenceDraft,
    type ReferenceIntent,
  } from "./reference-actions";
  let {
    budgetId,
    onchanged,
    onclose,
  }: { budgetId: string; onchanged: () => Promise<void>; onclose: () => void } =
    $props();
  interface Row {
    id: string;
    name?: string;
    tag?: string;
    favorite?: boolean;
    hidden?: boolean;
    deleted: boolean;
    transfer_account_id?: string | null;
    transaction_count?: number;
    color?: string | null;
    description?: string | null;
    field_versions?: Record<string, string>;
  }
  let draft = $state.raw<ReferenceDraft | null>(null),
    rows = $state.raw<Row[]>([]),
    busy = $state(false),
    error = $state(""),
    clockRecovery = $state(false),
    notice = $state(""),
    search = $state(""),
    visibility = $state("all"),
    online = $state(offlineStatus.reachable),
    confirmRemove = $state(false),
    currentEpoch = "";
  let visible = $derived(
    rows.filter(
      (r) =>
        (r.name ?? r.tag ?? "")
          .toLocaleLowerCase()
          .includes(search.toLocaleLowerCase()) &&
        (visibility !== "unused" || r.transaction_count === 0) &&
        (visibility !== "hidden" || r.hidden) &&
        (visibility !== "visible" || !r.hidden),
    ),
  );
  let locked = $derived(busy || !!draft?.pending || !online),
    kind = $derived(draft?.kind ?? "payees");
  async function snapshot() {
    const checkpoint = await request<{
      data: { epoch: string; snapshot: string };
    }>(`/budgets/${budgetId}/checkpoint`);
    currentEpoch = checkpoint.data.epoch;
    const values: Row[] = [];
    let offset: number | null = 0;
    do {
      const page: { data: Row[]; next_offset: number | null } = await request(
        `/budgets/${budgetId}/${draft?.kind ?? "payees"}?include_hidden=true&limit=1000&offset=${offset}&snapshot=${checkpoint.data.snapshot}`,
      );
      values.push(...page.data);
      offset = page.next_offset;
    } while (offset !== null);
    rows = values;
    for (const row of rows) observeVersions(row.field_versions);
  }
  async function load() {
    busy = true;
    error = "";
    try {
      draft = readReferenceDraft(budgetId);
      await snapshot();
      if (!draft)
        draft = saveReferenceDraft(
          {
            version: 1,
            budget: budgetId,
            revision: "",
            epoch: currentEpoch,
            kind: "payees",
            selected: [],
            editing: "",
            name: "",
            color: "",
            description: "",
            target: "",
            pending: null,
          },
          null,
        );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function change(patch: Partial<ReferenceDraft>) {
    if (!draft || locked) return;
    try {
      draft = saveReferenceDraft({ ...draft, ...patch }, draft);
      error = "";
      confirmRemove = false;
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function chooseKind(value: "payees" | "tags") {
    if (locked) return;
    visibility = "all";
    change({
      kind: value,
      selected: [],
      editing: "",
      name: "",
      color: "",
      description: "",
      target: "",
    });
    busy = true;
    try {
      await snapshot();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function select(id: string, checked: boolean) {
    if (!draft) return;
    change({
      selected: checked
        ? [...draft.selected, id]
        : draft.selected.filter((v) => v !== id),
    });
  }
  function edit(row: Row) {
    change({
      editing: row.id,
      name: row.name ?? row.tag ?? "",
      color: row.color ?? "",
      description: row.description ?? "",
    });
  }
  async function prepare(
    path: string,
    body: Record<string, unknown>,
    method: "POST" | "PATCH" = "POST",
  ) {
    if (!draft || locked) return;
    error = "";
    try {
      if (!navigator.onLine || !offlineStatus.reachable)
        throw new Error("Reconnect before managing payees or tags.");
      const intent: ReferenceIntent = {
        path: `/budgets/${budgetId}/${path}`,
        body,
        key: uuid(),
        method,
        clock: nextEditClock(),
        epoch: draft.epoch,
      };
      draft = saveReferenceDraft({ ...draft, pending: intent }, draft);
      clockRecovery = false;
      await send();
    } catch (e) {
      clockRecovery = e instanceof LocalClockError;
      error = errorMessage(e);
    }
  }
  function useCorrectedTime() {
    if (!draft || locked || !clockRecovery) return;
    try {
      draft = saveReferenceDraft({ ...draft }, draft);
      resetCorrectedClock();
      for (const row of rows) observeVersions(row.field_versions);
      clockRecovery = false;
      error = "";
      notice = "Device time adopted. Review your form, then save.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function send() {
    if (!draft?.pending || busy) return;
    const intent = draft.pending;
    busy = true;
    error = "";
    try {
      const result = await request<{ conflicts?: unknown[] }>(
        intent.path,
        intent.body,
        intent.key,
        intent.method,
        intent.clock,
        localStorage.getItem(referenceCancelKey(intent)) === "true",
        intent.epoch,
      );
      notice = result.conflicts?.length
        ? "A newer edit was retained. Review the refreshed record."
        : "Changes saved.";
      draft = saveReferenceDraft(
        {
          ...draft,
          pending: null,
          selected: [],
          editing: "",
          name: "",
          color: "",
          description: "",
          target: "",
        },
        draft,
      );
      localStorage.removeItem(referenceCancelKey(intent));
      await snapshot();
      draft = saveReferenceDraft({ ...draft, epoch: currentEpoch }, draft);
      await onchanged();
    } catch (e) {
      if (
        e instanceof RequestError &&
        !e.uncertain &&
        e.code === "operation_canceled"
      ) {
        try {
          draft = saveReferenceDraft({ ...draft!, pending: null }, draft);
          localStorage.removeItem(referenceCancelKey(intent));
          notice = "Pending change canceled. Your form is retained.";
          await snapshot();
          draft = saveReferenceDraft({ ...draft, epoch: currentEpoch }, draft);
        } catch (failure) {
          error = errorMessage(failure);
        }
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function cancel() {
    if (!draft?.pending || busy) return;
    const original = draft.pending;
    busy = true;
    try {
      draft = saveReferenceDraft({ ...draft }, draft);
      const info = await request<{ data: { capabilities: string[] } }>("/info");
      if (!info.data.capabilities.includes("reference_cancellation"))
        throw new Error(
          "This server cannot safely cancel this request. Retry it unchanged.",
        );
      draft = saveReferenceDraft({ ...draft }, draft);
      if (draft.pending !== original)
        throw new Error("The pending change changed. Reload saved work.");
      localStorage.setItem(referenceCancelKey(original), "true");
      busy = false;
      await send();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function save() {
    if (!draft) return;
    const name = draft.name.trim();
    if (!name) {
      error = "Enter a name.";
      return;
    }
    if (kind === "tags" && /[#\s]/u.test(name)) {
      error = "Use one word without #.";
      return;
    }
    const body: Record<string, unknown> =
      kind === "payees"
        ? { name }
        : {
            tag: name,
            color: draft.color || null,
            description: draft.description || null,
          };
    if (!draft.editing) body.id = uuid();
    await prepare(
      `${kind}${draft.editing ? `/${draft.editing}` : ""}`,
      body,
      draft.editing ? "PATCH" : "POST",
    );
  }
  function batch(action: string) {
    if (!draft) return;
    void prepare(`${kind}/batch`, {
      action,
      ids: draft.selected,
      ...(action === "merge" ? { target_id: draft.target } : {}),
    });
  }
  function done() {
    if (!draft || busy) return;
    try {
      clearReferenceDraft(draft);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(() => {
    void load();
    return subscribeOffline(() => {
      online = offlineStatus.reachable;
    });
  });
</script>

<section aria-labelledby="reference-title" class="reference-management">
  <header>
    <h1 id="reference-title">{t("Payees and tags")}</h1>
    <button onclick={done} disabled={busy || !!draft?.pending}
      >{t("Done")}</button
    >
  </header>
  <p class="hint">
    Clean up names and organize transactions with tags in notes.
  </p>
  {#if error}<p class="error" role="alert">{error}</p>
    <button onclick={load} disabled={busy}>{t("Reload saved work")}</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if clockRecovery && !draft?.pending}
    <p>
      Correct your device time, then retry with a new edit clock. Your form
      stays here.
    </p>
    <button onclick={useCorrectedTime} disabled={locked}
      >{t("Use corrected device time")}</button
    >
  {/if}
  {#if !online}<p role="status">
      Reconnect to manage payees or tags. Your saved work stays here.
    </p>{/if}
  {#if busy}<p role="status">{t("Loading or saving\u2026")}</p>{/if}
  {#if draft?.pending}<div class="pending">
      <h2>{t("Pending change")}</h2>
      <p>
        Retry keeps the original request. Cancel checks whether it was already
        saved.
      </p>
      <button onclick={send} disabled={busy}>{t("Retry pending change")}</button
      ><button onclick={cancel} disabled={busy}
        >{t("Cancel pending change")}</button
      >
    </div>{/if}
  {#if draft}
    <nav aria-label="Manage records">
      <button
        aria-pressed={kind === "payees"}
        onclick={() => chooseKind("payees")}
        disabled={locked}>{t("Payees")}</button
      ><button
        aria-pressed={kind === "tags"}
        onclick={() => chooseKind("tags")}
        disabled={locked}>{t("Tags")}</button
      >
    </nav>
    <div class="management-columns">
      <div>
        <label for="reference-search">Search {kind}</label><input
          id="reference-search"
          type="search"
          bind:value={search}
        />
        <label for="reference-visibility">{t("Show")}</label><select
          id="reference-visibility"
          bind:value={visibility}
          ><option value="all">All {kind}</option>{#if kind === "payees"}<option
              value="unused">{t("Without transactions")}</option
            >{:else}<option value="visible">{t("Visible tags")}</option><option
              value="hidden">{t("Hidden tags")}</option
            >{/if}</select
        >
        {#if visible.length}
          <label class="select-all"
            ><input
              type="checkbox"
              checked={visible.every((r) => draft!.selected.includes(r.id))}
              disabled={locked}
              onchange={(e) =>
                change({
                  selected: e.currentTarget.checked
                    ? visible.map((r) => r.id)
                    : [],
                })}
            />Select displayed {kind}</label
          >
          <ul class="reference-list">
            {#each visible as row (row.id)}<li>
                <label class="row-select"
                  ><input
                    type="checkbox"
                    checked={draft.selected.includes(row.id)}
                    disabled={locked}
                    onchange={(e) => select(row.id, e.currentTarget.checked)}
                  /><span
                    >{row.name ?? `#${row.tag}`}{#if row.hidden}<small
                        >{t("Hidden")}</small
                      >{/if}{#if row.favorite}<small>{t("Favorite")}</small
                      >{/if}{#if kind === "payees"}<small
                        >{row.transaction_count} transactions</small
                      >{/if}</span
                  ></label
                ><button
                  onclick={() => edit(row)}
                  disabled={locked}
                  aria-label={`Edit ${row.name ?? row.tag}`}>{t("Edit")}</button
                >
              </li>{/each}
          </ul>
        {:else if !busy}<p>No {kind} match this view.</p>{/if}
        {#if draft.selected.length}<div class="bulk">
            <h2>{draft.selected.length} selected</h2>
            {#if kind === "payees"}<label for="merge-target"
                >{t("Merge into")}</label
              ><select
                id="merge-target"
                value={draft.target}
                onchange={(e) => change({ target: e.currentTarget.value })}
                disabled={locked}
                ><option value="">{t("Choose the payee to keep")}</option
                >{#each rows.filter((r) => !draft!.selected.includes(r.id)) as row (row.id)}<option
                    value={row.id}>{row.name}</option
                  >{/each}</select
              ><button
                onclick={() => batch("merge")}
                disabled={locked || !draft.target}
                >{t("Merge selected payees")}</button
              ><button onclick={() => batch("favorite")} disabled={locked}
                >{t("Favorite")}</button
              ><button onclick={() => batch("unfavorite")} disabled={locked}
                >{t("Unfavorite")}</button
              >{:else}<button onclick={() => batch("hide")} disabled={locked}
                >{t("Hide selected tags")}</button
              ><button onclick={() => batch("show")} disabled={locked}
                >{t("Show selected tags")}</button
              >{/if}<button
              onclick={() => (confirmRemove = true)}
              disabled={locked}>Remove selected {kind}</button
            >{#if confirmRemove}<p>
                {kind === "payees"
                  ? "Remove these payees from selection? Transaction history and imported text are retained."
                  : "Remove these tags from management? Tags already written in notes remain."}
              </p>
              <button
                class="danger"
                onclick={() => batch("remove")}
                disabled={locked}>{t("Confirm removal")}</button
              ><button onclick={() => (confirmRemove = false)}
                >{t("Keep records")}</button
              >{/if}
          </div>{/if}
      </div>
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <h2>
          {draft.editing ? "Edit" : "Add"}
          {kind === "payees" ? "payee" : "tag"}
        </h2>
        <label for="reference-name"
          >{kind === "payees" ? "Payee name" : "Tag name"}</label
        ><input
          id="reference-name"
          value={draft.name}
          oninput={(e) => change({ name: e.currentTarget.value })}
          disabled={locked}
          maxlength={kind === "payees" ? 120 : 1000}
          required
        />
        {#if kind === "tags"}<p class="hint">
            One word, without #. Renaming updates matching tags in transaction
            notes.
          </p>
          <label for="tag-color">Color (optional)</label><input
            id="tag-color"
            placeholder="#RRGGBB"
            value={draft.color}
            oninput={(e) => change({ color: e.currentTarget.value })}
            disabled={locked}
            pattern="#[0-9a-fA-F]{6}"
          /><label for="tag-description">Description (optional)</label><textarea
            id="tag-description"
            value={draft.description}
            oninput={(e) => change({ description: e.currentTarget.value })}
            disabled={locked}
            maxlength="10000"></textarea>{/if}
        <button class="primary" disabled={locked}
          >{draft.editing
            ? "Save changes"
            : kind === "payees"
              ? "Add payee"
              : "Add tag"}</button
        >{#if draft.editing}<button
            type="button"
            onclick={() =>
              change({ editing: "", name: "", color: "", description: "" })}
            disabled={locked}>{t("Add another")}</button
          >{/if}
        {#if kind === "tags"}<button
            type="button"
            onclick={() => prepare("tags/discover", {})}
            disabled={locked}>{t("Discover tags from notes")}</button
          >{/if}
      </form>
    </div>
  {/if}
</section>

<style>
  header,
  nav,
  .bulk {
    display: flex;
    align-items: center;
    gap: var(--space-2, 0.5rem);
    flex-wrap: wrap;
  }
  header {
    justify-content: space-between;
  }
  .management-columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(16rem, 0.8fr);
    gap: 2rem;
    margin-top: 1.5rem;
  }
  .reference-list {
    list-style: none;
    padding: 0;
  }
  .reference-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid var(--line);
    padding: 0.5rem 0;
  }
  .row-select,
  .select-all {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-height: 44px;
  }
  .row-select {
    margin: 0;
  }
  nav button[aria-pressed="true"] {
    background: var(--selected);
    color: var(--selected-text);
  }
  .row-select span {
    overflow-wrap: anywhere;
  }
  .row-select input,
  .select-all input {
    width: auto;
  }
  .row-select small {
    display: block;
    font-weight: normal;
  }
  .bulk h2 {
    flex-basis: 100%;
  }
  .pending {
    padding: 1rem 0;
    border-block: 1px solid var(--line);
  }
  form {
    align-self: start;
  }
  form button {
    margin-top: 0.75rem;
    margin-right: 0.5rem;
  }
  nav {
    margin-top: 1.5rem;
  }
  @media (max-width: 700px) {
    .management-columns {
      grid-template-columns: 1fr;
      gap: 1.5rem;
    }
  }
</style>
