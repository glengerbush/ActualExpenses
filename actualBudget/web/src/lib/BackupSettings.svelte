<script lang="ts">
  import { onMount } from "svelte";
  import { request, uuid, errorMessage } from "./api";
  import Notifications from "./Notifications.svelte";
  let interval = $state<number | null>(null);
  let busy = $state(false),
    error = $state(""),
    notice = $state("");
  let pending = $state<{ interval_seconds: number; key: string } | null>(null);
  async function load() {
    try {
      interval = (
        await request<{ data: { interval_seconds: number } }>("/backup-policy")
      ).data.interval_seconds;
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function save(seconds: number) {
    busy = true;
    error = "";
    pending ??= { interval_seconds: seconds, key: uuid() };
    try {
      const result = await request<{ data: { interval_seconds: number } }>(
        "/backup-policy",
        { interval_seconds: pending.interval_seconds },
        pending.key,
        "PATCH",
      );
      interval = result.data.interval_seconds;
      pending = null;
      notice = "Backup schedule saved.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void load();
  });
</script>

<section aria-labelledby="backup-settings-title">
  <h2 id="backup-settings-title">Automatic backups</h2>
  <label for="backup-schedule">Automatic backup schedule</label>
  <select
    id="backup-schedule"
    value={interval ?? 604800}
    disabled={busy || interval === null || !!pending}
    onchange={(event) => void save(Number(event.currentTarget.value))}
  >
    <option value={604800}>Weekly, only when data changed</option>
    <option value={0}>Never</option>
    {#if interval !== null && ![0, 604800].includes(interval)}<option
        value={interval}>Server interval ({interval} seconds)</option
      >{/if}
  </select>
  <p class="hint">
    Each backup is a complete budget snapshot. The latest 10 automatic backups
    are kept on the server. Weekly backups run seven days after the first change
    since the last backup.
  </p>
  {#if pending || error}<button
      onclick={() =>
        pending ? void save(pending.interval_seconds) : void load()}
      disabled={busy}>Retry backup settings</button
    >{/if}
  <Notifications bind:notice bind:error />
</section>

<style>
  select {
    max-width: 24rem;
  }
  p {
    max-width: 65ch;
    margin-top: 0.75rem;
  }
</style>
