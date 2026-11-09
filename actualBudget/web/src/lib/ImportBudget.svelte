<script lang="ts">
  import { onMount } from "svelte";
  import {
    errorMessage,
    list,
    request,
    uuid,
    RequestError,
    type Budget,
  } from "./api";
  import { uploadArchive, type ImportStage } from "./imports-api";
  import {
    readImportSession,
    saveImportSession,
    clearImportSession,
    type ImportSession,
  } from "./import-storage";
  import { formatAmountInput } from "./money";
  import { getCurrency } from "./preferences/currencies";
  import { t } from "./i18n.svelte";
  let {
    oncreated,
    oncancel,
  }: {
    oncreated: (budget: Budget) => void | Promise<void>;
    oncancel: () => void;
  } = $props();
  let session = $state.raw<ImportSession | null>(null);
  let imports = $state.raw<ImportStage[]>([]);
  let files = $state<FileList>();
  let busy = $state(true);
  let error = $state("");
  let initialized = $state(false);
  const stage = $derived(session?.stage ?? null);
  const blockers = $derived(
    stage?.report.requirements.filter((row) => row.blocking) ?? [],
  );
  const retained = $derived(
    stage?.report.requirements.filter((row) => !row.blocking) ?? [],
  );
  async function initialize() {
    busy = true;
    error = "";
    try {
      session = await readImportSession();
      imports = await list<ImportStage>("/imports");
      if (session?.stage && !session.pendingActivation) {
        const response = await request<{ data: ImportStage }>(
          `/imports/${session.stage.id}`,
        );
        session = { ...session, stage: response.data };
        await saveImportSession(session);
      }
      initialized = true;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void initialize();
  });
  async function upload() {
    busy = true;
    error = "";
    try {
      let saved = session;
      if (!saved) {
        const file = files?.[0];
        if (!file) throw new Error("Choose an Actual ZIP export.");
        if (file.size > 16 * 1024 * 1024)
          throw new Error("Choose a ZIP export no larger than 16 MiB.");
        saved = {
          version: 1,
          key: uuid(),
          activationKey: uuid(),
          file,
          filename: file.name,
          stage: null,
          pendingUpload: true,
          pendingActivation: false,
        };
      }
      if (!saved.file)
        throw new Error(
          "Saved archive is unavailable. Keep the original file and choose it again.",
        );
      await saveImportSession(saved);
      session = saved;
      const result = await uploadArchive(saved.file, saved.key);
      const confirmed = { ...saved, stage: result, pendingUpload: false };
      await saveImportSession(confirmed);
      session = confirmed;
    } catch (e) {
      if (e instanceof RequestError && !e.uncertain && session) {
        try {
          await clearImportSession(session.key, true);
          session = null;
        } catch {
          /* Retain original saved operation if cleanup fails. */
        }
      }
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function selectStage(value: ImportStage) {
    busy = true;
    error = "";
    try {
      const saved: ImportSession = {
        version: 1,
        key: uuid(),
        activationKey: uuid(),
        file: null,
        filename: value.report.source.budgetName,
        stage: value,
        pendingUpload: false,
        pendingActivation: false,
      };
      await saveImportSession(saved);
      session = saved;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function activate() {
    if (!session?.stage) return;
    busy = true;
    error = "";
    try {
      const pending = { ...session, pendingActivation: true };
      await saveImportSession(pending);
      session = pending;
      const response = await request<{ data: Budget }>(
        `/imports/${pending.stage!.id}/activate`,
        {},
        pending.activationKey,
      );
      await clearImportSession(pending.key, true);
      session = null;
      await oncreated(response.data);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function another() {
    if (!session || session.pendingActivation || session.pendingUpload) return;
    busy = true;
    error = "";
    try {
      await clearImportSession(session.key);
      session = null;
      files = undefined;
      imports = await list<ImportStage>("/imports");
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  function featureLabel(feature: string): string {
    const labels: Record<string, string> = {
      rules: "Rules",
      account_groups: "Account groups",
      category_groups: "Category groups",
      category_mapping: "Category aliases",
      payee_mapping: "Payee aliases",
      transfer_payees: "Transfer payees",
      dashboard: "Dashboard layout",
      dashboard_pages: "Dashboard pages",
      messages_clock: "Source clock",
      messages_crdt: "Original edit history",
      account_configuration: "Account settings",
      category_configuration: "Category settings",
      transaction_features: "Transaction features",
      future_source_clock: "Source clock needs correction",
    };
    return t(labels[feature] ?? feature.replaceAll("_", " "));
  }
  function amount(value: number) {
    const settings = stage!.report.preferences.shared;
    return formatAmountInput(value, {
      number_format: settings.number_format,
      decimal_places: getCurrency(
        settings.experimental.currency ? settings.currency_code : "",
      ).decimalPlaces as 0 | 2,
    });
  }
</script>

<section
  aria-labelledby="import-heading"
  aria-busy={busy}
  class="import-budget"
>
  <h1 id="import-heading">{t("Import Actual budget")}</h1>
  <p class="muted">
    {t(
      "Review an unencrypted Actual 26.10.0 ZIP export before opening it. Your original file is kept for recovery.",
    )}
  </p>
  {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
  {#if busy}<p role="status">{t("Checking import…")}</p>{/if}
  {#if !initialized && !busy}<button onclick={initialize}
      >{t("Retry recovery")}</button
    >{/if}
  {#if session?.pendingUpload}
    <h2>{t("Recover saved upload")}</h2>
    <p>{session.filename}</p>
    <p>
      {t(
        "The original archive and request are saved on this device. Retry safely, even if the server already received it.",
      )}
    </p>
    <button class="primary" onclick={upload} disabled={busy}
      >{t("Retry saved upload")}</button
    >
  {:else if stage}
    <h2>{stage.report.source.budgetName}</h2>
    <p>
      {stage.status === "blocked"
        ? t("This budget needs additional conversion before it can be opened.")
        : stage.status === "activated"
          ? t("This budget has already been imported.")
          : t(
              "The archive passed integrity and relationship checks. Review the balances before activation.",
            )}
    </p>
    <div class="import-balances">
      <h3>{t("Source balances, including future entries")}</h3>
      <dl>
        {#each stage.report.balances as balance (balance.account_id)}
          <div class="balance-row">
            <dt>
              {stage.report.accounts.find(
                (account) => account.id === balance.account_id,
              )?.name ?? balance.account_id}
            </dt>
            <dd>{amount(balance.total)}</dd>
          </div>
        {/each}
      </dl>
    </div>
    {#if blockers.length}<div role="alert">
        <h3>{t("Conversion required")}</h3>
        <ul>
          {#each blockers as item, index (`${item.feature}-${index}`)}<li>
              <strong>{featureLabel(item.feature)}</strong> ({item.count}): {item.reason}
            </li>{/each}
        </ul>
      </div>{/if}
    <details>
      <summary>{t("Retained source data")}</summary>
      <p>
        {t(
          "These records remain in the original archive for later feature converters. They have not been silently removed.",
        )}
      </p>
      <ul>
        {#each retained as item, index (`${item.feature}-${index}`)}<li>
            {featureLabel(item.feature)} ({item.count}): {item.reason}
          </li>{/each}
      </ul>
    </details>
    <p>
      <a href={`/api/v1/imports/${stage.id}/original.zip`} download
        >{t("Download original archive")}</a
      >
    </p>
    <div class="form-actions">
      <button
        class="primary"
        onclick={activate}
        disabled={busy || stage.status === "blocked"}
        >{session?.pendingActivation
          ? t("Retry saved activation")
          : stage.status === "activated"
            ? t("Open imported budget")
            : t("Activate imported budget")}</button
      >
      <button onclick={another} disabled={busy || session?.pendingActivation}
        >{t("Review another file")}</button
      >
    </div>
  {:else if initialized}
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void upload();
      }}
    >
      <label for="actual-archive">{t("Actual ZIP export")}</label>
      <input
        id="actual-archive"
        type="file"
        accept=".zip,application/zip"
        bind:files
        required
        disabled={busy}
        aria-describedby="archive-limit"
      />
      <p id="archive-limit" class="hint">
        {t(
          "Maximum 16 MiB compressed and 64 MiB expanded. This release supports ordinary account ledgers; other financial features require conversion.",
        )}
      </p>
      <button class="primary" disabled={busy || !files?.length}
        >{t("Review archive")}</button
      >
    </form>
    {#if imports.length}<h2>{t("Previous imports")}</h2>
      <ul class="import-list">
        {#each imports as item (item.id)}<li>
            <button onclick={() => selectStage(item)} disabled={busy}
              >{item.report.source.budgetName}<span class="muted"
                >{item.status === "blocked"
                  ? t("Conversion required")
                  : item.status === "activated"
                    ? t("Imported")
                    : t("Ready for review")}</span
              ></button
            >
          </li>{/each}
      </ul>{/if}
  {/if}
  <button class="quiet close-import" onclick={oncancel} disabled={busy}
    >{t("Back to budgets")}</button
  >
</section>

<style>
  .import-budget {
    max-width: 48rem;
  }
  .import-budget p {
    max-width: 65ch;
  }
  .import-budget h2 {
    margin-top: 2rem;
  }
  .import-budget h3 {
    margin-top: 1.5rem;
  }
  .import-budget form {
    display: grid;
    gap: 0.75rem;
    margin-block: 1.5rem;
  }
  .import-budget input {
    max-width: 100%;
    min-width: 0;
  }
  .import-budget li {
    margin-block: 0.5rem;
    overflow-wrap: anywhere;
  }
  .import-budget details {
    margin-block: 1.5rem;
  }
  .import-budget summary {
    cursor: pointer;
    padding-block: 0.75rem;
  }
  .import-balances {
    margin-block: 1.5rem;
  }
  .import-balances dl {
    margin: 0;
  }
  .balance-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    padding-block: 1rem;
    border-bottom: 1px solid var(--line);
  }
  .balance-row dt {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .balance-row dd {
    margin: 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .form-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
  }
  @media (max-width: 30rem) {
    .balance-row {
      display: block;
    }
    .balance-row dd {
      text-align: left;
      margin-top: 0.5rem;
    }
  }
  .import-list {
    padding: 0;
    list-style: none;
  }
  .import-list button {
    width: 100%;
    text-align: left;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .close-import {
    margin-top: 1.5rem;
  }
</style>
