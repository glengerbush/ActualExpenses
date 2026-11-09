<script lang="ts">
  import { onMount } from "svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import { t } from "./i18n.svelte";
  import { offlineStatus, subscribeOffline } from "./offline/events";
  import {
    readRestoreUpload,
    saveRestoreUpload,
    clearRestoreUpload,
    completeRestoreUpload,
    uploadRestore,
    readLifecycle,
    saveLifecycle,
    completeLifecycle,
    lifecycleOperation,
    sendLifecycle,
    cancelLifecycle,
    type RestoreUpload,
    type ManagedBudget,
    type LifecycleOperation,
  } from "./lifecycle";
  let {
    onrestored,
    onclose,
  }: {
    onrestored: (budget: ManagedBudget) => void | Promise<void>;
    onclose: () => void;
  } = $props();
  let session = $state.raw<RestoreUpload | null>(null),
    pending = $state.raw<LifecycleOperation | null>(null);
  let target = $state.raw<{
    id: string;
    name: string;
    epoch?: string;
    deleted?: boolean;
  } | null>(null);
  let files = $state<FileList>(),
    confirmation = $state("");
  let busy = $state(true),
    initialized = $state(false),
    error = $state(""),
    notice = $state("");
  let offline = $state(!offlineStatus.reachable);
  let otherPending = $state(false);
  let mounted = true;
  async function loadTarget() {
    const stage = session?.stage;
    if (!stage) return;
    pending = await readLifecycle(stage.budget_id);
    otherPending =
      !!pending &&
      (pending.kind !== "restored" ||
        pending.path !==
          `/budgets/${stage.budget_id}/restores/${stage.id}/activate`);
    try {
      target = (
        await request<{ data: ManagedBudget }>(
          `/budgets/${stage.budget_id}?include_deleted=true`,
        )
      ).data;
    } catch (e) {
      if (e instanceof RequestError && e.code === "not_found")
        target = { id: stage.budget_id, name: stage.name };
      else throw e;
    }
  }
  async function initialize() {
    busy = true;
    error = "";
    try {
      session = await readRestoreUpload();
      await loadTarget();
      initialized = true;
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
  async function upload() {
    if (busy || offline) return;
    busy = true;
    error = "";
    try {
      let value = session;
      if (!value) {
        const file = files?.[0];
        if (!file) throw new Error("Choose an actualBudget backup ZIP.");
        if (file.size > 64 * 1024 * 1024)
          throw new Error("Choose a backup no larger than 64 MiB.");
        value = {
          version: 1,
          origin: location.origin,
          key: uuid(),
          file,
          filename: file.name,
          stage: null,
        };
      }
      await saveRestoreUpload(value);
      session = value;
      const stage = await uploadRestore(value);
      value = { ...value, stage };
      await saveRestoreUpload(value);
      session = value;
      await loadTarget();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function another() {
    if (!session || busy || pending) return;
    try {
      await clearRestoreUpload(session);
      session = null;
      target = null;
      files = undefined;
      confirmation = "";
      error = "";
      notice = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function restore(event?: SubmitEvent) {
    event?.preventDefault();
    if (busy || offline || otherPending || !session?.stage) return;
    busy = true;
    error = "";
    try {
      if (!pending) {
        if (!target || confirmation !== target.name)
          throw new Error("Enter the budget name to confirm restoration.");
        const value = lifecycleOperation(
          target,
          "restored",
          `/budgets/${target.id}/restores/${session.stage.id}/activate`,
          "POST",
          { confirm_name: confirmation },
        );
        await saveLifecycle(value);
        pending = value;
      }
      const operation = pending;
      await sendLifecycle(operation);
      if (!mounted) return;
      // Read current state: replayed receipts can precede a later restore.
      const current = (
        await request<{ data: ManagedBudget }>(`/budgets/${operation.budgetId}`)
      ).data;
      await completeRestoreUpload(session, operation);
      pending = null;
      session = null;
      await onrestored(current);
    } catch (e) {
      if (
        e instanceof RequestError &&
        e.code === "operation_canceled" &&
        pending
      ) {
        try {
          await completeLifecycle(pending);
          pending = null;
          notice = t("Restore canceled. The budget was not replaced.");
          await loadTarget().catch(() => {});
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
      await restore();
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<section
  class="restore-budget"
  aria-labelledby="restore-title"
  aria-busy={busy}
>
  <div class="page-heading">
    <div>
      <h1 id="restore-title">{t("Restore budget backup")}</h1>
      <p class="muted">
        {t(
          "Open an actualBudget ZIP backup and review it before replacing any data.",
        )}
      </p>
    </div>
    <button onclick={onclose} disabled={busy}>{t("Back to budgets")}</button>
  </div>
  {#if offline}<p role="status">
      {t(
        "Connect to review or restore a backup. The selected archive is kept on this device.",
      )}
    </p>{/if}
  {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if !initialized}<p role="status">
      {t(
        busy ? "Loading saved restore…" : "Saved restore could not be loaded.",
      )}
    </p>
    {#if !busy}<button onclick={initialize}>{t("Retry recovery")}</button>{/if}
  {:else if !session?.stage}
    <label for="native-backup">{t("actualBudget backup ZIP")}</label><input
      id="native-backup"
      type="file"
      accept=".zip,application/zip"
      bind:files
      disabled={busy || !!session}
    />
    {#if session}<p>{session.filename}</p>
      <p class="hint">
        {t(
          "The original archive is saved for retry. Reviewing it does not change any budget.",
        )}
      </p>{/if}
    <div class="actions">
      <button
        class="primary"
        disabled={busy || offline || (!session && !files?.length)}
        onclick={upload}
        >{t(session ? "Retry saved upload" : "Review backup")}</button
      >{#if session}<button disabled={busy} onclick={another}
          >{t("Choose another backup")}</button
        >{/if}
    </div>
  {:else}
    <h2>{session.stage.name}</h2>
    <p><strong>{t("Selected backup")}</strong>: {session.filename}</p>
    <p>{t("Backup validated. The original archive is retained.")}</p>
    <dl>
      <div>
        <dt>{t("Accounts")}</dt>
        <dd>{session.stage.report.domains.accounts ?? 0}</dd>
      </div>
      <div>
        <dt>{t("Transactions")}</dt>
        <dd>{session.stage.report.domains.transactions ?? 0}</dd>
      </div>
    </dl>
    <a href={`/api/v1/restores/${session.stage.id}/original.zip`} download
      >{t("Download original backup")}</a
    >
    {#if otherPending}<p class="error" role="alert">
        {t(
          "Another operation for this budget is unconfirmed. Open Manage budget to resolve it before restoring this archive.",
        )}
      </p>
    {:else if pending}
      <h2>{t("Resolve the saved restore")}</h2>
      <p>
        {t(
          "Its outcome is unconfirmed. Retry the original request or cancel it safely.",
        )}
      </p>
      <div class="actions">
        <button
          class="primary"
          disabled={busy || offline}
          onclick={() => restore()}
          >{t(
            pending.cancelRequested
              ? "Retry cancellation"
              : "Retry saved restore",
          )}</button
        ><button
          disabled={busy || offline || pending.cancelRequested}
          onclick={cancelPending}>{t("Cancel saved restore")}</button
        >
      </div>
    {:else if target}
      <form onsubmit={restore}>
        <h2>{t("Confirm restore")}</h2>
        <p>
          {target.epoch && !target.deleted
            ? t(
                "This replaces the existing budget. A recovery backup is saved first. Review any pending expenses afterward.",
              )
            : t("This restores the budget with its original identity.")}
        </p>
        <p>
          {t("Type the budget name to confirm")}: <strong>{target.name}</strong>
        </p>
        <label for="restore-confirm">{t("Confirm budget name")}</label><input
          id="restore-confirm"
          bind:value={confirmation}
          required
          autocomplete="off"
          disabled={busy || offline}
        />
        <div class="actions">
          <button
            class="primary"
            disabled={busy || offline || confirmation !== target.name}
            >{t("Restore reviewed backup")}</button
          ><button type="button" disabled={busy} onclick={another}
            >{t("Choose another backup")}</button
          >
        </div>
      </form>
    {:else}<button onclick={initialize} disabled={busy || offline}
        >{t("Retry budget lookup")}</button
      >{/if}
  {/if}
</section>

<style>
  .restore-budget {
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
  h2 {
    margin-top: 2rem;
    font-size: 1.25rem;
  }
  p {
    max-width: 70ch;
  }
  input {
    width: min(100%, 30rem);
    max-width: 100%;
  }
  .actions {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-top: 1rem;
  }
  dl {
    display: flex;
    gap: 2rem;
    flex-wrap: wrap;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  a {
    display: inline-flex;
    align-items: center;
    min-height: 44px;
  }
  form {
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--line);
  }
</style>
