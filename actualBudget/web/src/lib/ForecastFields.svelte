<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { Account } from "./api";
  let {
    meta,
    accounts,
    onchange,
  }: {
    meta: Record<string, unknown>;
    accounts: Account[];
    onchange: (key: string, value: unknown) => void;
  } = $props();
</script>

<div class="fields">
  <label
    >{t("Forecast source")}<select
      value={String(meta.source ?? "schedules")}
      onchange={(e) => onchange("source", e.currentTarget.value)}
      ><option value="schedules">{t("Schedules")}</option><option
        value="tracking-budget">{t("Tracking budget")}</option
      >{#if meta.source && !["schedules", "tracking-budget"].includes(String(meta.source))}<option
          value={String(meta.source)}
          >{t("Preserved:")} {String(meta.source)}</option
        >{/if}</select
    ></label
  >
  <label
    >{t("Interval")}<select
      value={String(meta.granularity ?? "Monthly")}
      onchange={(e) => onchange("granularity", e.currentTarget.value)}
      ><option value="Daily">{t("Daily")}</option><option value="Monthly"
        >{t("Monthly")}</option
      >{#if meta.granularity && !["Daily", "Monthly"].includes(String(meta.granularity))}<option
          value={String(meta.granularity)}
          >{t("Preserved:")} {String(meta.granularity)}</option
        >{/if}</select
    ></label
  >
  {#if meta.source === "tracking-budget"}<p>
      {t(
        "Tracking budgets only: monthly planned income minus expenses, up to 12 months. Account and transaction filters are ignored. Choose Schedules for envelope budgets or daily projections.",
      )}
    </p>{/if}
  <label class="check"
    ><input
      type="checkbox"
      checked={meta.accounts === undefined}
      onchange={(e) =>
        onchange(
          "accounts",
          e.currentTarget.checked ? undefined : accounts.map((a) => a.id),
        )}
    />{t("All accounts and accountless schedules")}</label
  >
  {#if meta.accounts !== undefined}<label
      >{t("Accounts")}<select
        multiple
        value={Array.isArray(meta.accounts) ? (meta.accounts as string[]) : []}
        onchange={(e) =>
          onchange(
            "accounts",
            Array.from(e.currentTarget.selectedOptions).map((o) => o.value),
          )}
        >{#each accounts as a (a.id)}<option value={a.id}>{a.name}</option
          >{/each}{#each (Array.isArray(meta.accounts) ? (meta.accounts as string[]) : []).filter((id) => !accounts.some((a) => a.id === id)) as id (id)}<option
            value={id}
            >{t("Unavailable account (reference preserved)")} {id}</option
          >{/each}</select
      ></label
    >{/if}
  <p>
    {t(
      "Estimates use posted history and simulate upcoming schedules and rules. Simulations never post entries or advance schedules. Range amounts and early payments follow the schedule matching rules; changes require recalculation.",
    )}
  </p>
</div>

<style>
  .fields {
    display: grid;
    gap: 1rem;
    min-width: 0;
  }
  label {
    display: grid;
    gap: 0.35rem;
  }
  select {
    max-width: 100%;
    min-height: 44px;
  }
  .check input {
    width: auto;
    flex: none;
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  p {
    max-width: 70ch;
    margin: 0.3rem 0;
  }
</style>
