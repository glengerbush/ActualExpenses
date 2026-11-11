<script lang="ts">
  import { t } from "./i18n.svelte";
  import { captureAmountFormat, formatAmountInput, parseAmount } from "./money";
  import { errorMessage } from "./api";
  import type { Field } from "./monte-carlo";
  let {
    field,
    value,
    onchange,
  }: { field: Field; value: unknown; onchange: (value: unknown) => void } =
    $props();
  const hintId = $props.id();
  let draft = $state.raw<{
    value: unknown;
    text: string;
    format: string;
  } | null>(null);
  let text = $derived.by(() => {
    const format = captureAmountFormat();
    if (
      draft &&
      Object.is(draft.value, value) &&
      draft.format === JSON.stringify(format)
    )
      return draft.text;
    if (value == null) return "";
    if (typeof value !== "number") return String(value);
    if (field.kind === "money" && Number.isSafeInteger(value))
      return formatAmountInput(value, format);
    return field.kind === "percent"
      ? String(Number((value * 100).toPrecision(15)))
      : String(value);
  });
  function input(event: Event) {
    const element = event.currentTarget as HTMLInputElement;
    let next: unknown = element.value;
    try {
      if (field.kind !== "text") {
        if (field.nullable && element.value === "") next = null;
        else {
          next =
            field.kind === "money"
              ? parseAmount(element.value, captureAmountFormat())
              : element.valueAsNumber / (field.kind === "percent" ? 100 : 1);
          if (typeof next !== "number" || !Number.isFinite(next))
            throw Error("Enter a valid number before saving.");
          if (field.kind === "money" && field.min === 0 && next < 0)
            throw Error("Enter zero or a positive amount.");
        }
      }
      element.setCustomValidity("");
    } catch (error) {
      next = element.value;
      element.setCustomValidity(errorMessage(error));
    }
    draft = {
      value: next,
      text: element.value,
      format: JSON.stringify(captureAmountFormat()),
    };
    onchange(next);
  }
  function validity(element: HTMLInputElement) {
    const invalid =
      field.kind !== "text" &&
      !(field.nullable && value === null) &&
      (typeof value !== "number" ||
        !Number.isFinite(value) ||
        (field.kind === "money" &&
          (!Number.isSafeInteger(value) || (field.min === 0 && value < 0))));
    element.setCustomValidity(
      invalid ? t("Enter a valid number before saving.") : "",
    );
  }
</script>

<label class={{ check: field.kind === "boolean" }}>
  {#if field.kind === "boolean"}
    <input
      type="checkbox"
      checked={value === true}
      onchange={(e) => onchange(e.currentTarget.checked)}
    />{t(field.label)}
  {:else}
    {t(field.label)}
    {#if field.kind === "select"}
      <select
        value={String(value ?? "")}
        required={!field.nullable}
        aria-describedby={field.hint ? hintId : undefined}
        onchange={(e) => onchange(e.currentTarget.value || null)}
      >
        {#if field.nullable}<option value="">{t("None")}</option>{/if}
        {#each field.choices ?? [] as choice (choice[0])}<option
            value={choice[0]}>{t(choice[1])}</option
          >{/each}
        {#if value && !field.choices?.some(([key]) => key === value)}<option
            value={String(value)}>{t("Preserved:")} {String(value)}</option
          >{/if}
      </select>
    {:else}
      <input
        type={field.kind === "money" || field.kind === "text"
          ? "text"
          : "number"}
        inputmode={field.kind === "money" ? "decimal" : undefined}
        value={text}
        required={field.kind !== "text" && !field.nullable}
        min={field.kind === "money" ? undefined : field.min}
        max={field.max}
        step={field.step ?? "any"}
        maxlength={field.kind === "text" ? 4096 : undefined}
        aria-describedby={field.hint ? hintId : undefined}
        oninput={input}
        {@attach validity}
      />
    {/if}
  {/if}
</label>
{#if field.hint}<p id={hintId} class="muted">{t(field.hint)}</p>{/if}

<style>
  label {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
  }
  input,
  select {
    width: 100%;
    min-width: 0;
    min-height: 44px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .check input {
    width: auto;
    flex: none;
  }
  p {
    margin: 0.25rem 0 0.65rem;
    max-width: 70ch;
  }
</style>
