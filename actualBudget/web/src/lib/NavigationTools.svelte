<script lang="ts">
  import { t } from "./i18n.svelte";
  import { tick } from "svelte";
  import { preferences } from "./preferences.svelte";
  import { errorMessage, uuid } from "./api";
  import type { NavigationCommand } from "./navigation";
  let {
    commands,
    budgetId,
    offline,
    busy,
  }: {
    commands: NavigationCommand[];
    budgetId?: string;
    offline: boolean;
    busy: boolean;
  } = $props();
  const release = "navigation-45";
  let dialog: HTMLDialogElement;
  let search = $state<HTMLInputElement>();
  let mode = $state<"search" | "help">("search");
  let query = $state("");
  let active = $state(0);
  let saving = $state(false);
  let error = $state("");
  let trigger: HTMLElement | null = null;
  let matches = $derived(
    commands.filter((c) =>
      `${c.label} ${c.keywords ?? ""}`
        .toLocaleLowerCase()
        .includes(query.toLocaleLowerCase().trim()),
    ),
  );
  async function open(value: "search" | "help") {
    if (dialog?.open) return;
    trigger =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    mode = value;
    query = "";
    active = 0;
    dialog.showModal();
    await tick();
    if (value === "search") search?.focus();
  }
  function close() {
    dialog.close();
    trigger?.focus();
  }
  async function run(command: NavigationCommand | undefined) {
    if (!command || command.disabled) return;
    close();
    try {
      await command.run();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  function keyboard(event: KeyboardEvent) {
    if (event.repeat || event.isComposing) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      void open("search");
    }
    const target = event.target;
    const editing =
      target instanceof HTMLElement &&
      (target.isContentEditable ||
        ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
    if (
      !editing &&
      !dialog?.open &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      event.key === "?"
    ) {
      event.preventDefault();
      void open("help");
    }
  }
  function searchKeys(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      active = matches.length
        ? (active + (event.key === "ArrowDown" ? 1 : -1) + matches.length) %
          matches.length
        : 0;
      document
        .getElementById(`navigation-command-${active}`)
        ?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      void run(matches[active]);
    }
  }
  async function notices(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    await preferences.updateDevice({ show_release_notices: input.checked });
    input.checked = !!preferences.device.show_release_notices;
  }
  async function privacy() {
    if (!budgetId || saving || offline) return;
    saving = true;
    error = "";
    try {
      await preferences.saveShared(
        budgetId,
        { privacy_mode: !preferences.shared.privacy_mode },
        uuid(),
        preferences.budgetEpoch,
      );
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={keyboard} />
<div class="navigation-tools" role="group" aria-label="Navigation tools">
  <button onclick={() => open("search")} aria-keyshortcuts="Control+k Meta+k"
    >Search commands</button
  >
  <button
    onclick={privacy}
    aria-pressed={!!preferences.shared.privacy_mode}
    title={offline
      ? "Reconnect to change the shared privacy preference"
      : "Mask displayed amounts"}
    disabled={!budgetId ||
      !preferences.hasUsableShared ||
      offline ||
      busy ||
      saving}>Privacy mode</button
  >
  <button onclick={() => open("help")} aria-keyshortcuts="?"
    >Help and releases</button
  >
</div>
{#if error}<p class="error" role="alert">
    {error} Retry the action when connected.
  </p>{/if}
{#if preferences.device.show_release_notices && preferences.device.seen_release !== release}
  <div class="release-notice" role="status">
    <p>
      actualBudget navigation: command search, privacy masking and local help
      are available in this fork.
    </p>
    <button onclick={() => preferences.updateDevice({ seen_release: release })}
      >Dismiss release notice</button
    >
  </div>
{/if}
<dialog
  bind:this={dialog}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  aria-labelledby="navigation-title"
>
  {#if mode === "search"}
    <div class="page-heading">
      <h2 id="navigation-title">Go to…</h2>
      <button onclick={close}>{t("Close search")}</button>
    </div>
    <label for="command-query">Search commands</label>
    <input
      bind:this={search}
      id="command-query"
      type="search"
      value={query}
      oninput={(event) => {
        query = event.currentTarget.value;
        active = 0;
      }}
      onkeydown={searchKeys}
      aria-describedby="command-help"
      aria-controls="command-results"
    />
    <p class="hint" id="command-help">
      Arrow keys choose; Enter opens; Escape closes. Finish or close the current
      editor to navigate.
    </p>
    <p class="visually-hidden" role="status">
      {matches[active]?.label ?? "No matching commands"}{matches[active]
        ?.disabled
        ? " — unavailable in this view"
        : ""}
    </p>
    <ul id="command-results" aria-label="Matching commands">
      {#each matches as command, index (command.id)}
        <li>
          <button
            id={`navigation-command-${index}`}
            class={{ chosen: index === active }}
            disabled={command.disabled}
            onclick={() => run(command)}
            >{command.label}{command.disabled
              ? " — unavailable in this view"
              : ""}</button
          >
        </li>
      {:else}<li>No matching commands.</li>{/each}
    </ul>
  {:else}
    <div class="page-heading">
      <h2 id="navigation-title">actualBudget help</h2>
      <button onclick={close}>Close help</button>
    </div>
    <h3>{t("Keyboard shortcuts")}</h3>
    <dl>
      <dt>Ctrl / Cmd + K</dt>
      <dd>Search delivered views and accounts.</dd>
      <dt>?</dt>
      <dd>Open help outside a text field.</dd>
      <dt>Escape</dt>
      <dd>Close command search or help.</dd>
      <dt>Tab / Shift + Tab</dt>
      <dd>Move between controls. Focus stays inside this dialog.</dd>
    </dl>
    <h3>Privacy and saved views</h3>
    <p>
      Privacy mode masks displayed amounts on every screen size. On desktop,
      amount fields are masked at rest and reveal on hover or focus. Names,
      dates, notes and exports remain visible. This display preference does not
      secure access to your budget.
    </p>
    <p>
      The last budget, account and supported view are remembered on this
      browser. Unfinished entries keep their existing recovery prompts.
    </p>
    <h3>Connection and access</h3>
    <p>
      Use your server over plain HTTP on a trusted LAN or tailnet. No
      application login. Disconnected use supports cached views and pending
      expense capture; full offline budgeting is unavailable.
    </p>
    <h3>Fork releases</h3>
    <p>
      This is actualBudget, a separate application using Actual 26.10.0 as its
      compatibility reference. Local announcements describe features in this
      application; they do not claim an upstream update is available.
    </p>
    <p>No external update checks or analytics.</p>
    <label class="notice-choice"
      ><input
        type="checkbox"
        checked={!!preferences.device.show_release_notices}
        onchange={notices}
      />Show local release notices on this device</label
    >
    <p class="hint">
      Off by default. Dismissed announcements stay dismissed on this browser.
      Help remains available when notices are off.
    </p>
    {#if preferences.deviceError}<p class="error" role="alert">
        {preferences.deviceError}
      </p>{/if}
  {/if}
</dialog>

<style>
  .navigation-tools {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  dialog {
    width: min(42rem, calc(100% - 2rem));
    max-height: calc(100dvh - 2rem);
    overflow: auto;
    padding: 1.5rem;
    border: 1px solid var(--line);
    border-radius: 0.5rem;
    color: var(--text);
    background: var(--surface);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 45%);
  }
  ul {
    list-style: none;
    padding: 0;
    max-height: 45dvh;
    overflow: auto;
  }
  li button {
    width: 100%;
    text-align: left;
    margin-bottom: 0.25rem;
  }
  .chosen {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  input[type="search"] {
    width: 100%;
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0 0 0.75rem;
  }
  .notice-choice {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  .release-notice {
    margin-bottom: 1rem;
  }
  h3 {
    margin-top: 1.5rem;
  }
  p {
    max-width: 70ch;
  }
  @media (max-width: 700px) {
    dialog {
      padding: 1rem;
    }
  }
</style>
