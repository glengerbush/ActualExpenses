<script lang="ts">
  import { prepareVisualBody } from "./visual-automations";
  import TemplateEditor from "./TemplateEditor.svelte";
  import CleanupNotice from "./CleanupNotice.svelte";
  import CleanupEditor from "./CleanupEditor.svelte";
  import { preferences } from "./preferences.svelte";
  import type { SchedulePage } from "./schedules";
  import { onMount } from "svelte";
  import { request, errorMessage, uuid } from "./api";
  import { mutate, requestCancellation } from "./register-api";
  import { historyOwner } from "./history-owner";
  import {
    observeVersions,
    nextEditClock,
    resetCorrectedClock,
  } from "./edit-clock";
  import {
    captureAmountFormat,
    formatAmount,
    formatAmountInput,
    parseAmount,
    type AmountFormat,
  } from "./money";
  import type { PlanView } from "./planning";
  import {
    automationKey,
    readAutomationDraft,
    type AutomationDraft,
    type DefinitionPage,
    type TemplateDefinition,
    type TemplatePlan,
    type MigrationPreview,
    type CleanupNotification,
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
  let cleanupOutcome = $state.raw<CleanupNotification | null>(null);
  let schedules = $state.raw<{ id: string; name: string }[]>([]),
    nextScheduleOffset = $state<number | null>(0);
  function money(amount: number, format?: AmountFormat) {
    return format ? formatAmountInput(amount, format) : formatAmount(amount);
  }
  let checkpoint: string | null = null;
  const enabled = $derived(data?.experimental.goalTemplatesEnabled === true);
  const visualEnabled = $derived(
    data?.experimental.goalTemplatesUIEnabled === true,
  );
  const invalidVisual = $derived.by(() => {
    if (draft?.mode !== "configure" || draft.pending) return "";
    try {
      prepareVisualBody(draft.body, draft.format);
      return "";
    } catch (e) {
      return errorMessage(e);
    }
  });
  function issueText(issue: unknown) {
    return issue && typeof issue === "object" && "error" in issue
      ? String(issue.error)
      : String(issue);
  }

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
      cleanupOutcome = null;
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
  async function preview(
    mode: "apply" | "overwrite" | "cleanup",
    ids?: string[],
  ) {
    if (!data || draft || busy || disconnected || !(enabled || visualEnabled))
      return;
    busy = true;
    error = "";
    notice = "";
    cleanupOutcome = null;
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
        ...(mode === "cleanup"
          ? { action: "cleanup" }
          : { force: mode === "overwrite" }),
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
  function openCommand(
    mode: "configure" | "migrate" | "pool",
    row?: TemplateDefinition,
  ) {
    if (!data || draft || busy || !visualEnabled) return;
    try {
      const body: Record<string, unknown> =
        mode === "configure"
          ? {
              action: "configure",
              snapshot: data.snapshot,
              category_id: row!.id,
              source: "ui",
              templates: structuredClone(row!.templates),
              cleanup: structuredClone(row!.cleanup),
            }
          : mode === "migrate"
            ? {
                action: "migrate",
                month: data.month,
                snapshot: data.snapshot,
                category_id: row!.id,
                target_source: row!.source === "ui" ? "notes" : "ui",
              }
            : { action: "pool", snapshot: data.snapshot, name: "" };
      save({
        version: 1,
        mode,
        month: data.month,
        method: data.method,
        epoch: data.epoch,
        snapshot: data.snapshot,
        body,
        categoryName: row?.name,
        originalNotes: row?.notes,
        dateFormat: preferences.shared.date_format,
        format: captureAmountFormat(),
        preview: null,
        pending: null,
        migration: null,
        projection: null,
      });
      error = "";
      notice = "";
      cleanupOutcome = null;
      schedules = [];
      nextScheduleOffset = 0;
      if (mode === "migrate") void migrationPreview();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function commandField(key: string, value: unknown) {
    if (!draft || draft.pending || busy) return;
    try {
      save({
        ...structuredClone(draft),
        body: { ...draft.body, [key]: value },
        migration: null,
        projection: null,
      });
      error = "";
      notice = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function loadScheduleChoices() {
    if (!draft || busy || disconnected || nextScheduleOffset === null) return;
    busy = true;
    error = "";
    try {
      const page = (
        await request<{ data: SchedulePage }>(
          `/budgets/${budgetId}/schedules?limit=100&offset=${nextScheduleOffset}&snapshot=${encodeURIComponent(draft.snapshot)}`,
        )
      ).data;
      if (page.epoch !== draft.epoch || page.snapshot !== draft.snapshot)
        throw new Error(
          "The budget changed. Keep this draft and review it against the current budget.",
        );
      schedules = [
        ...schedules,
        ...page.schedules.map((s) => ({
          id: s.id,
          name: s.name ?? "Unnamed schedule",
        })),
      ];
      nextScheduleOffset = page.next_offset;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function migrationPreview() {
    if (
      !draft ||
      draft.mode !== "migrate" ||
      draft.pending ||
      busy ||
      disconnected
    )
      return;
    busy = true;
    error = "";
    try {
      const { action, ...body } = draft.body;
      const result = (
        await request<{ data: MigrationPreview }>(
          `/budgets/${budgetId}/budget/templates/migration-preview`,
          body,
        )
      ).data;
      if (result.snapshot !== draft.snapshot || result.epoch !== draft.epoch)
        throw new Error(
          "The captured budget changed; retain these fields and reopen migration.",
        );
      save({ ...structuredClone(draft), migration: result });
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function project() {
    if (
      !draft ||
      draft.mode !== "configure" ||
      draft.pending ||
      busy ||
      disconnected
    )
      return;
    busy = true;
    error = "";
    try {
      const result = (
        await request<{ data: NonNullable<AutomationDraft["projection"]> }>(
          `/budgets/${budgetId}/budget/templates/projection`,
          {
            month: draft.month,
            snapshot: draft.snapshot,
            category_id: draft.body.category_id,
            templates: prepareVisualBody(draft.body, draft.format).templates,
          },
        )
      ).data;
      save({ ...structuredClone(draft), projection: result });
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
    cleanupOutcome = null;
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
          ["apply", "overwrite", "cleanup"].includes(draft.mode) &&
          (!draft.preview || draft.preview.errors.length)
        )
          throw new Error(
            "Resolve template errors before applying this captured plan.",
          );
        if (draft.mode === "migrate" && !draft.migration?.compatible)
          throw new Error("Review a compatible migration before saving.");
        if (
          ["configure", "migrate", "pool"].includes(draft.mode) &&
          !visualEnabled
        )
          throw new Error(
            "Enable visual budget automations before preparing this change.",
          );
        const pending = {
          path: `/budgets/${budgetId}/budget${draft.mode === "notes" ? "" : "/templates/commands"}`,
          method: "POST" as const,
          body:
            draft.mode === "configure"
              ? prepareVisualBody(draft.body, draft.format)
              : structuredClone(draft.body),
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
        const response = await mutate(draft.pending!, cancel);
        if (draft.mode === "cleanup" && "notification" in response) {
          const value = response.notification;
          if (
            value &&
            typeof value === "object" &&
            "message" in value &&
            typeof value.message === "string" &&
            "type" in value &&
            typeof value.type === "string"
          ) {
            cleanupOutcome = value as CleanupNotification;
          }
        }
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
      notice = cleanupOutcome
        ? "Month-end cleanup saved."
        : "Template change saved.";
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
  {#if cleanupOutcome}<section aria-label="Confirmed cleanup outcome">
      <CleanupNotice notification={cleanupOutcome} committed={true} />
    </section>{/if}
  {#if storageBlocked}<p class="error" role="alert">
      These fields are retained in this view but are not safely saved on this
      device. Sending is blocked.
    </p>
    <button onclick={retryStorage} disabled={busy}
      >Retry saving retained template work</button
    >{/if}
  {#if !enabled && !visualEnabled && !draft?.pending}<p role="status">
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
          : draft.mode === "configure"
            ? `${draft.categoryName} visual automation`
            : draft.mode === "migrate"
              ? `${draft.categoryName} source migration`
              : draft.mode === "pool"
                ? "Named cleanup pool"
                : draft.mode === "cleanup"
                  ? "Captured month-end cleanup"
                  : "Captured template allocation"}
      </h3>
      <p>
        {draft.month} · {draft.method === "tracking" ? "Tracking" : "Envelope"} ·
        This review keeps the plan and budgeting method you opened.
      </p>
      {#if ["apply", "overwrite"].includes(draft.mode)}<p>
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
      {:else if draft.mode === "configure"}
        <p>
          Visual definitions are authoritative. Saving changes rules only;
          preview and apply allocations separately. Original notes remain
          retained.
        </p>
        {#if invalidVisual}<p class="error" role="alert">
            {invalidVisual}
          </p>{/if}
        <TemplateEditor
          rows={(draft.body.templates ?? []) as Record<string, unknown>[]}
          month={draft.month}
          income={(data?.categories ?? []).filter((c) => c.is_income)}
          {schedules}
          dateFormat={draft.dateFormat ?? "yyyy-MM-dd"}
          format={draft.format}
          disabled={busy || !!draft.pending}
          onchange={(rows) => commandField("templates", rows)}
        />
        <button
          onclick={loadScheduleChoices}
          disabled={busy ||
            disconnected ||
            !!draft.pending ||
            nextScheduleOffset === null}
          >{schedules.length
            ? "Load more schedule choices"
            : "Load schedule choices"}</button
        >
        <CleanupEditor
          rows={(draft.body.cleanup ?? []) as Record<string, unknown>[]}
          pools={data?.pools ?? []}
          disabled={busy || !!draft.pending}
          onchange={(rows) => commandField("cleanup", rows)}
        />
        <button
          onclick={project}
          disabled={busy || disconnected || !!draft.pending}
          >Preview category demand</button
        >
        {#if draft.projection}<p>
            Unclamped category demand: {money(
              draft.projection.budgeted,
              draft.format,
            )}. {draft.projection.goal === null
              ? "No target"
              : `Target: ${money(draft.projection.goal, draft.format)}`}.
            Full-plan application considers priorities and available funds.
          </p>
          {#if draft.projection.errors.length}<p class="error">
              {draft.projection.errors.map(issueText).join("; ")}
            </p>{/if}{/if}
      {:else if draft.mode === "pool"}<label
          >Pool name<input
            maxlength="120"
            value={String(draft.body.name ?? "")}
            disabled={busy || !!draft.pending}
            oninput={(e) => commandField("name", e.currentTarget.value)}
          /></label
        >
        <p>
          Pool names are matched without case distinctions. Reusing a name
          preserves its identity; creating a pool does not allocate money.
        </p>
      {:else if draft.mode === "migrate"}
        <p>
          This changes the authoritative representation only. Allocations and
          recorded expenses stay unchanged. Incompatible rules or indicators
          must be edited separately.
        </p>
        <details>
          <summary>Original retained notes</summary>
          <p class="saved-notes">{draft.originalNotes || "No notes"}</p>
        </details>
        {#if draft.body.target_source === "notes"}<label
            >Proposed notes<textarea
              rows="10"
              value={String(draft.body.notes ?? draft.migration?.notes ?? "")}
              disabled={busy || !!draft.pending}
              oninput={(e) => commandField("notes", e.currentTarget.value)}
            ></textarea></label
          >
          <p>
            Ordinary prose is retained. Inactive directive lines are replaced so
            stale rules are not reactivated.
          </p>{/if}
        <button
          onclick={migrationPreview}
          disabled={busy || disconnected || !!draft.pending}
          >Review source migration</button
        >
        {#if draft.migration}<p>
            Source: {draft.migration.current_source} → {draft.migration
              .target_source}. {draft.migration.compatible
              ? "Rules and goal indicators are preserved."
              : "Migration needs repair."}
          </p>
          <ul>
            {#each draft.migration.issues as issue, index (index)}<li
                class="error"
              >
                {issue}
              </li>{/each}
          </ul>
          <p>
            Full-plan check for {draft.month}: {money(
              draft.migration.before.budgeted,
              draft.format,
            )} → {money(draft.migration.after.budgeted, draft.format)}. Goal: {draft
              .migration.before.goal === null
              ? "None"
              : money(draft.migration.before.goal, draft.format)} → {draft
              .migration.after.goal === null
              ? "None"
              : money(draft.migration.after.goal, draft.format)}.
          </p>
          <details>
            <summary>Proposed typed definitions</summary><TemplateEditor
              rows={draft.migration.templates}
              month={draft.month}
              income={(data?.categories ?? []).filter((c) => c.is_income)}
              {schedules}
              dateFormat={draft.dateFormat ?? "yyyy-MM-dd"}
              format={draft.format}
              disabled={true}
              onchange={() => {}}
            /><CleanupEditor
              rows={draft.migration.cleanup}
              pools={draft.migration.pools ?? data?.pools ?? []}
              disabled={true}
              onchange={() => {}}
            />
          </details>
          <ul>
            {#each draft.migration.templates as rule, index (index)}<li>
                {String(rule.type)}{rule.description
                  ? ` · ${rule.description}`
                  : ""}
              </li>{:else}<li>No allocation rules.</li>{/each}
          </ul>{/if}
      {:else if draft.preview}<section
          class="template-preview"
          aria-labelledby="template-preview-title"
        >
          <h4 id="template-preview-title">
            {draft.mode === "cleanup"
              ? "Proposed month-end cleanup"
              : "Proposed allocations and targets"}
          </h4>
          {#if draft.preview.notification}<CleanupNotice
              notification={draft.preview.notification}
            />{/if}
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
                      >{money(row.before_budgeted)}</td
                    ><td>{money(row.budgeted)}</td><td
                      >{row.goal === null
                        ? "No target"
                        : money(row.goal)}{row.long_goal === 1
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
              (["apply", "overwrite", "cleanup"].includes(draft.mode) &&
                (!draft.preview || !!draft.preview.errors.length)) ||
              (draft.mode === "migrate" && !draft.migration?.compatible) ||
              !!invalidVisual}
            >{draft.mode === "notes"
              ? "Save template notes"
              : draft.mode === "configure"
                ? "Save visual automation"
                : draft.mode === "migrate"
                  ? "Save source migration"
                  : draft.mode === "pool"
                    ? "Save cleanup pool"
                    : draft.mode === "cleanup"
                      ? "Apply captured cleanup"
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
        disabled={busy ||
          !!draft ||
          disconnected ||
          !(enabled || visualEnabled)}>Preview unallocated templates</button
      ><button
        onclick={() => preview("overwrite")}
        disabled={busy ||
          !!draft ||
          disconnected ||
          !(enabled || visualEnabled)}>Preview overwrite all templates</button
      ><button
        onclick={() => preview("overwrite", selected)}
        disabled={busy ||
          !!draft ||
          disconnected ||
          !(enabled || visualEnabled) ||
          !selected.length}>Preview selected templates</button
      >
    </div>
    {#if visualEnabled}<div class="toolbar">
        <button
          onclick={() => openCommand("pool")}
          disabled={busy || !!draft || disconnected}
          >Create named cleanup pool</button
        ><button
          onclick={() => preview("cleanup")}
          disabled={busy || !!draft || disconnected}
          >Preview month-end cleanup</button
        >
      </div>{:else}<p>
        Enable Visual budget automations in Experimental features to edit visual
        rules and cleanup pools.
      </p>{/if}
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
            {#if visualEnabled}<button
                onclick={() =>
                  openCommand(
                    row.source === "ui" ? "configure" : "migrate",
                    row,
                  )}
                disabled={busy || !!draft || disconnected}
                aria-label={`${row.source === "ui" ? "Edit" : "Migrate"} ${row.name} visual automation`}
                >{row.source === "ui"
                  ? "Edit visual automation"
                  : "Move to visual definitions"}</button
              >{#if row.source === "ui"}<button
                  onclick={() => openCommand("migrate", row)}
                  disabled={busy || !!draft || disconnected}
                  aria-label={`Move ${row.name} automation to notes`}
                  >Move definitions to notes</button
                >{/if}{/if}
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
              >{:else if row.source === "ui"}<p>
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
