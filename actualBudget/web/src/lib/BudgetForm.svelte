<script lang="ts">
  import {
    errorMessage,
    request,
    RequestError,
    uuid,
    type Budget,
  } from "./api";
  let {
    oncreated,
    oncancel,
  }: { oncreated: (budget: Budget) => void; oncancel: () => void } = $props();
  let name = $state("");
  let method = $state<"tracking" | "envelope">("tracking");
  let busy = $state(false);
  let error = $state("");
  let uncertain = $state(false);
  let pending: { body: Budget; key: string } | undefined;
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    if (!name.trim()) {
      error = "Enter a budget name.";
      return;
    }
    if (!uncertain)
      pending = {
        body: { id: uuid(), name: name.trim(), budget_type: method },
        key: uuid(),
      };
    busy = true;
    error = "";
    try {
      const result = await request<{ data: Budget }>(
        "/budgets",
        pending!.body,
        pending!.key,
      );
      oncreated(result.data);
    } catch (e) {
      error = errorMessage(e);
      uncertain = e instanceof RequestError && e.uncertain;
    } finally {
      busy = false;
    }
  }
</script>

<section class="form-section" aria-labelledby="budget-form-title">
  <h1 id="budget-form-title">New budget</h1>
  <p class="muted">Keep related accounts together in one budget.</p>
  <form onsubmit={submit} aria-busy={busy}>
    <fieldset disabled={busy || uncertain}>
      <label for="budget-name">Budget name</label>
      <input
        id="budget-name"
        bind:value={name}
        required
        maxlength="120"
        autocomplete="off"
      />
      <label for="budget-method">Budgeting method</label>
      <select
        id="budget-method"
        bind:value={method}
        aria-describedby="method-hint"
        ><option value="tracking">Tracking</option><option value="envelope"
          >Envelope</option
        ></select
      >
      <p id="method-hint" class="hint">
        Tracking follows income and spending. Envelope assigns money to
        categories.
      </p>
    </fieldset>
    {#if error}<div class="error" role="alert">
        {error}{#if uncertain}<p>
            The outcome is unconfirmed. Retry this request before changing its
            details.
          </p>{/if}
      </div>{/if}
    <div class="actions">
      <button class="primary" disabled={busy}
        >{busy
          ? "Creating…"
          : uncertain
            ? "Retry creation"
            : "Create budget"}</button
      ><button type="button" onclick={oncancel} disabled={busy || uncertain}
        >Cancel</button
      >
    </div>
  </form>
</section>
