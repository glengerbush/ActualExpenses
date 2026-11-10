<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import {
    request,
    RequestError,
    errorMessage,
    uuid,
    type Account,
  } from "./api";
  import { preferences } from "./preferences.svelte";
  import { readValue, writeValue } from "./offline/storage";
  import { historyOwner } from "./history-owner";
  import { nextEditClock } from "./edit-clock";
  import { formatAmount, formatAmountInput, parseAmount } from "./money";
  import { validDate } from "./dates";
  import DateInput from "./DateInput.svelte";
  import BankSettings from "./BankSettings.svelte";
  import {
    defaultBankPreferences,
    newBankFields,
    readBankDraft,
    writeBankDraft,
    secretDigest,
    type BankDraft,
    type BankFields,
    type BankPending,
    type BankState,
    type BankAccountView,
    type BankPreferences,
    type BankConnection,
    type BankDiagnostic,
  } from "./banking";
  let {
    budgetId,
    accounts,
    disconnected,
    onclose,
    onchanged,
  }: {
    budgetId: string;
    accounts: Account[];
    disconnected: boolean;
    onclose: () => void;
    onchanged: () => Promise<void>;
  } = $props();
  let view = $state.raw<BankState | null>(null),
    saved = $state.raw<BankDraft | null>(null),
    accountView = $state.raw<BankAccountView | null>(null);
  let fields = $state<BankFields>(newBankFields());
  let rawToken = $state(""),
    busy = $state(false),
    loading = $state(true),
    live = $state(false),
    error = $state(""),
    draftProblem = $state(""),
    notice = $state(""),
    cacheDate = $state("");
  let broken = $state(false),
    unsaved = $state(false),
    saving = $state(false);
  let tail = Promise.resolve(),
    editSequence = 0,
    settingsGeneration = 0;
  const root = $derived(`/budgets/${budgetId}`),
    pending = $derived(saved?.pending ?? null);
  const selected = $derived(
    view?.connections.find((c) => c.id === fields.connectionId) ?? null,
  );
  const local = $derived(
    accounts.find((a) => a.id === fields.accountId) ?? null,
  );
  const link = $derived(
    view?.links.find((l) => l.id === fields.accountId && !l.deleted) ?? null,
  );
  const remotes = $derived(
    view?.accounts.filter(
      (a) => a.connection_id === fields.connectionId && !a.deleted,
    ) ?? [],
  );
  const displayReady = $derived(
    !!view &&
      preferences.hasUsableShared &&
      preferences.budgetId === budgetId &&
      preferences.budgetEpoch === view.epoch,
  );
  const masked = $derived(!displayReady || preferences.shared.privacy_mode);
  const liveLocalIds = $derived(
    new Set(accounts.filter((a) => !a.deleted).map((a) => a.id)),
  );
  const remoteById = $derived(
    new Map((view?.accounts ?? []).map((a) => [a.id, a])),
  );
  const unavailableLinks = $derived(
    (displayReady ? view?.links : [])?.filter(
      (l) =>
        !l.deleted &&
        l.connection_id === fields.connectionId &&
        !liveLocalIds.has(l.id),
    ) ?? [],
  );
  const chosenRemote = $derived(remotes.find((r) => r.id === fields.remoteId));
  const mappingCurrent = $derived(
    !!link &&
      !!selected &&
      link.connection_id === selected.id &&
      link.connection_generation === selected.generation,
  );
  const staleDraft = $derived(
    !!saved?.epoch && !!view && saved.epoch !== view.epoch,
  );
  const editable = $derived(
    displayReady && !busy && !loading && !pending && !broken,
  );
  const writable = $derived(
    editable &&
      live &&
      !staleDraft &&
      !draftProblem &&
      preferences.hasUsableShared,
  );
  const labels: Record<BankConnection["state"], string> = {
    claim_pending: "Ready to claim",
    claim_inflight: "Claim interrupted or still finishing",
    connected: "Connected",
    claim_failed: "Claim needs a new token",
    credentials_missing: "Credentials missing",
  };
  const cacheKey = $derived(`view:banking:${budgetId}`);
  const privateText = (value: string) => (masked ? "Hidden" : value);
  function providerDate(seconds: number) {
    const date = new Date(seconds * 1000);
    return seconds && Number.isFinite(date.getTime())
      ? date.toISOString().slice(0, 10)
      : "Not retained";
  }
  const diagnostics = (rows: BankDiagnostic[]) => [
    ...new Map(rows.map((d) => [JSON.stringify(d), d])).values(),
  ];
  function clone<T>(value: T): T {
    return JSON.parse(JSON.stringify(value));
  }
  async function checkpoint(
    nextPending: BankPending | null = saved?.pending ?? null,
    epoch = saved?.epoch ?? view?.epoch ?? null,
  ) {
    const next: BankDraft = {
      version: 1,
      budgetId,
      revision: saved?.revision ?? uuid(),
      epoch,
      fields: clone(fields),
      pending: clone(nextPending),
    };
    saved = await writeBankDraft(budgetId, saved, next);
  }
  function edit<K extends keyof BankFields>(key: K, value: BankFields[K]) {
    if (!editable) return;
    fields[key] = value;
    unsaved = true;
    const sequence = ++editSequence,
      snapshot = clone(fields);
    saving = true;
    tail = tail.then(async () => {
      try {
        const next: BankDraft = {
          version: 1,
          budgetId,
          revision: saved?.revision ?? uuid(),
          epoch: saved?.epoch ?? view?.epoch ?? null,
          fields: snapshot,
          pending: saved?.pending ?? null,
        };
        saved = await writeBankDraft(budgetId, saved, next);
        if (sequence === editSequence) {
          unsaved = false;
          draftProblem = "";
        }
      } catch (e) {
        draftProblem = `Draft could not be saved: ${errorMessage(e)} No new bank request was sent.`;
      } finally {
        if (sequence === editSequence) saving = false;
      }
    });
  }
  async function saveAgain() {
    busy = true;
    error = "";
    await tail;
    try {
      await checkpoint();
      unsaved = false;
      draftProblem = "";
    } catch (e) {
      draftProblem = `Draft could not be saved: ${errorMessage(e)}`;
    } finally {
      busy = false;
    }
  }
  async function loadSettings() {
    const generation = ++settingsGeneration;
    accountView = null;
    if (!fields.accountId || !live || !displayReady) return;
    try {
      const response = await request<{ data: BankAccountView }>(
        `${root}/accounts/${fields.accountId}/banking`,
      );
      if (generation !== settingsGeneration || !displayReady) return;
      accountView = response.data;
      if (fields.settingsAccount !== fields.accountId) {
        fields.settings = clone(
          response.data.preferences.settings ??
            suggestSettings(response.data.preferences.raw_preferences),
        );
        fields.settingsAccount = fields.accountId;
        await checkpoint();
      }
    } catch (e) {
      if (generation === settingsGeneration) error = errorMessage(e);
    }
  }
  function suggestSettings(raw: Record<string, unknown>): BankPreferences {
    const result = defaultBankPreferences();
    for (const key of [
      "import_transactions",
      "import_pending",
      "import_notes",
      "update_dates",
    ] as const) {
      const v = raw[key];
      if (v === true || v === "true") result[key] = true;
      else if (v === false || v === "false") result[key] = false;
    }
    try {
      const map =
        typeof raw.mappings === "string"
          ? JSON.parse(raw.mappings)
          : raw.mappings;
      if (
        map &&
        ["payment", "deposit"].every((kind) =>
          ["date", "payee", "notes"].every(
            (field) => typeof map[kind]?.[field] === "string",
          ),
        )
      )
        result.mappings = map;
    } catch {
      /* Unsupported fields remain proposed defaults until explicit Save. */
    }
    return result;
  }
  async function load() {
    const response = await request<{ data: BankState }>(`${root}/banking`);
    view = response.data;
    live = true;
    cacheDate = "";
    // Metadata only. Tokens and raw provider responses never enter this cache.
    await writeValue(cacheKey, {
      version: 1,
      origin: location.origin,
      budgetId,
      data: response.data,
      savedAt: new Date().toISOString(),
    }).catch(() => {
      notice =
        "Connection details loaded; this device could not cache them for offline review.";
    });
    await preferences.loadBudget(budgetId);
    await loadSettings();
  }
  async function refreshDetails() {
    await tail;
    busy = true;
    error = "";
    try {
      await load();
      if (displayReady) await onchanged();
    } catch (e) {
      live = false;
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function initialize() {
    loading = true;
    try {
      saved = await readBankDraft(budgetId);
      if (saved) fields = clone(saved.fields);
    } catch (e) {
      broken = true;
      error = errorMessage(e);
    }
    const cached = await readValue<{
      version: number;
      origin: string;
      budgetId: string;
      data: BankState;
      savedAt: string;
    }>(cacheKey).catch(() => null);
    if (
      cached?.version === 1 &&
      cached.origin === location.origin &&
      cached.budgetId === budgetId &&
      Array.isArray(cached.data?.connections) &&
      Array.isArray(cached.data?.accounts) &&
      Array.isArray(cached.data?.links)
    ) {
      view = cached.data;
      cacheDate = cached.savedAt;
    }
    try {
      await load();
      if (displayReady) await onchanged();
    } catch (e) {
      live = false;
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }
  async function chooseConnection(id: string) {
    rawToken = "";
    edit("connectionId", id);
    edit("remoteId", "");
    await tail;
  }
  async function chooseAccount(id: string) {
    edit("accountId", id);
    await tail;
    await loadSettings();
  }
  function commandPath(id: string) {
    return `${root}/banking/connections/${id}/commands`;
  }
  async function finish(result: unknown, canceled = false) {
    const data = (result as { data?: Record<string, unknown> })?.data;
    if (data && typeof data.id === "string" && data.provider === "simplefin")
      fields.connectionId = data.id;
    await checkpoint(null);
    unsaved = false;
    draftProblem = "";
    if (canceled)
      notice = "Original bank request canceled. It cannot be submitted later.";
    else if (data && Array.isArray(data.accounts) && !data.connection) {
      const rows = data.accounts as {
        status?: string;
        added?: unknown[];
        updated?: unknown[];
        ignored?: unknown[];
      }[];
      const count = (key: "added" | "updated" | "ignored") =>
        rows.reduce(
          (n, r) => n + (Array.isArray(r[key]) ? r[key]!.length : 0),
          0,
        );
      const failed = rows.filter((r) => r.status === "failed").length;
      const messages = Array.isArray(data.errors)
        ? data.errors
            .filter(
              (e): e is { message: string } =>
                !!e && typeof e === "object" && typeof e.message === "string",
            )
            .slice(0, 8)
            .map((e) => e.message)
        : [];
      notice =
        data.status === "failed"
          ? `Provider refresh failed. No financial changes were applied. ${messages.join(" ")}`
          : `Refresh confirmed: ${count("added")} added, ${count("updated")} updated, ${count("ignored")} ignored.${failed ? ` ${failed} account(s) need review; their unsuccessful windows were not advanced.` : ""} ${messages.join(" ")}`;
    } else
      notice =
        "Original bank request confirmed. Current connection details are shown below.";
    await load();
    if (displayReady) await onchanged();
  }
  async function send(p: BankPending, body = p.body, receiptOnly = false) {
    return request(
      p.path,
      body,
      p.key,
      "POST",
      p.editClock,
      p.cancel,
      p.epoch,
      p.owner,
      receiptOnly,
    );
  }
  async function begin(
    path: string,
    body: Record<string, unknown>,
    label: string,
    token?: string,
    owned = false,
  ) {
    if (!writable) {
      error =
        "Load current connection details and save this draft before sending a new request.";
      return;
    }
    busy = true;
    error = "";
    notice = "";
    try {
      await tail;
      if (!displayReady || staleDraft || !live)
        throw Error(
          "Refresh matching display preferences and connection details before a new request.",
        );
      if (draftProblem)
        throw Error(
          "Save the current draft again before sending a bank request.",
        );
      const p: BankPending = {
        path,
        body: clone(body),
        key: uuid(),
        epoch: saved?.epoch ?? view!.epoch,
        credential: token !== undefined,
        cancel: false,
        label,
        ...(owned
          ? {
              owner: await historyOwner(),
              editClock: nextEditClock({
                server: view!.clock,
                ...accountView?.preferences.field_versions,
              }),
            }
          : {}),
      };
      await checkpoint(p, p.epoch);
      unsaved = false;
      let wire = p.body;
      if (token !== undefined) {
        wire = { ...wire, setup_token: token };
        delete wire.setup_token_sha256;
        rawToken = "";
      }
      const result = await send(p, wire);
      await finish(result);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function recover(mode: "check" | "retry" | "cancel") {
    const old = pending;
    if (!old || busy) return;
    busy = true;
    error = "";
    notice = "";
    await tail;
    try {
      const p = { ...clone(old), cancel: old.cancel || mode === "cancel" };
      // CAS before every delivery prevents a stale tab from authorizing work.
      await checkpoint(p);
      if (p.cancel) {
        await finish(await send(p));
        return;
      }
      try {
        await finish(await send(p, p.body, true));
        return;
      } catch (e) {
        if (e instanceof RequestError && e.code === "operation_canceled") {
          await finish(null, true);
          return;
        }
        if (!(e instanceof RequestError && e.code === "operation_uncommitted"))
          throw e;
      }
      if (mode === "retry" && !p.credential) await finish(await send(p));
      else
        notice = p.credential
          ? "No receipt yet. Keep this original intent or cancel it before entering a token again. Tokens are never resent from browser storage."
          : "No receipt yet. Retry the exact original request or cancel it.";
    } catch (e) {
      if (e instanceof RequestError && e.code === "operation_canceled") {
        try {
          await finish(null, true);
        } catch (saveError) {
          error = errorMessage(saveError);
        }
      } else error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function credentials() {
    const token = rawToken.trim();
    if (!token || token.length > 8192 || !/^[A-Za-z0-9+/=]+$/.test(token)) {
      error = "Paste a valid SimpleFIN setup token (up to 8192 characters).";
      return;
    }
    if (!selected && !fields.name.trim()) {
      error = "Enter a connection name.";
      return;
    }
    const hash = secretDigest(token);
    if (selected)
      await begin(
        commandPath(selected.id),
        {
          action: "replace",
          generation: selected.generation,
          setup_token_sha256: hash,
        },
        "Replace SimpleFIN setup token",
        token,
      );
    else
      await begin(
        `${root}/banking/connections`,
        {
          id: uuid(),
          provider: "simplefin",
          name: fields.name.trim(),
          setup_token_sha256: hash,
        },
        "Stage SimpleFIN connection",
        token,
      );
  }
  async function linkAccount() {
    if (!selected || !local || !accountView) {
      error = "Choose a local account and load its settings first.";
      return;
    }
    const body: Record<string, unknown> = {
      action: "link",
      snapshot: accountView.snapshot,
      connection_id: selected.id,
      generation: selected.generation,
      remote_account_id: fields.remoteId,
      opening_balance_mode: fields.openingMode,
    };
    try {
      if (fields.openingMode === "custom") {
        body.opening_amount = parseAmount(fields.openingAmount, fields.format);
        if (!validDate(fields.openingDate))
          throw Error("Choose a valid opening date.");
        body.opening_date = fields.openingDate;
      }
      await begin(
        `${root}/accounts/${local.id}/banking/commands`,
        body,
        "Link provider account",
        undefined,
        true,
      );
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function saveSettings() {
    if (!local || !accountView || fields.settingsAccount !== local.id) {
      error = "Load the selected account's preferences before saving.";
      return;
    }
    await begin(
      `${root}/accounts/${local.id}/banking/commands`,
      {
        action: "settings",
        snapshot: accountView.snapshot,
        revision: accountView.link?.revision ?? null,
        changes: clone(fields.settings),
      },
      "Save bank preferences",
      undefined,
      true,
    );
  }
  async function refreshTransactions() {
    if (
      !link ||
      !selected ||
      link.connection_id !== selected.id ||
      link.connection_generation !== selected.generation
    ) {
      error =
        "Review and save this account's mapping to the current connection first.";
      return;
    }
    if (!validDate(fields.startDate) || !validDate(fields.endDate)) {
      error = "Choose a valid inclusive start and exclusive end date.";
      return;
    }
    const days =
      (Date.parse(fields.endDate + "T00:00:00Z") -
        Date.parse(fields.startDate + "T00:00:00Z")) /
      86400000;
    if (days < 1 || days > 90) {
      error = "Choose a range of 1–90 days.";
      return;
    }
    await begin(
      commandPath(selected.id),
      {
        action: "refresh",
        generation: selected.generation,
        snapshot: accountView?.snapshot ?? view!.snapshot,
        start_date: fields.startDate,
        end_date: fields.endDate,
        accounts: [{ account_id: link.id, revision: link.revision }],
      },
      "Refresh linked bank account",
      undefined,
      true,
    );
  }
  async function discard() {
    if (pending || busy) return;
    busy = true;
    error = "";
    await tail;
    try {
      saved = await writeBankDraft(budgetId, saved, null);
      fields = newBankFields();
      rawToken = "";
      accountView = null;
      unsaved = false;
      draftProblem = "";
      notice = "Unsent bank draft discarded.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function reviewEpoch() {
    busy = true;
    await tail;
    try {
      await checkpoint(null, view!.epoch);
      draftProblem = "";
      error = "";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  async function close() {
    await tail;
    if (rawToken || unsaved || draftProblem) {
      error =
        "Clear the unsent token, and save or discard this draft before closing.";
      return;
    }
    onclose();
  }
  function leaving(event: BeforeUnloadEvent) {
    if (rawToken || unsaved || draftProblem) {
      event.preventDefault();
      event.returnValue = "";
    }
  }
  onMount(() => {
    void initialize();
  });
</script>

{#snippet credentialForm()}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void credentials();
    }}
  >
    <h3>{selected ? "Replace credentials" : "Add a SimpleFIN connection"}</h3>
    {#if !selected}<label for="bank-name">{t("Connection name")}</label><input
        id="bank-name"
        value={displayReady ? fields.name : ""}
        oninput={(e) => edit("name", e.currentTarget.value)}
        maxlength="120"
        required
        disabled={!editable}
      />{/if}
    <label for="bank-token">{t("SimpleFIN setup token")}</label><input
      id="bank-token"
      type="password"
      autocomplete="off"
      spellcheck="false"
      value={rawToken}
      oninput={(e) => (rawToken = e.currentTarget.value)}
      disabled={!editable}
      aria-describedby="bank-token-help"
    />
    <p id="bank-token-help">
      This token is not saved in browser storage. After submission, you can
      check or cancel the original change without entering the token again. {selected
        ? "Replacement preserves saved transactions and requires reviewing the connection again."
        : "Staging stores it privately on this server; claiming is a separate action."}
    </p>
    <div class="actions">
      <button
        class="primary"
        type="submit"
        disabled={!writable || !rawToken.trim()}
        >{selected ? "Replace setup token" : "Stage connection"}</button
      >{#if rawToken}<button
          type="button"
          onclick={() => (rawToken = "")}
          disabled={busy}>{t("Clear unsent token")}</button
        >{/if}
    </div>
  </form>
{/snippet}

<svelte:window onbeforeunload={leaving} />
<section class="banking" aria-labelledby="banking-title">
  <header>
    <div>
      <h1 id="banking-title">{t("Bank connections")}</h1>
      <p>
        SimpleFIN credentials stay on this server. Refresh runs only when you
        request it.
      </p>
    </div>
    <button onclick={close} disabled={busy || saving}
      >{t("Close bank connections")}</button
    >
  </header>
  {#if error || draftProblem}<div role="alert">
      <p>{error}</p>
      {#if draftProblem}<p>{draftProblem}</p>
        <button onclick={saveAgain} disabled={busy || saving || broken}
          >{t("Save draft again")}</button
        >{/if}
    </div>{/if}
  {#if view && !displayReady}<p role="status">
      Display preferences are unconfirmed for this budget version. Names and
      amounts are hidden; new changes are blocked. Refresh connection details to
      try again. Original saved requests can still be checked or canceled.
    </p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if loading}<p role="status">{t("Loading bank connections\u2026")}</p>{/if}
  {#if busy && !loading}<p role="status">
      {pending
        ? "Processing saved bank request…"
        : "Updating bank connection details…"}
    </p>{/if}
  {#if !live && !loading}<p>
      Offline review only. {cacheDate
        ? `Cached bank details saved ${new Date(cacheDate).toLocaleString()}.`
        : "No bank details cached on this device."} Saved requests keep their original
      identity.
    </p>{/if}
  {#if disconnected && live}<p>
      Server reached. Account balances outside this page may still need to
      refresh.
    </p>{/if}
  <div class="actions">
    <button onclick={refreshDetails} disabled={busy || loading}
      >{t("Refresh connection details")}</button
    >{#if !pending}<button
        onclick={discard}
        disabled={busy || saving || loading || broken}
        >{t("Discard unsent draft")}</button
      >{/if}
  </div>
  {#if pending}
    <section class="recovery" aria-labelledby="banking-pending-title">
      <h2 id="banking-pending-title">{t("Saved bank request")}</h2>
      <p>
        {pending.label}. {pending.cancel
          ? "Cancellation is saved; continue it without submitting the original action."
          : "Check whether this change was saved. Retry sends the same change; cancel prevents an unsaved change from being applied later."}
      </p>
      {#if pending.credential}<p>
          The token was not saved on this device. Check the original result or
          cancel before entering a token again.
        </p>{/if}
      <div class="actions">
        <button onclick={() => recover("check")} disabled={busy}
          >{t("Check original result")}</button
        >{#if !pending.credential && !pending.cancel}<button
            onclick={() => recover("retry")}
            disabled={busy}>{t("Retry original request")}</button
          >{/if}<button onclick={() => recover("cancel")} disabled={busy}
          >{pending.cancel
            ? "Continue cancellation"
            : "Cancel original request"}</button
        >
      </div>
    </section>
  {/if}
  {#if staleDraft && !pending}<section class="recovery">
      <h2>{t("Budget changed since this draft")}</h2>
      <p>
        Review the current connection details. This budget was restored after
        the draft was started. Explicitly review it before using this draft with
        the restored budget.
      </p>
      <button onclick={reviewEpoch} disabled={busy || !live}
        >{t("Use current budget for this draft")}</button
      >
    </section>{/if}
  {#if view}
    <section aria-labelledby="connections-title">
      <h2 id="connections-title">{t("Connections")}</h2>
      {#if !view.connections.length}<p>
          No connections yet. Get a one-use setup token from <a
            href="https://beta-bridge.simplefin.org/"
            target="_blank"
            rel="noopener noreferrer">{t("SimpleFIN Bridge")}</a
          >, then stage it below.
        </p>{/if}
      <label for="bank-connection">{t("Connection")}</label><select
        id="bank-connection"
        value={fields.connectionId}
        onchange={(e) => chooseConnection(e.currentTarget.value)}
        disabled={!editable}
        ><option value="">{t("New connection")}</option
        >{#each view.connections as connection (connection.id)}<option
            value={connection.id}
            >{privateText(connection.name)} — {t(
              labels[connection.state],
            )}</option
          >{/each}</select
      >
      {#if selected}<div class="connection-status">
          <h3>{privateText(selected.name)}</h3>
          <p>
            {labels[selected.state]}. {selected.last_success
              ? `Last successful request: ${new Date(selected.last_success).toLocaleString()}.`
              : "No successful provider request recorded."}
          </p>
          {#if selected.message}<p>{selected.message}</p>{/if}
          {#if selected.state === "claim_inflight" || selected.state === "claim_failed"}<p
            >
              A one-use claim may have been consumed. Replace its setup token
              explicitly; retrying with a new identity cannot safely reclaim it.
            </p>{/if}
          <div class="actions">
            {#if selected.state === "claim_pending"}<button
                class="primary"
                onclick={() =>
                  begin(
                    commandPath(selected.id),
                    { action: "claim", generation: selected.generation },
                    "Claim SimpleFIN setup token",
                  )}
                disabled={!writable}>{t("Claim setup token")}</button
              >{/if}<button
              onclick={() =>
                begin(
                  commandPath(selected.id),
                  { action: "discover", generation: selected.generation },
                  "Discover provider accounts",
                )}
              disabled={!writable || selected.state !== "connected"}
              >{t("Discover provider accounts")}</button
            ><button
              onclick={() =>
                begin(
                  commandPath(selected.id),
                  { action: "remove", generation: selected.generation },
                  "Remove SimpleFIN connection",
                )}
              disabled={!writable}>{t("Remove connection")}</button
            >
          </div>
          <p>
            Removing a connection deletes its server credentials and outstanding
            downloads. Saved transactions remain. Replace the token to reconnect
            an existing mapping.
          </p>
        </div>{/if}
      {#if selected}<details
          open={selected.state === "credentials_missing" ||
            selected.state === "claim_failed" ||
            selected.state === "claim_inflight"}
        >
          <summary>{t("Replace credentials")}</summary
          >{@render credentialForm()}
        </details>{:else}{@render credentialForm()}{/if}
    </section>
    {#if selected}<section aria-labelledby="bank-accounts-title">
        <h2 id="bank-accounts-title">
          {t("Provider accounts and local mapping")}
        </h2>
        <p>
          Provider balances are separate from ledger balances. Currency is
          supplied by the provider; no exchange-rate conversion is applied.
        </p>
        {#if !remotes.length}<p>
            No current provider accounts. Claim the connection and discover its
            accounts before linking.
          </p>{/if}
        {#if unavailableLinks.length}<section
            aria-labelledby="bank-unavailable-title"
          >
            <h3 id="bank-unavailable-title">
              {t("Unavailable local accounts")}
            </h3>
            <p>
              A linked local account was deleted or is no longer available.
              Explicitly unlink its mapping before assigning that provider
              account elsewhere. Saved transaction history is preserved.
            </p>
            {#each unavailableLinks as old (old.id)}<div>
                <p>
                  {privateText(
                    remoteById.get(old.remote_account_id)?.name ??
                      "Provider account",
                  )}
                </p>
                <button
                  onclick={() =>
                    begin(
                      `${root}/accounts/${old.id}/banking/commands`,
                      {
                        action: "unlink",
                        snapshot: view!.snapshot,
                        revision: old.revision,
                      },
                      "Unlink unavailable local account",
                      undefined,
                      true,
                    )}
                  disabled={!writable}>{t("Unlink unavailable account")}</button
                >
              </div>{/each}
          </section>{/if}
        <div class="mapping-grid">
          <div>
            <label for="bank-local">{t("Local account")}</label><select
              id="bank-local"
              value={fields.accountId}
              onchange={(e) => chooseAccount(e.currentTarget.value)}
              disabled={!editable}
              ><option value="">{t("Choose a local account")}</option
              >{#each accounts.filter((a) => !a.deleted) as account (account.id)}<option
                  value={account.id}
                  >{privateText(account.name)}{account.closed
                    ? " (closed)"
                    : ""}</option
                >{/each}</select
            >
          </div>
          <div>
            <label for="bank-remote">{t("Provider account")}</label><select
              id="bank-remote"
              value={fields.remoteId}
              onchange={(e) => edit("remoteId", e.currentTarget.value)}
              disabled={!editable}
              ><option value="">{t("Choose a provider account")}</option
              >{#each remotes as remote (remote.id)}<option value={remote.id}
                  >{privateText(remote.institution)} — {privateText(
                    remote.name,
                  )}{remote.generation !== selected.generation
                    ? " (review after discovery)"
                    : ""}</option
                >{/each}</select
            >
          </div>
        </div>
        {#if !accounts.length}<p>
            Create an empty local account from Accounts, then return here to
            link it.
          </p>{/if}
        {#if fields.remoteId}{@const remote = remotes.find(
            (r) => r.id === fields.remoteId,
          )}{#if remote}<h3>{privateText(remote.name)}</h3>
            <p>
              Institution: {privateText(remote.institution)}. Provider account
              reference: {privateText(remote.remote_id)}.
            </p>
            <dl class="balances">
              <div>
                <dt>{t("Provider balance")}</dt>
                <dd>
                  {masked
                    ? "••••"
                    : `${remote.balance || "Not retained"}${remote.currency ? ` ${remote.currency}` : " (currency not retained)"}`}
                </dd>
              </div>
              <div>
                <dt>{t("Available balance")}</dt>
                <dd>
                  {masked
                    ? "••••"
                    : (remote.available_balance ?? "Not provided")}
                </dd>
              </div>
              <div>
                <dt>{t("Provider balance date")}</dt>
                <dd>{providerDate(remote.balance_date)}</dd>
              </div>
              {#if local}<div>
                  <dt>{t("Current ledger balance")}</dt>
                  <dd>{masked ? "••••" : formatAmount(local.balance)}</dd>
                </div>{/if}
            </dl>
            {#each diagnostics(remote.diagnostics) as diagnostic (JSON.stringify(diagnostic))}<p
              >
                {diagnostic.message}
              </p>{/each}{/if}{/if}
        {#if local}<p>
            {link
              ? `This local account has a saved mapping${link.connection_generation !== selected.generation ? " that needs review after credential replacement" : ""}.`
              : "This local account has no active provider mapping."}
          </p>
          <label for="bank-opening-mode">{t("Initial opening balance")}</label
          ><select
            id="bank-opening-mode"
            value={fields.openingMode}
            onchange={(e) =>
              edit(
                "openingMode",
                e.currentTarget.value as BankFields["openingMode"],
              )}
            disabled={!editable}
            ><option value="auto">{t("Automatic for an empty account")}</option
            ><option value="none">{t("No opening entry")}</option><option
              value="custom">{t("Custom amount for an empty account")}</option
            ></select
          >
          <p>
            Automatic opening uses the provider balance and returned initial
            transactions. Existing ledger history is kept. Choose no opening
            entry when the balance is already represented.
          </p>
          {#if fields.openingMode === "custom"}<p>
              Saved draft amount format: {formatAmountInput(
                123456,
                fields.format,
              )} (example). To use a changed display format, discard this unsent draft
              and start again.
            </p>
            <div class="mapping-grid">
              <label
                >{t("Opening amount")}<input
                  inputmode="decimal"
                  value={displayReady ? fields.openingAmount : ""}
                  oninput={(e) => edit("openingAmount", e.currentTarget.value)}
                  disabled={!editable}
                /></label
              ><DateInput
                id="bank-opening-date"
                label="Opening date"
                format="yyyy-MM-dd"
                bind:value={
                  () => fields.openingDate, (v) => edit("openingDate", v)
                }
                disabled={!editable}
              />
            </div>{/if}
          <div class="actions">
            <button
              class="primary"
              onclick={linkAccount}
              disabled={!writable ||
                !accountView ||
                !fields.remoteId ||
                selected.state !== "connected" ||
                chosenRemote?.generation !== selected.generation}
              >{t("Save account mapping")}</button
            >{#if link}<button
                onclick={() =>
                  begin(
                    `${root}/accounts/${local.id}/banking/commands`,
                    {
                      action: "unlink",
                      snapshot: accountView?.snapshot ?? view!.snapshot,
                      revision: link.revision,
                    },
                    "Unlink provider account",
                    undefined,
                    true,
                  )}
                disabled={!writable || !accountView}
                >{t("Unlink account")}</button
              >{/if}
          </div>
          {#if accountView}<BankSettings
              value={fields.settings}
              disabled={!editable}
              onchange={(value) => edit("settings", value)}
            />{#if accountView.preferences.error}<p>
                {accountView.preferences.error} Review the proposed values before
                explicitly saving replacements. Supported saved choices were retained.
              </p>{/if}<button onclick={saveSettings} disabled={!writable}
              >{t("Save refresh preferences")}</button
            >{:else}<p>
              {t("Loading this account's refresh preferences\u2026")}
            </p>{/if}
          <section aria-labelledby="bank-refresh-title">
            <h3 id="bank-refresh-title">{t("Refresh linked account")}</h3>
            <p>
              Choose 1–90 days. The start is included; the end is excluded.
              Overlap is safe: saved bank IDs prevent duplicate imports. Each
              connection allows at most 24 provider requests in 24 hours,
              including failed or canceled downloads.
            </p>
            <div class="mapping-grid">
              <DateInput
                id="bank-start"
                label="Start date (included)"
                format="yyyy-MM-dd"
                bind:value={() => fields.startDate, (v) => edit("startDate", v)}
                disabled={!editable}
              /><DateInput
                id="bank-end"
                label="End date (excluded)"
                format="yyyy-MM-dd"
                bind:value={() => fields.endDate, (v) => edit("endDate", v)}
                disabled={!editable}
              />
            </div>
            <button
              class="primary"
              onclick={refreshTransactions}
              disabled={!writable ||
                !mappingCurrent ||
                selected.state !== "connected" ||
                !accountView}>{t("Refresh linked account")}</button
            >
          </section>
        {/if}
      </section>{/if}
  {/if}
</section>

<style>
  .banking {
    max-width: 72rem;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1.5rem;
    flex-wrap: wrap;
  }
  header h1 {
    margin: 0;
  }
  p {
    max-width: 72ch;
    overflow-wrap: anywhere;
  }
  header p,
  .connection-status p {
    color: var(--muted);
  }
  section section {
    margin-top: 2rem;
    padding-top: 1.25rem;
    border-top: 1px solid var(--line);
  }
  h1,
  h2,
  h3 {
    overflow-wrap: anywhere;
  }
  h2 {
    margin: 0 0 1rem;
  }
  h3 {
    margin: 1.5rem 0 0.75rem;
  }
  .actions {
    display: flex;
    gap: 0.65rem;
    flex-wrap: wrap;
    margin: 1rem 0;
  }
  form {
    margin-top: 1.5rem;
    max-width: 46rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    font-weight: 600;
    margin: 0.75rem 0;
  }
  input,
  select {
    max-width: 100%;
    min-width: 0;
  }
  summary {
    min-height: 44px;
    align-content: center;
    cursor: pointer;
    font-weight: 600;
  }
  .mapping-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
    align-items: start;
  }
  .balances {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
    gap: 1rem;
    margin: 1.5rem 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0.25rem 0 0;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .recovery {
    background: var(--sidebar);
    padding: 1.25rem;
  }
  [role="alert"] {
    margin: 1rem 0;
    padding: 1rem;
    background: var(--error-bg);
    color: var(--error);
  }
  @media (max-width: 700px) {
    .mapping-grid {
      grid-template-columns: 1fr;
    }
    header {
      display: block;
    }
    header > button {
      margin: 0.5rem 0;
    }
    .actions > button {
      flex: 1 1 14rem;
    }
    .recovery {
      padding: 1rem;
    }
  }
</style>
