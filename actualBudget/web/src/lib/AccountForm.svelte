<script lang="ts">
  import { onMount } from "svelte";
  import type { Account } from "./api";
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
  const session = new CreationSession<AccountDraft, Account>(() =>
    newAccountDraft(budgetId),
  );
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const result = await session.submit();
    if (result) oncreated(result);
  }
  function cancel() {
    if (session.cancel()) oncancel();
  }
  onMount(() => session.load());
</script>

<section class="form-section" aria-labelledby="account-form-title">
  <h1 id="account-form-title">Add account</h1>
  <p class="muted">
    Start with the balance on the date you want to track from.
  </p>
  {#if session.restored}<p class="hint" role="status">
      Recovered your saved draft.
    </p>{/if}
  {#if session.draft}
    <form onsubmit={submit} aria-busy={session.busy}>
      <fieldset disabled={session.busy || !!session.draft.pending}>
        <label for="account-name">Account name</label><input
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
            <label for="opening-balance">Opening balance</label><input
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
              Use a minus sign for money owed.
            </p>
          </div>
          <div>
            <label for="opening-date">Opening date</label><input
              id="opening-date"
              type="date"
              min="0001-01-01"
              max="9999-12-31"
              bind:value={
                () => session.draft!.fields.date,
                (value) => session.update({ date: value })
              }
              required
            />
          </div>
        </div>
        <label for="account-type">Account type</label><select
          id="account-type"
          bind:value={
            () => session.draft!.fields.off_budget,
            (value) => session.update({ off_budget: value })
          }
          ><option value={false}>On budget</option><option value={true}
            >Off budget</option
          ></select
        >
        <p class="hint">
          Off-budget accounts track assets or debts outside your spending
          budget.
        </p>
        <label for="account-notes"
          >Notes <span class="muted">(optional)</span></label
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
          {session.error}
        </div>{/if}
      {#if session.draft.pending && !session.busy}<p
          class="error"
          role="status"
        >
          The outcome is unconfirmed. Retry this saved request before changing
          its details.
        </p>{/if}
      <div class="actions">
        <button class="primary" disabled={session.busy}
          >{session.busy
            ? "Creating…"
            : session.draft.pending
              ? "Retry creation"
              : "Create account"}</button
        ><button
          type="button"
          onclick={cancel}
          disabled={session.busy || !!session.draft.pending}>Cancel</button
        >
      </div>
    </form>
  {:else}
    {#if session.error}<p class="error" role="alert">{session.error}</p>{/if}
    <button onclick={() => session.load()}>Retry draft recovery</button>
  {/if}
</section>
