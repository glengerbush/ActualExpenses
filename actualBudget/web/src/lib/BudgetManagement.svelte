<script lang="ts">
  import { onMount, tick } from "svelte";
  import { request, errorMessage, RequestError } from "./api";
  import { t } from "./i18n.svelte";
  import { offlineStatus, subscribeOffline } from "./offline/events";
  import {
    readLifecycle,
    saveLifecycle,
    completeLifecycle,
    lifecycleOperation,
    sendLifecycle,
    cancelLifecycle,
    type ManagedBudget,
    type BackupPoint,
    type LifecycleKind,
    type LifecycleOperation,
  } from "./lifecycle";
  let {
    budget,
    onchanged,
    onrestore,
  }: {
    budget: ManagedBudget;
    onchanged: (
      budget: ManagedBudget,
      reason: LifecycleKind,
    ) => void | Promise<void>;
    onrestore: () => void;
  } = $props();
  let current = $state.raw<ManagedBudget | null>(null);
  let points = $state.raw<BackupPoint[]>([]);
  let pending = $state.raw<LifecycleOperation | null>(null);
  let busy = $state(true),
    error = $state(""),
    notice = $state("");
  let label = $state(""),
    confirmation = $state(false);
  let confirm = $state.raw<{
    kind: "restored";
    point?: BackupPoint;
    budget: ManagedBudget;
  } | null>(null);
  let offline = $state(!offlineStatus.reachable);
  let mounted = true;
  let confirmationHeading: HTMLElement | undefined;
  let confirmationTrigger: HTMLElement | undefined;
  const retired = $derived(
    !!pending && !["backup", "restored"].includes(pending.kind),
  );
  const locked = $derived(busy || offline || !!pending || !current);
  async function refresh() {
    const id = budget.id;
    const [metadata, backups] = await Promise.all([
      request<{ data: ManagedBudget }>(`/budgets/${id}`),
      request<{ data: BackupPoint[] }>(`/budgets/${id}/backups`),
    ]);
    if (!mounted || id !== budget.id) return;
    current = metadata.data;
    points = backups.data;
  }
  async function initialize() {
    busy = true;
    error = "";
    try {
      pending = await readLifecycle(budget.id);
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void initialize();
    const stop = subscribeOffline(() => {
      offline = !offlineStatus.reachable;
    });
    return () => {
      mounted = false;
      stop();
    };
  });
  async function finish(operation: LifecycleOperation) {
    await sendLifecycle(operation);
    if (!mounted) return;
    notice = operation.cancelRequested
      ? t(
          "This operation had already been saved. Its confirmed result was recovered.",
        )
      : operation.kind === "backup"
        ? t("Backup saved. Download a copy to keep it outside this server.")
        : t(
            "Budget restored. Saved device changes will be checked against the restored budget.",
          );
    await refresh();
    await completeLifecycle(operation);
    pending = null;
    confirm = null;
    confirmation = false;
    if (operation.kind !== "backup") {
      await onchanged(current!, operation.kind);
    }
  }
  async function run(operation?: LifecycleOperation) {
    if (busy || offline) return;
    busy = true;
    error = "";
    notice = "";
    try {
      if (operation) {
        await saveLifecycle(operation);
        pending = operation;
      }
      if (pending) await finish(pending);
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        pending
      ) {
        try {
          await completeLifecycle(pending);
          pending = null;
          confirm = null;
          notice = t("Operation canceled. No delayed retry can apply it.");
          await refresh().catch(() => {});
        } catch (cleanupError) {
          error = errorMessage(cleanupError);
        }
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function cancelPending() {
    if (!pending || busy) return;
    try {
      pending = await cancelLifecycle(pending);
      await run();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (locked || !current) return;
    void run(
      lifecycleOperation(
        current,
        "backup",
        `/budgets/${current.id}/backups`,
        "POST",
        { label: label.trim() },
      ),
    );
  }
  async function choose(
    kind: "restored",
    point: BackupPoint | undefined,
    trigger: HTMLElement,
  ) {
    if (locked || !current) return;
    confirm = { kind, point, budget: structuredClone(current) };
    confirmation = false;
    error = "";
    confirmationTrigger = trigger;
    await tick();
    confirmationHeading?.focus();
  }
  async function cancelConfirmation() {
    confirm = null;
    await tick();
    confirmationTrigger?.focus();
  }
  function confirmed(event: SubmitEvent) {
    event.preventDefault();
    if (!confirm || !confirmation || locked) return;
    const { kind, point, budget: target } = confirm;
    void run(
      lifecycleOperation(
        target,
        kind,
        `/budgets/${target.id}${point ? `/backups/${point.id}/restore` : ""}`,
        "POST",
        { confirm_name: target.name },
      ),
    );
  }
  function kindLabel(kind: string) {
    return t(
      (
        {
          manual: "Manual backup",
          automatic: "Automatic backup",
          before_restore: "Before restore",
          before_delete: "Before deletion",
          upgrade: "Before server upgrade",
        } as Record<string, string>
      )[kind] ?? "Backup",
    );
  }
</script>

<section
  class="management"
  aria-labelledby="manage-budget-title"
  aria-busy={busy}
>
  <header class="page-heading">
    <div>
      <h1 id="manage-budget-title">{t("Backups")}</h1>
      <p class="muted">Save a copy or restore your data.</p>
      <button onclick={onrestore} disabled={busy || offline || !!pending}
        >Restore backup file</button
      >
    </div>
  </header>
  {#if offline}<p role="status">
      {t(
        "Connect to manage this budget. Saved requests are kept on this device.",
      )}
    </p>{/if}
  {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if pending}
    <section class="pending" aria-label={t("Unconfirmed budget operation")}>
      <h2>{t("Resolve the saved operation")}</h2>
      <p>
        {t(
          "The outcome is unconfirmed. Retry its original details, or cancel it safely before making another change.",
        )}
      </p>
      <p>{t("Operation")}: {pending.kind}</p>
      {#if retired}
        <p>
          This operation is no longer supported. Download its saved details
          before removing the request.
        </p>
        <div class="actions">
          <a
            href={`data:application/json;charset=utf-8,${encodeURIComponent(JSON.stringify(pending, null, 2))}`}
            download="saved-budget-operation.json">Download saved request</a
          >
          <button
            disabled={busy}
            onclick={async () => {
              if (!pending) return;
              try {
                await completeLifecycle(pending);
                pending = null;
              } catch (e) {
                error = errorMessage(e);
              }
            }}>Remove retired request</button
          >
        </div>
      {:else}
        <div class="actions">
          <button
            class="primary"
            disabled={busy || offline}
            onclick={() => run()}
            >{t(
              pending.cancelRequested
                ? "Retry cancellation"
                : "Retry saved operation",
            )}</button
          ><button
            disabled={busy || offline || pending.cancelRequested}
            onclick={cancelPending}>{t("Cancel saved operation")}</button
          >
        </div>
      {/if}
    </section>
  {/if}
  {#if !current}
    <p role="status">
      {t(
        busy
          ? "Loading budget and backups…"
          : "Budget details could not be loaded.",
      )}
    </p>
    {#if !busy}<button onclick={initialize}>{t("Retry loading budget")}</button
      >{/if}
  {:else}
    {#if confirm}
      <form class="confirmation" onsubmit={confirmed}>
        <h2
          tabindex="-1"
          {@attach (element) => {
            confirmationHeading = element;
            return () => {
              confirmationHeading = undefined;
            };
          }}
        >
          {t("Restore this backup?")}
        </h2>
        <p>
          This replaces current data. A recovery backup is saved first; saved
          device changes must be reviewed afterward.
        </p>
        {#if confirm.point}<p>
            {confirm.point.label || kindLabel(confirm.point.kind)} · {new Date(
              confirm.point.created_at,
            ).toLocaleString()}
          </p>{/if}
        <label class="restore-consent"
          ><input
            type="checkbox"
            bind:checked={confirmation}
            disabled={locked}
          /> Replace current data with this backup</label
        >
        <div class="actions">
          <button class="primary" disabled={locked || !confirmation}
            >{t("Confirm restore")}</button
          ><button
            type="button"
            disabled={busy || !!pending}
            onclick={cancelConfirmation}>{t("Cancel")}</button
          >
        </div>
      </form>
    {/if}
    <section aria-labelledby="backups-heading">
      <h2 id="backups-heading">{t("Saved backups")}</h2>
      <p class="muted">
        {t("Download a backup to keep a copy outside this server.")}
      </p>
      {#if current.backup_error}<p class="error" role="alert">
          {t("Automatic backup failed. Existing backups are retained.")}
          {current.backup_error}
        </p>{/if}
      {#if current.last_auto}<p class="hint">
          {t("Last automatic backup")}: {new Date(
            current.last_auto * 1000,
          ).toLocaleString()}
        </p>{/if}
      <form onsubmit={submit}>
        <fieldset disabled={locked || !!confirm}>
          <label for="backup-label">{t("Backup label")}</label><input
            id="backup-label"
            bind:value={label}
            maxlength="120"
            placeholder={t("Optional label")}
          /><button>{t("Create backup")}</button>
        </fieldset>
      </form>
      {#if !points.length}<p>{t("No backups yet.")}</p>{:else}<ul
          class="backup-list"
        >
          {#each points as point (point.id)}<li>
              <div>
                <strong>{point.label || kindLabel(point.kind)}</strong>
                <p class="hint">
                  {kindLabel(point.kind)} ·
                  <time datetime={point.created_at}
                    >{new Date(point.created_at).toLocaleString()}</time
                  >
                  · {Math.ceil(point.bytes / 1024).toLocaleString()} KiB
                </p>
              </div>
              <div class="actions">
                <a
                  href={`/api/v1/budgets/${budget.id}/backups/${point.id}/download`}
                  download>{t("Download")}</a
                ><button
                  disabled={locked || !!confirm}
                  onclick={(event) =>
                    choose("restored", point, event.currentTarget)}
                  >{t("Restore")}</button
                >
              </div>
            </li>{/each}
        </ul>{/if}
      <button disabled={busy || offline || !!pending} onclick={initialize}
        >{t("Refresh backups")}</button
      >
    </section>
  {/if}
</section>

<style>
  .management {
    max-width: 60rem;
    overflow-wrap: anywhere;
  }
  .page-heading {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .management > section,
  .management > form {
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--line);
  }
  h2 {
    font-size: 1.25rem;
    margin: 0 0 0.75rem;
  }
  p {
    max-width: 70ch;
  }
  fieldset {
    display: flex;
    align-items: start;
    flex-direction: column;
    gap: 0.75rem;
  }
  input {
    width: min(100%, 30rem);
  }
  .actions {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .backup-list {
    list-style: none;
    padding: 0;
    margin: 1.25rem 0;
  }
  .backup-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 1rem 0;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .backup-list li > div {
    min-width: 0;
  }
  .backup-list p {
    margin: 0.25rem 0;
  }
  .backup-list a {
    display: inline-flex;
    align-items: center;
    min-height: 44px;
  }
  .restore-consent {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }
  .restore-consent input {
    width: auto;
  }
  .confirmation {
    padding: 1.25rem;
    background: var(--hover);
  }
  .confirmation .actions {
    margin-top: 1rem;
  }
</style>
