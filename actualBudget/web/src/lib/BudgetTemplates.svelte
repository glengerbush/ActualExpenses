<script lang="ts">
  import { onMount } from "svelte";
  import { request, errorMessage, uuid } from "./api";
  import { mutate, requestCancellation } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    observeVersions,
    nextEditClock,
    resetCorrectedClock,
  } from "./edit-clock";
  import { captureAmountFormat, formatAmount, parseAmount } from "./money";
  import type { PlanView } from "./planning";
  import {
    automationKey,
    readAutomationDraft,
    type AutomationDraft,
    type DefinitionPage,
    type TemplateDefinition,
    type TemplatePlan,
  } from "./budget-automations";
  let {
    budgetId,
    month,
    disconnected,
    onclose,
    onchanged,
  }: {
    budgetId: string;
    month: string;
    disconnected: boolean;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  let data = $state.raw<DefinitionPage | null>(null),
    draft = $state.raw<AutomationDraft | null>(null),
    selected = $state<string[]>([]),
    busy = $state(false),
    error = $state(""),
    notice = $state(""),
    broken = $state(false),
    storageBlocked = $state(false),
    parsed = $state.raw<{
      templates: Record<string, unknown>[];
      errors: { line: string; error: string }[];
    } | null>(null);
  let checkpoint: string | null = null;
  const enabled = $derived(data?.experimental.goalTemplatesEnabled === true);
  function save(next: AutomationDraft | null) {
    try {
      if (localStorage.getItem(automationKey(budgetId)) !== checkpoint)
        throw new Error(
          "Another tab changed this template work. Keep your retained fields and reload before sending.",
        );
      const raw = next ? JSON.stringify(next) : null;
      if (raw === null) localStorage.removeItem(automationKey(budgetId));
      else localStorage.setItem(automationKey(budgetId), raw);
      checkpoint = raw;
      draft = next;
      storageBlocked = false;
    } catch (e) {
      if (next) draft = next;
      storageBlocked = true;
      notice = "";
      throw e;
    }
  }
  function retryStorage() {
    if (!draft || busy) return;
    try {
      save(structuredClone(draft));
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function load() {
    if (disconnected) {
      error =
        "Reconnect to load confirmed templates. Your saved fields remain on this device.";
      return;
    }
    busy = true;
    error = "";
    try {
      const shown = draft?.month ?? month;
      const [definitions, view] = await Promise.all([
        request<{ data: DefinitionPage }>(
          `/budgets/${budgetId}/budget/templates?month=${shown}`,
        ),
        request<{ data: PlanView }>(
          `/budgets/${budgetId}/budget?month=${shown}`,
        ),
      ]);
      if (definitions.data.snapshot !== view.data.snapshot)
        throw new Error(
          "The budget changed while loading. Refresh to review one confirmed plan.",
        );
      observeVersions({ budget: view.data.edit_clock });
      data = definitions.data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    try {
      checkpoint = localStorage.getItem(automationKey(budgetId));
      draft = readAutomationDraft(budgetId);
      selected = Array.isArray(draft?.body.category_ids)
        ? (draft.body.category_ids as string[])
        : [];
      void load();
    } catch (e) {
      broken = true;
      error = errorMessage(e);
    }
  });
  function editNotes(row: TemplateDefinition, savings = false) {
    if (!data || draft || busy) return;
    try {
      save({
        version: 1,
        mode: "notes",
        month: data.month,
        method: data.method,
        epoch: data.epoch,
        snapshot: data.snapshot,
        body: {
          action: "category_notes",
          month: data.month,
          category: row.id,
          notes: row.notes,
          snapshot: data.snapshot,
        },
        originalNotes: row.notes,
        categoryName: row.name,
        format: captureAmountFormat(),
        preview: null,
        pending: null,
      });

      parsed = null;
      notice = savings
        ? "Enter a target, then review the resulting notes before saving."
        : "";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function notes(value: string) {
    if (!draft || draft.pending || busy) return;
    try {
      save({
        ...structuredClone(draft),
        body: { ...draft.body, notes: value },
      });
      parsed = null;
      notice = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function goalText(value: string) {
    if (!draft || draft.pending || busy) return;
    try {
      save({ ...structuredClone(draft), goalInput: value });
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function setGoal() {
    if (!draft || draft.mode !== "notes" || draft.pending) return;
    try {
      const amount = parseAmount(draft.goalInput ?? "", {
        ...draft.format,
        decimal_places: 2,
      });
      if (amount === null)
        throw new Error("Enter a target with at most two fractional digits.");
      const lines = String(draft.body.notes).split("\n");
      const targets = lines
        .map((line, index) => ({ line, index, start: line.indexOf("#") }))
        .filter(
          ({ line, start }) =>
            start >= 0 && line.slice(start).trim().startsWith("#goal"),
        );
      if (targets.length > 1)
        throw new Error(
          "Multiple goal directives need review. Keep one target in the notes before using this control.",
        );
      const directive = `#goal ${amount / 100}`;
      if (targets.length) {
        const old = targets[0];
        lines[old.index] = old.line.slice(0, old.start) + directive;
        notes(lines.join("\n"));
      } else {
        const old = String(draft.body.notes);
        notes(old + (old && !old.endsWith("\n") ? "\n" : "") + directive);
      }
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function checkNotes() {
    if (!draft || draft.mode !== "notes" || busy || disconnected) return;
    busy = true;
    error = "";
    try {
      parsed = (
        await request<{ data: typeof parsed }>(
          `/budgets/${budgetId}/budget/templates/parse`,
          { notes: draft.body.notes },
        )
      ).data;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function preview(mode: "apply" | "overwrite", ids?: string[]) {
    if (!data || draft || busy || disconnected || !enabled) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (ids && !ids.length) throw new Error("Select at least one category.");
      const body: Record<string, unknown> = {
        action: mode,
        month: data.month,
        snapshot: data.snapshot,
        ...(ids ? { category_ids: [...ids] } : {}),
      };
      save({
        version: 1,
        mode,
        month: data.month,
        method: data.method,
        epoch: data.epoch,
        snapshot: data.snapshot,
        body,
        format: captureAmountFormat(),
        preview: null,
        pending: null,
      });
      const requestBody = {
        month: data.month,
        snapshot: data.snapshot,
        force: mode === "overwrite",
        ...(ids ? { category_ids: [...ids] } : {}),
      };
      const result = (
        await request<{ data: TemplatePlan }>(
          `/budgets/${budgetId}/budget/templates/preview`,
          requestBody,
        )
      ).data;
      if (result.epoch !== draft!.epoch || result.snapshot !== draft!.snapshot)
        throw new Error(
          "The budget generation changed. Keep this review and reopen the current plan.",
        );
      save({ ...structuredClone(draft!), preview: result });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function send(cancel = false) {
    if (!draft || busy || disconnected || broken || storageBlocked) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (!draft.pending) {
        if (data?.method !== draft.method)
          throw new Error(
            "The budgeting method changed. Keep these fields, close the review and reopen the current method.",
          );
        if (data?.epoch !== draft.epoch)
          throw new Error(
            "This budget was restored. Keep these fields and explicitly review the current generation before preparing another change.",
          );
        if (
          draft.mode !== "notes" &&
          (!draft.preview || draft.preview.errors.length)
        )
          throw new Error(
            "Resolve template errors before applying this captured plan.",
          );
        const pending = {
          path: `/budgets/${budgetId}/budget${draft.mode === "notes" ? "" : "/templates/commands"}`,
          method: "POST" as const,
          body: structuredClone(draft.body),
          key: uuid(),
          edit_clock: nextEditClock(),
          budgetEpoch: draft.epoch,
          historyOwner: await historyOwner(),
        };
        save({ ...structuredClone(draft), pending });
      }
      if (cancel) requestCancellation(draft.pending!);
      save(structuredClone(draft));
      try {
        await mutate(draft.pending!, cancel);
      } catch (e) {
        if (
          e instanceof Error &&
          "code" in e &&
          e.code === "operation_canceled"
        ) {
          resetCorrectedClock();
          save({ ...structuredClone(draft), pending: null });
          notice =
            "The saved request was cancelled. Retained fields remain. Review them against the current budget before preparing a new request.";
          return;
        }
        throw e;
      }
      save(null);
      parsed = null;
      await onchanged();
      notice = "Template change saved.";
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function reviewRetained() {
    if (!draft || draft.pending || busy || disconnected) return;
    await load();
    notice =
      "Review the current notes and budgeting method below. Choosing to use this budget creates a new request; the previous request is never changed.";
  }
  function useCurrent() {
    if (!draft || draft.pending || !data) return;
    try {
      if (draft.mode !== "notes")
        throw new Error(
          "Close this allocation review and preview the current plan again.",
        );
      save({
        ...structuredClone(draft),
        epoch: data.epoch,
        method: data.method,
        snapshot: data.snapshot,
        body: { ...draft.body, snapshot: data.snapshot },
        originalNotes:
          data.categories.find((c) => c.id === draft!.body.category)?.notes ??
          draft.originalNotes,
      });
      notice =
        "Retained notes now have the explicitly reviewed budget context. Review and save when ready.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function discard() {
    if (busy || draft?.pending) return;
    try {
      save(null);
      parsed = null;
      notice = "";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function close() {
    if (draft) {
      error =
        "Finish, cancel or discard the saved template work before closing this review.";
      return;
    }
    onclose();
  }
</script>

<section class="budget-templates" aria-labelledby="goal-template-title">
  <header>
    <div>
      <h2 id="goal-template-title">Goal templates</h2>
      <p>
        Review saved directives and savings targets. Applied templates change
        the budget plan; confirmed expenses and account balances stay separate.
      </p>
    </div>
    <button onclick={close} disabled={busy}>Close goal templates</button>
  </header>
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p
      role="status"
    >
      {notice}
    </p>{/if}
  {#if storageBlocked}<p class="error" role="alert">
      These fields are retained in this view but are not safely saved on this
      device. Sending is blocked.
    </p>
    <button onclick={retryStorage} disabled={busy}
      >Retry saving retained template work</button
    >{/if}
  {#if !enabled && !draft?.pending}<p role="status">
      Goal templates are experimental. Enable Goal templates in Settings →
      Experimental features to apply them. Existing notes and targets remain
      retained.
    </p>{/if}
  {#if draft}<section
      class="template-draft"
      aria-labelledby="template-draft-title"
    >
      <h3 id="template-draft-title">
        {draft.mode === "notes"
          ? `${draft.categoryName} template notes`
          : "Captured template allocation"}
      </h3>
      <p>
        {draft.month} · {draft.method === "tracking" ? "Tracking" : "Envelope"} ·
        This review keeps the plan and budgeting method you opened.
      </p>
      {#if draft.mode !== "notes"}<p>
          Scope: {Array.isArray(draft.body.category_ids)
            ? `${draft.body.category_ids.length} explicitly selected categories`
            : "All eligible categories"} · {draft.mode === "overwrite"
            ? "Replace existing allocations"
            : "Skip existing allocations"}
        </p>{/if}
      {#if draft.pending}<p role="status">
          The original request is saved. Retry keeps its fields, logical clock
          and budget generation. Cancellation requires a confirmed server
          outcome.
        </p>{/if}
      {#if draft.mode === "notes"}<fieldset
          disabled={busy || !!draft.pending || broken}
        >
          <label
            >Category template notes<textarea
              rows="7"
              value={String(draft.body.notes)}
              oninput={(e) => notes(e.currentTarget.value)}></textarea></label
          >
          <p>
            Examples: <code>#template 25</code> allocates a monthly amount;
            <code>#goal 100</code> tracks a savings target. Ordinary text and multiple
            directives are retained.
          </p>
          <div class="goal-input">
            <label
              >Savings target<input
                inputmode="decimal"
                value={draft.goalInput ?? ""}
                oninput={(e) => goalText(e.currentTarget.value)}
              /></label
            ><button onclick={setGoal}>Use savings target</button>
          </div>
          <button onclick={checkNotes} disabled={disconnected}
            >Check directives</button
          >
        </fieldset>
        {#if parsed}<h4>Parsed directives</h4>
          <ul>
            {#each parsed.templates as template, index (index)}<li>
                {String(template.type)}{template.description
                  ? ` · ${String(template.description)}`
                  : ""}{template.type === "error"
                  ? ` · ${String(template.error)}`
                  : ""}
              </li>{:else}<li>No template or goal directives found.</li>{/each}
          </ul>{/if}
      {:else if draft.preview}<section
          class="template-preview"
          aria-labelledby="template-preview-title"
        >
          <h4 id="template-preview-title">Proposed allocations and targets</h4>
          {#if draft.preview.errors.length}<ul class="error" role="alert">
              {#each draft.preview.errors as issue, index (index)}<li>
                  {issue.name}: {issue.error}
                </li>{/each}
            </ul>{/if}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable financial preview needs keyboard access at enlarged text.) -->
          <div
            class="table-scroll"
            role="region"
            aria-label="Template allocation preview"
            tabindex="0"
          >
            <table>
              <thead
                ><tr
                  ><th>Category</th><th>Current plan</th><th>New plan</th><th
                    >Goal or demand</th
                  ></tr
                ></thead
              ><tbody
                >{#each draft.preview.categories.filter((row) => row.applies || row.clear_goal) as row (row.id)}<tr
                    ><th scope="row">{row.name}</th><td
                      >{formatAmount(row.before_budgeted)}</td
                    ><td>{formatAmount(row.budgeted)}</td><td
                      >{row.goal === null
                        ? "No target"
                        : formatAmount(row.goal)}{row.long_goal === 1
                        ? " · Savings goal"
                        : " · Monthly demand"}</td
                    ></tr
                  >{:else}<tr
                    ><td colspan="4"
                      >No eligible changes. Existing allocations are skipped
                      unless explicitly overwritten or selected.</td
                    ></tr
                  >{/each}</tbody
              >
            </table>
          </div>
        </section>{:else}<p>
          Preview was interrupted. Close this review, refresh the confirmed plan
          and preview again. No money was changed by the preview.
        </p>{/if}
      <div class="toolbar">
        {#if draft.pending}<button
            class="primary"
            onclick={() => send()}
            disabled={busy || disconnected || storageBlocked}
            >Retry saved template change</button
          ><button
            onclick={() => send(true)}
            disabled={busy || disconnected || storageBlocked}
            >Cancel saved template change safely</button
          >{:else}<button
            class="primary"
            onclick={() => send()}
            disabled={busy ||
              disconnected ||
              broken ||
              storageBlocked ||
              (draft.mode !== "notes" &&
                (!draft.preview || !!draft.preview.errors.length))}
            >{draft.mode === "notes"
              ? "Save template notes"
              : "Apply captured templates"}</button
          ><button onclick={discard} disabled={busy}
            >Discard template review</button
          >{/if}
      </div>
      {#if !draft.pending}<details>
          <summary>Review retained fields against the current budget</summary
          ><button onclick={reviewRetained} disabled={busy || disconnected}
            >Load current budget for review</button
          >{#if data}<p>Current method: {data.method}. Current notes:</p>
            <p class="saved-notes">
              {data.categories.find((c) => c.id === draft!.body.category)
                ?.notes ??
                "Allocation review: preview again to use current figures."}
            </p>
            {#if draft.mode === "notes"}<button
                onclick={useCurrent}
                disabled={busy}>Use current budget with retained notes</button
              >{/if}{/if}
        </details>{/if}
    </section>{/if}
  {#if data}<div class="toolbar">
      <button onclick={load} disabled={busy || disconnected}
        >Refresh template definitions</button
      ><button
        onclick={() => preview("apply")}
        disabled={busy || !!draft || disconnected || !enabled}
        >Preview unallocated templates</button
      ><button
        onclick={() => preview("overwrite")}
        disabled={busy || !!draft || disconnected || !enabled}
        >Preview overwrite all templates</button
      ><button
        onclick={() => preview("overwrite", selected)}
        disabled={busy ||
          !!draft ||
          disconnected ||
          !enabled ||
          !selected.length}>Preview selected templates</button
      >
    </div>
    <p>
      {data.month} · {selected.length} selected. All-category application excludes
      hidden categories and groups; explicit selection can include them. A preview
      always precedes allocation.
    </p>
    <ul class="definitions">
      {#each data.categories as row (row.id)}<li>
          <div class="category-heading">
            <label
              ><input
                type="checkbox"
                aria-label={`Select ${row.name}`}
                checked={selected.includes(row.id)}
                disabled={busy ||
                  !!draft ||
                  (data.method === "envelope" && row.is_income)}
                onchange={(e) =>
                  (selected = e.currentTarget.checked
                    ? [...selected, row.id]
                    : selected.filter((id) => id !== row.id))}
              /><strong>{row.name}</strong></label
            ><span
              >{row.group_name} · {row.source === "notes"
                ? "Notes"
                : "Visual definitions"}{row.hidden || row.group_hidden
                ? " · Hidden"
                : ""}</span
            >
          </div>
          {#if data.method === "envelope" && row.is_income}<p>
              Income targets apply in Tracking budgets. Notes remain available.
            </p>{/if}
          <p>
            {row.templates.length}
            {row.templates.length === 1 ? "directive" : "directives"}
          </p>
          {#if row.parse_errors.length && row.source === "notes"}<ul
              class="error"
            >
              {#each row.parse_errors as issue, index (index)}<li>
                  {issue.line}: {issue.error}
                </li>{/each}
            </ul>{/if}
          <details>
            <summary>Saved notes and directives</summary>
            <p class="saved-notes">{row.notes || "No notes"}</p>
            <ul>
              {#each row.templates as template, index (index)}<li>
                  {String(template.type)}{template.description
                    ? ` · ${String(template.description)}`
                    : ""}
                </li>{/each}
            </ul>
          </details>
          <div class="toolbar">
            <button
              onclick={() => editNotes(row)}
              disabled={busy || !!draft}
              aria-label={`Edit ${row.name} template notes`}
              >{row.source === "notes"
                ? "Edit template notes"
                : "Edit retained notes"}</button
            >{#if row.source === "notes" && !(data.method === "envelope" && row.is_income)}<button
                onclick={() => editNotes(row, true)}
                disabled={busy || !!draft}
                aria-label={`Set ${row.name} savings target`}
                >Set savings target</button
              >{:else}<p>
                Visual definitions remain authoritative; retained note edits do
                not change this plan.
              </p>{/if}
          </div>
        </li>{/each}
    </ul>{:else if busy}<p role="status">Loading confirmed templates…</p>{/if}
</section>

<style>
  .budget-templates {
    margin-block: 1.5rem;
    border-top: 1px solid var(--border);
    padding-block: 1.5rem;
  }
  header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 1rem;
  }
  h2 {
    margin-top: 0;
  }
  header p {
    max-width: 58rem;
  }
  .toolbar,
  .goal-input,
  .category-heading {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    margin-block: 1rem;
  }
  .toolbar > button,
  .goal-input > button {
    min-height: 44px;
  }
  label {
    display: grid;
    gap: 0.4rem;
  }
  textarea {
    width: 100%;
    resize: vertical;
  }
  .goal-input label {
    flex: 1 1 15rem;
  }
  .template-draft {
    padding: 1rem;
    border: 1px solid var(--border);
    margin-block: 1.25rem;
  }
  .definitions {
    list-style: none;
    padding: 0;
  }
  .definitions > li {
    padding-block: 1.25rem;
    border-top: 1px solid var(--border);
  }
  .category-heading label {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .category-heading input {
    width: 1.25rem;
    height: 1.25rem;
    min-width: 1.25rem;
    min-height: 1.25rem;
    padding: 0;
  }
  .category-heading > span {
    color: var(--muted);
  }
  .saved-notes {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .table-scroll {
    overflow: auto;
    max-width: 100%;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 0.75rem;
    text-align: left;
    border-bottom: 1px solid var(--border);
    font-variant-numeric: tabular-nums;
  }
  td {
    white-space: nowrap;
  }
  fieldset {
    border: 0;
    padding: 0;
    min-width: 0;
  }
  details {
    margin-block: 1rem;
  }
  code {
    overflow-wrap: anywhere;
  }
  @media (max-width: 40rem) {
    header {
      flex-direction: column;
    }
    .toolbar > button,
    .goal-input > button {
      flex: 1 1 13rem;
    }
    .template-draft {
      padding: 0.75rem;
    }
  }
</style>
