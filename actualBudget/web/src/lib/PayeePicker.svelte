<script lang="ts">
  import type { Reference } from "./register-api";
  let {
    id,
    payees,
    value,
    newName = "",
    disabled = false,
    canCreate = true,
    onselect,
    oncreate,
    onsearch,
  }: {
    id: string;
    payees: Reference[];
    value: string;
    newName?: string;
    disabled?: boolean;
    canCreate?: boolean;
    onselect: (id: string) => void;
    oncreate: (name: string) => Promise<void>;
    onsearch: (searching: boolean) => void;
  } = $props();
  let search = $state<string | null>(null),
    open = $state(false),
    active = $state(-1);
  let input: HTMLInputElement;
  let menu = $state<HTMLDivElement>();
  let position = $state({ left: 0, top: 0, width: 0, height: 280 });
  const normalize = (text: string) =>
    text.trim().normalize("NFKD").replace(/\p{M}/gu, "").toLocaleLowerCase();
  const selectedName = $derived(
    value === "new"
      ? newName
      : value
        ? (payees.find((p) => p.id === value)?.name ?? newName)
        : "",
  );
  const matches = $derived(
    payees
      .filter(
        (p) =>
          !p.deleted && normalize(p.name).includes(normalize(search ?? "")),
      )
      .slice(0, 100),
  );
  const createName = $derived(search?.trim() ?? "");
  const creationDisabled = $derived(
    !canCreate ||
      !createName ||
      payees.some(
        (payee) =>
          !payee.deleted && normalize(payee.name) === normalize(createName),
      ),
  );
  const options = $derived([
    ...(!createName ? [{ id: "", name: "No payee" }] : []),
    ...matches,
    {
      id: "create",
      name: createName ? `Create payee “${createName}”` : "Create payee",
    },
  ]);
  function positionMenu() {
    const rect = input.getBoundingClientRect();
    const below = innerHeight - rect.bottom - 8;
    const height = Math.min(280, Math.max(below, rect.top - 8));
    position = {
      left: Math.max(
        8,
        Math.min(rect.left, innerWidth - Math.max(rect.width, 240) - 8),
      ),
      top:
        below >= Math.min(200, height)
          ? rect.bottom + 4
          : Math.max(8, rect.top - height - 4),
      width: Math.min(Math.max(rect.width, 240), innerWidth - 16),
      height,
    };
  }
  function show() {
    if (disabled) return;
    positionMenu();
    open = true;
    active = -1;
  }
  function validity() {
    input?.setCustomValidity(
      search?.trim() ? "Select a matching payee or create this payee." : "",
    );
  }
  function type(text: string) {
    search = text;
    onsearch(!!text.trim());
    if (!text.trim()) onselect("");
    validity();
    show();
  }
  async function choose(index: number) {
    const option = options[index];
    if (!option || disabled || (option.id === "create" && creationDisabled))
      return;
    if (option.id === "create") {
      await oncreate(createName);
      // A failed creation is retained by the parent and displayed with retry controls.
    } else onselect(option.id);
    search = null;
    onsearch(false);
    validity();
    open = false;
    input?.focus();
    open = false;
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) show();
      active =
        active < 0
          ? event.key === "ArrowDown"
            ? 0
            : options.length - 1
          : (active + (event.key === "ArrowDown" ? 1 : -1) + options.length) %
            options.length;
      document
        .getElementById(`${id}-option-${active}`)
        ?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter" && open) {
      event.preventDefault();
      void choose(Math.max(0, active));
    } else if (event.key === "Escape") {
      event.preventDefault();
      open = false;
      search = null;
      onsearch(false);
      validity();
    } else if (event.key === "Tab") open = false;
  }
</script>

<svelte:window
  onresize={() => {
    if (open) positionMenu();
  }}
/>
<svelte:document
  onscrollcapture={(event) => {
    if (open && event.target instanceof Node && !menu?.contains(event.target))
      positionMenu();
  }}
/>
<div class="payee-picker">
  <input
    {id}
    bind:this={input}
    role="combobox"
    aria-autocomplete="list"
    aria-expanded={open && !disabled}
    aria-controls={`${id}-options`}
    aria-activedescendant={open && !disabled && active >= 0
      ? `${id}-option-${active}`
      : undefined}
    autocomplete="off"
    maxlength="120"
    placeholder="Search payees"
    {disabled}
    value={search ?? selectedName}
    oninput={(e) => type(e.currentTarget.value)}
    onfocus={show}
    onclick={() => {
      if (!open) show();
    }}
    onblur={() => (open = false)}
    onkeydown={keydown}
  />
  <svg
    class="chevron"
    aria-hidden="true"
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.75"><path d="m6 9 6 6 6-6" /></svg
  >
  {#if open && !disabled}
    <div
      bind:this={menu}
      popover="manual"
      class="payee-menu"
      id={`${id}-options`}
      role="listbox"
      aria-label="Payees"
      style:left={`${position.left}px`}
      style:top={`${position.top}px`}
      style:width={`${position.width}px`}
      style:max-height={`${position.height}px`}
      {@attach (element) => {
        element.showPopover();
        return () => element.hidePopover();
      }}
    >
      {#each options as option, index (option.id)}
        <button
          type="button"
          role="option"
          tabindex="-1"
          id={`${id}-option-${index}`}
          aria-selected={index === active}
          disabled={option.id === "create" && creationDisabled}
          onpointerdown={(event) => event.preventDefault()}
          onclick={() => void choose(index)}>{option.name}</button
        >
      {/each}
      {#if createName && !canCreate}<p>Connect to create a payee.</p>{/if}
    </div>
  {/if}
</div>

<style>
  .payee-picker {
    position: relative;
    min-width: 0;
  }
  input {
    width: 100%;
    min-width: 0;
    padding-right: 2rem;
  }
  .chevron {
    position: absolute;
    right: 0.65rem;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
  }
  .payee-menu {
    position: fixed;
    inset: auto;
    margin: 0;
    padding: 0.25rem;
    box-sizing: border-box;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface, white);
    color: var(--text);
  }
  .payee-menu button {
    display: block;
    width: 100%;
    min-height: 44px;
    padding: 0.55rem 0.65rem;
    border: 0;
    text-align: left;
    font-weight: 400;
    background: transparent;
    overflow-wrap: anywhere;
  }
  .payee-menu button[aria-selected="true"],
  .payee-menu button:hover {
    background: var(--hover);
  }
  .payee-menu p {
    font-size: 0.875rem;
    padding: 0.5rem;
    margin: 0;
  }
</style>
