<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import type { Budget } from "./api";
  import { CreationSession } from "./CreationSession.svelte";
  import { newBudgetDraft, type BudgetDraft } from "./drafts";
  let {
    oncreated,
    oncancel,
  }: { oncreated: (budget: Budget) => void; oncancel: () => void } = $props();
  const session = new CreationSession<BudgetDraft, Budget>(newBudgetDraft);
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

<section class="form-section" aria-labelledby="budget-form-title">
  <h1 id="budget-form-title">{t("New budget")}</h1>
  <p class="muted">{t("Track your income and expenses.")}</p>
  {#if session.restored}<p class="hint" role="status">
      {t("Recovered your saved draft.")}
    </p>{/if}
  {#if session.draft}
    <form onsubmit={submit} aria-busy={session.busy}>
      <fieldset disabled={session.busy || !!session.draft.pending}>
        <label for="budget-name">{t("Budget name")}</label>
        <input
          id="budget-name"
          bind:value={
            () => session.draft!.fields.name,
            (value) => session.update({ name: value })
          }
          required
          maxlength="120"
          autocomplete="off"
        />
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
                : "Create budget",
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
