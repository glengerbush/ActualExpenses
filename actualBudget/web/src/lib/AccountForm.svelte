<script lang="ts">
  import {
    errorMessage,
    request,
    RequestError,
    uuid,
    type Account,
    type NewAccount,
  } from "./api";
  import { parseAmount, today } from "./money";
  let {
    budgetId,
    oncreated,
    oncancel,
  }: {
    budgetId: string;
    oncreated: (account: Account) => void;
    oncancel: () => void;
  } = $props();
  let name = $state("");
  let balance = $state("0.00");
  let date = $state(today());
  let offBudget = $state(false);
  let notes = $state("");
  let busy = $state(false);
  let error = $state("");
  let uncertain = $state(false);
  let pending: { body: NewAccount; key: string } | undefined;
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    try {
      if (!name.trim()) throw new Error("Enter an account name.");
      if (!uncertain)
        pending = {
          body: {
            id: uuid(),
            name: name.trim(),
            off_budget: offBudget,
            opening_balance: parseAmount(balance),
            opening_date: date,
            notes,
          },
          key: uuid(),
        };
      busy = true;
      const result = await request<{ data: Account }>(
        `/budgets/${budgetId}/accounts`,
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

<section class="form-section" aria-labelledby="account-form-title">
  <h1 id="account-form-title">Add account</h1>
  <p class="muted">
    Start with the balance on the date you want to track from.
  </p>
  <form onsubmit={submit} aria-busy={busy}>
    <fieldset disabled={busy || uncertain}>
      <label for="account-name">Account name</label><input
        id="account-name"
        bind:value={name}
        required
        maxlength="120"
        autocomplete="off"
      />
      <div class="form-row">
        <div>
          <label for="opening-balance">Opening balance</label><input
            id="opening-balance"
            bind:value={balance}
            inputmode="decimal"
            required
            aria-describedby="balance-hint"
          />
          <p id="balance-hint" class="hint">Use a minus sign for money owed.</p>
        </div>
        <div>
          <label for="opening-date">Opening date</label><input
            id="opening-date"
            type="date"
            min="0001-01-01"
            max="9999-12-31"
            bind:value={date}
            required
          />
        </div>
      </div>
      <label for="account-type">Account type</label><select
        id="account-type"
        bind:value={offBudget}
        ><option value={false}>On budget</option><option value={true}
          >Off budget</option
        ></select
      >
      <p class="hint">
        Off-budget accounts track assets or debts outside your spending budget.
      </p>
      <label for="account-notes"
        >Notes <span class="muted">(optional)</span></label
      ><textarea
        id="account-notes"
        bind:value={notes}
        rows="3"
        maxlength="10000"></textarea>
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
            : "Create account"}</button
      ><button type="button" onclick={oncancel} disabled={busy || uncertain}
        >Cancel</button
      >
    </div>
  </form>
</section>
