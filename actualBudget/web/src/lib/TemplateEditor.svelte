<script lang="ts">
  import { visualNumber } from "./visual-automations";
  import type { AmountFormat } from "./money";
  import DateInput from "./DateInput.svelte";
  import type { DateFormat } from "./preferences/types";
  type Row = Record<string, unknown>;
  let {
    rows,
    month,
    income,
    schedules,
    dateFormat,
    format,
    disabled = false,
    onchange,
  }: {
    rows: Row[];
    month: string;
    income: { id: string; name: string }[];
    schedules: { id: string; name: string }[];
    dateFormat: DateFormat;
    format: AmountFormat;
    disabled?: boolean;
    onchange: (rows: Row[]) => void;
  } = $props();
  const kinds = [
    ["simple", "Fixed monthly amount"],
    ["by", "Save by a month"],
    ["spend", "Spend over a period"],
    ["periodic", "Repeating contribution"],
    ["schedule", "Scheduled payments"],
    ["average", "Historical average"],
    ["copy", "Copy a previous month"],
    ["percentage", "Percentage of income"],
    ["limit", "Balance limit"],
    ["refill", "Refill to the limit"],
    ["remainder", "Remaining funds"],
    ["goal", "Savings target"],
  ];
  const fields: Record<
    string,
    { key: string; label: string; type?: string; min?: number }[]
  > = {
    simple: [{ key: "monthly", label: "Monthly amount" }],
    by: [
      { key: "amount", label: "Target amount" },
      { key: "month", label: "Target month", type: "month" },
      { key: "repeat", label: "Repeat interval", min: 1 },
    ],
    spend: [
      { key: "amount", label: "Target amount" },
      { key: "month", label: "Target month", type: "month" },
      { key: "from", label: "Spending starts", type: "month" },
      { key: "repeat", label: "Repeat interval", min: 1 },
    ],
    periodic: [
      { key: "amount", label: "Contribution" },
      { key: "period.amount", label: "Every", min: 1 },
      { key: "starting", label: "Starting date", type: "date" },
    ],
    average: [{ key: "numMonths", label: "Previous months", min: 1 }],
    copy: [{ key: "lookBack", label: "Months ago", min: 1 }],
    percentage: [{ key: "percent", label: "Percent", min: 0 }],
    limit: [
      { key: "amount", label: "Maximum balance" },
      { key: "start", label: "Weekly limit starts", type: "date" },
    ],
    remainder: [{ key: "weight", label: "Share weight", min: 1 }],
    goal: [{ key: "amount", label: "Savings target" }],
  };
  function fresh(kind: string): Row {
    const base = {
      type: kind,
      directive: kind === "goal" ? "goal" : "template",
      priority: ["goal", "limit", "remainder"].includes(kind) ? null : 0,
    };
    switch (kind) {
      case "simple":
        return { ...base, monthly: 0 };
      case "by":
        return { ...base, amount: 0, month };
      case "spend":
        return { ...base, amount: 0, month, from: month };
      case "periodic":
        return {
          ...base,
          amount: 0,
          period: { amount: 1, period: "month" },
          starting: month + "-01",
        };
      case "schedule":
        return {
          ...base,
          scheduleId: schedules[0]?.id ?? "",
          name: schedules[0]?.name ?? "",
          full: false,
        };
      case "average":
        return { ...base, numMonths: 3 };
      case "copy":
        return { ...base, lookBack: 1 };
      case "percentage":
        return {
          ...base,
          percent: 10,
          category: "all income",
          previous: false,
        };
      case "limit":
        return { ...base, amount: 0, period: "monthly", hold: false };
      case "remainder":
        return { ...base, weight: 1 };
      case "goal":
        return { ...base, amount: 0 };
      default:
        return base;
    }
  }
  function get(row: Row, key: string): unknown {
    const [a, b] = key.split(".");
    return b ? (row[a] as Row)?.[b] : row[a];
  }
  function update(index: number, key: string, value: unknown) {
    const next = structuredClone(rows);
    const [a, b] = key.split(".");
    if (b) next[index][a] = { ...(next[index][a] as Row), [b]: value };
    else if (value === undefined) delete next[index][a];
    else next[index][a] = value;
    if (
      key === "repeat" &&
      ["by", "spend"].includes(String(next[index].type))
    ) {
      if (value === null || value === "") {
        delete next[index].annual;
      } else if (next[index].annual === undefined) {
        next[index].annual = false;
      }
    }
    if (key === "adjustment") {
      if (value === undefined) {
        delete next[index].adjustmentType;
      } else if (next[index].adjustmentType === undefined) {
        next[index].adjustmentType = "percent";
      }
    }
    onchange(next);
  }
  function move(index: number, delta: number) {
    const next = structuredClone(rows);
    [next[index], next[index + delta]] = [next[index + delta], next[index]];
    onchange(next);
  }
