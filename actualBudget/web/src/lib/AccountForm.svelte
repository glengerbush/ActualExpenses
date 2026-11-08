<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import type { Account } from "./api";
  import DateInput from "./DateInput.svelte";
  import { preferences } from "./preferences.svelte";
  import { CreationSession } from "./CreationSession.svelte";
  import { newAccountDraft, type AccountDraft } from "./drafts";
  let {
    budgetId,
    oncreated,
    oncancel,
  }: {
    budgetId: string;
    oncreated: (account: Account) => void;
    oncancel: () => void;
  } = $props();
  const session = new CreationSession<AccountDraft, Account>(
    () => newAccountDraft(budgetId),
    () => ({ kind: "account", budget_id: budgetId }),
  );
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!session.draft?.pending && !preferences.hasUsableShared) {
      session.error =
        "Load this budget's display preferences before creating an account.";
      return;
    }
    const result = await session.submit();
    if (result) oncreated(result);
  }
  function cancel() {
    if (session.cancel()) oncancel();
  }
  onMount(() => session.load());
</script>

<section class="form-section" aria-labelledby="account-form-title">
  <h1 id="account-form-title">{t("Add account")}</h1>
  <p class="muted">
    {t("Start with the balance on the date you want to track from.")}
  </p>
  {#if session.restored}<p class="hint" role="status">
      {t("Recovered your saved draft.")}
    </p>{/if}
  {#if session.draft}
    <form onsubmit={submit} aria-busy={session.busy}>
      <fieldset
        disabled={session.busy ||
          !!session.draft.pending ||
          !preferences.hasUsableShared}
      >
        <label for="account-name">{t("Account name")}</label><input
          id="account-name"
          bind:value={
            () => session.draft!.fields.name,
            (value) => session.update({ name: value })
          }
          required
          maxlength="120"
          autocomplete="off"
        />
        <div class="form-row">
          <div>
            <label for="opening-balance">{t("Opening balance")}</label><input
              id="opening-balance"
              maxlength="128"
              bind:value={
                () => session.draft!.fields.balance,
                (value) => session.update({ balance: value })
              }
              inputmode="decimal"
              required
              aria-describedby="balance-hint"
            />
            <p id="balance-hint" class="hint">
              {t("Use a minus sign for money owed.")}
            </p>
          </div>
          <div>
            <DateInput
              id="opening-date"
              label={t("Opening date")}
              bind:value={
                () => session.draft!.fields.date,
                (value) => session.update({ date: value })
              }
              format={session.draft.fields.date_format}
              required
            />
          </div>
        </div>
        <label for="account-type">{t("Account type")}</label><select
          id="account-type"
          bind:value={
            () => session.draft!.fields.off_budget,
            (value) => session.update({ off_budget: value })
          }
          ><option value={false}>{t("On budget")}</option><option value={true}
            >{t("Off budget")}</option
          ></select
        >
        <p class="hint">
          {t(
            "Off-budget accounts track assets or debts outside your spending budget.",
          )}
        </p>
        <label for="account-notes"
          >{t("Notes")}<span class="muted">{t("(optional)")}</span></label
        ><textarea
          id="account-notes"
          bind:value={
            () => session.draft!.fields.notes,
            (value) => session.update({ notes: value })
          }
          rows="3"
          maxlength="10000"></textarea>
      </fieldset>
      {#if session.error}<div class="error" role="alert">
          {t(session.error)}
        </div>{/if}
      {#if session.draft.pending && !session.busy}<p
          class="error"
          role="status"
        >
          {t(
            "The outcome is unconfirmed. Retry this saved request before changing its details.",
          )}
        </p>{/if}
      <div class="actions">
        <button class="primary" disabled={session.busy}
          >{t(
            session.busy
              ? "Creating…"
              : session.draft.pending
                ? "Retry creation"
                : "Create account",
          )}</button
        ><button
          type="button"
          onclick={cancel}
          disabled={session.busy || !!session.draft.pending}
          >{t("Cancel")}</button
        >
      </div>
    </form>
  {:else}
    {#if session.error}<p class="error" role="alert">{t(session.error)}</p>{/if}
    <button onclick={() => session.load()}>{t("Retry draft recovery")}</button>
  {/if}
</section>