</script>

<fieldset {disabled} class="template-editor">
  <legend>Visual allocation rules</legend>
  <p>
    Amounts use major currency units. Lower priority numbers run first; row
    order resolves ties. Limits cap the category balance; refill funds the gap
    to a limit.
  </p>
  {#each rows as row, index (index)}<fieldset class="rule">
      <legend>Rule {index + 1}</legend>
      <div class="fields">
        <label
          >Rule type<select
            value={String(row.type)}
            onchange={(e) =>
              onchange(
                rows.map((r, i) =>
                  i === index
                    ? fresh(e.currentTarget.value)
                    : structuredClone(r),
                ),
              )}
            >{#each kinds as [value, label] (value)}<option {value}
                >{label}</option
              >{/each}</select
          ><span class="selected-rule"
            >Selected: {kinds.find(([value]) => value === row.type)?.[1] ??
              String(row.type)}</span
          ></label
        >
        {#if !["goal", "limit", "remainder"].includes(String(row.type))}<label
            >Priority<input
              type="text"
              inputmode="numeric"
              value={visualNumber(row.priority ?? 0, format)}
              oninput={(e) => update(index, "priority", e.currentTarget.value)}
            /></label
          >{/if}
        {#each fields[String(row.type)] ?? [] as field (field.key)}
          {#if field.type === "date" && (field.key !== "start" || row.period === "weekly")}<DateInput
              id={`automation-${index}-${field.key}`}
              label={field.label}
              format={dateFormat}
              bind:value={
                () => String(get(row, field.key) ?? ""),
                (v) => update(index, field.key, v)
              }
            />
          {:else if field.type !== "date"}<label
              >{field.label}<input
                type={field.type ?? "text"}
                inputmode={field.type ? undefined : "decimal"}
                step={field.min === 1 ? "1" : "any"}
                min={field.min}
                value={field.type
                  ? String(get(row, field.key) ?? "")
                  : visualNumber(get(row, field.key), format)}
                oninput={(e) => update(index, field.key, e.currentTarget.value)}
              /></label
            >{/if}{/each}
        {#if row.type === "periodic"}<label
            >Interval unit<select
              value={String(get(row, "period.period"))}
              onchange={(e) =>
                update(index, "period.period", e.currentTarget.value)}
              ><option value="day">Days</option><option value="week"
                >Weeks</option
              ><option value="month">Months</option><option value="year"
                >Years</option
              ></select
            ></label
          >{/if}
        {#if row.type === "by" || row.type === "spend"}<label
            >Repeat unit<select
              disabled={!get(row, "repeat")}
              value={row.annual === true ? "year" : "month"}
              onchange={(e) =>
                update(index, "annual", e.currentTarget.value === "year")}
              ><option value="month">Months</option><option value="year"
                >Years</option
              ></select
            ></label
          >
          <p>Leave the repeat interval empty for a one-time target.</p>{/if}
        {#if row.type === "schedule"}<label
            >Schedule<select
              value={String(row.scheduleId ?? "")}
              onchange={(e) =>
                update(index, "scheduleId", e.currentTarget.value)}
              ><option value="">Choose schedule</option
              >{#if row.scheduleId && !schedules.some((s) => s.id === row.scheduleId)}<option
                  value={String(row.scheduleId)}
                  >Retained schedule: {String(
                    row.name ?? row.scheduleId,
                  )}</option
                >{/if}{#each schedules as schedule (schedule.id)}<option
                  value={schedule.id}>{schedule.name}</option
                >{/each}</select
            ></label
          ><label class="check"
            ><input
              type="checkbox"
              checked={row.full === true}
              onchange={(e) => update(index, "full", e.currentTarget.checked)}
            />Fund the full scheduled amount</label
          >{/if}
        {#if row.type === "percentage"}<label
            >Income basis<select
              value={String(row.category)}
              onchange={(e) => update(index, "category", e.currentTarget.value)}
              ><option value="all income">All income</option><option
                value="available funds">Available funds</option
              >{#each income as category (category.id)}<option
                  value={category.id}>{category.name}</option
                >{/each}</select
            ></label
          ><label class="check"
            ><input
              type="checkbox"
              checked={row.previous === true}
              onchange={(e) =>
                update(index, "previous", e.currentTarget.checked)}
            />Use previous month</label
          >{/if}
        {#if row.type === "limit"}<label
            >Limit period<select
              value={String(row.period ?? "monthly")}
              onchange={(e) => update(index, "period", e.currentTarget.value)}
              ><option value="monthly">Monthly</option><option value="weekly"
                >Weekly</option
              ><option value="daily">Daily</option></select
            ></label
          ><label class="check"
            ><input
              type="checkbox"
              checked={row.hold === true}
              onchange={(e) => update(index, "hold", e.currentTarget.checked)}
            />Hold a fixed limit</label
          >{/if}
        {#if row.type === "average" || row.type === "schedule"}<label
            >Adjustment<input
              type="text"
              inputmode="decimal"
              step="any"
              value={visualNumber(row.adjustment, format)}
              oninput={(e) =>
                update(
                  index,
                  "adjustment",
                  e.currentTarget.value === ""
                    ? undefined
                    : e.currentTarget.value,
                )}
            /></label
          ><label
            >Adjustment unit<select
              value={String(row.adjustmentType ?? "percent")}
              onchange={(e) =>
                update(index, "adjustmentType", e.currentTarget.value)}
              ><option value="percent">Percent</option><option value="fixed"
                >Major currency units</option
              ></select
            ></label
          >{/if}
        {#if ["simple", "periodic", "remainder", "copy"].includes(String(row.type))}
          <details>
            <summary>Embedded balance limit</summary>
            {#if row.limit}<label
                >Limit amount<input
                  type="text"
                  inputmode="decimal"
                  step="any"
                  value={visualNumber(get(row, "limit.amount"), format)}
                  oninput={(e) =>
                    update(index, "limit.amount", e.currentTarget.value)}
                /></label
              ><label
                >Embedded limit period<select
                  value={String(get(row, "limit.period") ?? "monthly")}
                  onchange={(e) =>
                    update(index, "limit.period", e.currentTarget.value)}
                  ><option value="monthly">Monthly</option><option
                    value="weekly">Weekly</option
                  ><option value="daily">Daily</option></select
                ></label
              >{#if get(row, "limit.period") === "weekly"}<DateInput
                  id={`automation-${index}-limit-start`}
                  label="Embedded weekly limit starts"
                  format={dateFormat}
                  bind:value={
                    () => String(get(row, "limit.start") ?? ""),
                    (v) => update(index, "limit.start", v)
                  }
                />{/if}<label class="check"
                ><input
                  type="checkbox"
                  checked={get(row, "limit.hold") === true}
                  onchange={(e) =>
                    update(index, "limit.hold", e.currentTarget.checked)}
                />Hold embedded limit fixed</label
              ><button onclick={() => update(index, "limit", undefined)}
                >Remove embedded limit</button
              >{:else}<button
                onclick={() =>
                  update(index, "limit", {
                    amount: 0,
                    period: "monthly",
                    hold: false,
                  })}>Add embedded limit</button
              >{/if}
          </details>
        {/if}
        <label class="description"
          >Description<textarea
            rows="2"
            value={String(row.description ?? "")}
            oninput={(e) => update(index, "description", e.currentTarget.value)}
          ></textarea></label
        >
      </div>
      <div class="actions">
        <button onclick={() => move(index, -1)} disabled={index === 0}
          >Move rule up</button
        ><button
          onclick={() => move(index, 1)}
          disabled={index === rows.length - 1}>Move rule down</button
        ><button onclick={() => onchange(rows.filter((_, i) => i !== index))}
          >Remove rule</button
        >
      </div>
    </fieldset>{/each}
  <button
    onclick={() => onchange([...structuredClone(rows), fresh("simple")])}
    disabled={rows.length >= 256}>Add allocation rule</button
  >
</fieldset>

<style>
  .selected-rule {
    display: none;
  }
  @media (max-width: 40rem) {
    .selected-rule {
      display: block;
      overflow-wrap: anywhere;
    }
  }
  .template-editor {
    border: 0;
    padding: 0;
    min-width: 0;
  }
  .rule {
    margin-block: 1rem;
    padding: 1rem;
    border: 1px solid var(--border);
    min-width: 0;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr));
    gap: 1rem;
  }
  label {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }
  input,
  select,
  textarea {
    max-width: 100%;
    width: 100%;
    box-sizing: border-box;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
  }
  .description {
    grid-column: 1/-1;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-block: 1rem;
  }
  button {
    min-height: 44px;
  }
</style>
