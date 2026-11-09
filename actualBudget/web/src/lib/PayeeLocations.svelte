<script lang="ts">
  import { onMount } from "svelte";
  import { request, uuid, errorMessage, RequestError } from "./api";
  import {
    nextEditClock,
    observeVersions,
    LocalClockError,
    resetCorrectedClock,
  } from "./edit-clock";
  import { offlineStatus, subscribeOffline } from "./offline/delivery";
  import {
    distance,
    nearby,
    readLocationWork,
    saveLocationWork,
    locationCancelKey,
    type SavedLocation,
    type LocationPayee,
    type LocationWork,
    type Position,
  } from "./payee-locations";
  let {
    budgetId,
    payeeId,
    newPayee,
    disabled = false,
    onselect,
    oncreated,
    onpendingpayee,
  }: {
    budgetId: string;
    payeeId: string;
    newPayee: { id: string; name: string };
    disabled?: boolean;
    onselect: (payee: LocationPayee) => void;
    oncreated: (payee: LocationPayee) => void;
    onpendingpayee: (id: string) => void;
  } = $props();
  let locations = $state.raw<SavedLocation[]>([]),
    payees = $state.raw<LocationPayee[]>([]),
    point = $state.raw<Position | null>(null),
    work = $state.raw<LocationWork | null>(null);
  let busy = $state(false),
    online = $state(true),
    error = $state(""),
    notice = $state(""),
    clockRecovery = $state(false),
    loaded = $state(false),
    secure = $state(true),
    supported = $state(true);
  let capturingPayee = "";
  function reportPending() {
    onpendingpayee(capturingPayee || work?.pending?.payee?.id || "");
  }
  let epoch = "",
    active = true;
  const suggestions = $derived(point ? nearby(point, locations, payees) : []);
  const chosen = $derived(payees.find((p) => p.id === payeeId));
  const hasNearby = $derived(
    !!point &&
      locations.some(
        (l) =>
          !l.deleted && l.payee_id === payeeId && distance(point!, l) <= 500,
      ),
  );
  const locked = $derived(disabled || busy || !!work?.pending || !online);
  function recover() {
    try {
      work = readLocationWork(budgetId);
      reportPending();
      error = "";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  async function snapshot() {
    const checkpoint = await request<{
      data: { epoch: string; snapshot: string };
    }>(`/budgets/${budgetId}/checkpoint`);
    async function pages<T>(path: string): Promise<T[]> {
      const rows: T[] = [];
      let offset: number | null = 0;
      do {
        const page: { data: T[]; next_offset: number | null } = await request(
          `/budgets/${budgetId}/${path}?limit=1000&offset=${offset}&snapshot=${checkpoint.data.snapshot}`,
        );
        rows.push(...page.data);
        offset = page.next_offset;
      } while (offset !== null);
      return rows;
    }
    const [saved, choices] = await Promise.all([
      pages<SavedLocation>("payee-locations"),
      pages<LocationPayee>("payees"),
    ]);
    if (!active) return;
    locations = saved;
    payees = choices;
    epoch = checkpoint.data.epoch;
    loaded = true;
    for (const row of saved) observeVersions(row.field_versions);
  }
  function locate(): Promise<Position> {
    if (!window.isSecureContext)
      throw new Error(
        "Location needs HTTPS or localhost. You can still enter and save expenses over HTTP.",
      );
    if (!navigator.geolocation)
      throw new Error(
        "This browser does not support location. Choose a payee normally.",
      );
    return new Promise((resolve, reject) =>
      navigator.geolocation.getCurrentPosition(
        (p) =>
          resolve({
            latitude: p.coords.latitude,
            longitude: p.coords.longitude,
          }),
        (e) =>
          reject(
            new Error(
              e.code === 1
                ? "Location permission denied. Choose a payee normally, or allow location in site settings and retry."
                : e.code === 3
                  ? "Location request timed out. Retry or choose a payee normally."
                  : "Location unavailable. Retry or choose a payee normally.",
            ),
          ),
        { enableHighAccuracy: true, timeout: 10000, maximumAge: 0 },
      ),
    );
  }
  async function requestPosition() {
    if (locked) return;
    busy = true;
    error = "";
    notice = "";
    try {
      const position = await locate();
      if (!active) return;
      await snapshot();
      if (!active) return;
      point = position;
      notice = suggestions.length
        ? "Nearby payees updated."
        : "No saved payees within 500 m.";
    } catch (e) {
      if (active) error = errorMessage(e);
    } finally {
      if (active) busy = false;
    }
  }
  async function prepare(
    body: Record<string, unknown>,
    id?: string,
    payee?: LocationPayee,
  ) {
    if (!navigator.onLine || !offlineStatus.reachable)
      throw new Error(
        "Reconnect to save or forget locations. Expense entry remains available.",
      );
    work = saveLocationWork(
      {
        version: 1,
        budget: budgetId,
        revision: "",
        pending: {
          path: `/budgets/${budgetId}/payee-locations${id ? `/${id}` : ""}`,
          method: id ? "PATCH" : "POST",
          key: uuid(),
          clock: nextEditClock(),
          epoch,
          body,
          payee,
        },
      },
      work,
    );
    reportPending();
    clockRecovery = false;
  }
  async function saveHere() {
    if (locked) return;
    const selected = payeeId === "new" ? { ...newPayee } : chosen;
    if (!selected?.name.trim()) {
      error = "Choose or name a payee first.";
      return;
    }
    const creating = payeeId === "new";
    if (creating) {
      capturingPayee = selected.id;
      reportPending();
    }
    busy = true;
    error = "";
    notice = "";
    try {
      const position = await locate();
      if (!active) return;
      await snapshot();
      if (!active) return;
      point = position;
      if (
        locations.some(
          (l) =>
            l.payee_id === selected.id &&
            !l.deleted &&
            distance(position, l) <= 500,
        )
      ) {
        notice = "This payee already has a saved location within 500 m.";
        return;
      }
      await prepare(
        {
          id: uuid(),
          ...position,
          ...(creating ? { new_payee: selected } : { payee_id: selected.id }),
        },
        undefined,
        creating ? selected : undefined,
      );
      busy = false;
      await send();
    } catch (e) {
      if (active) {
        clockRecovery = e instanceof LocalClockError;
        error = errorMessage(e);
      }
    } finally {
      if (active) {
        busy = false;
        capturingPayee = "";
        reportPending();
      }
    }
  }
  async function forget(id: string) {
    if (locked) return;
    busy = true;
    error = "";
    notice = "";
    try {
      await snapshot();
      if (!active) return;
      await prepare({ deleted: true }, id);
      busy = false;
      await send();
    } catch (e) {
      if (active) {
        clockRecovery = e instanceof LocalClockError;
        error = errorMessage(e);
      }
    } finally {
      if (active) busy = false;
    }
  }
  async function send() {
    if (busy || !work?.pending || !online) return;
    const intent = work.pending;
    busy = true;
    error = "";
    try {
      // Revision check prevents an old tab from clearing or replacing newer work.
      work = saveLocationWork({ ...work }, work);
      const response = await request<{ conflicts?: unknown[] }>(
        intent.path,
        intent.body,
        intent.key,
        intent.method,
        intent.clock,
        localStorage.getItem(locationCancelKey(intent)) === "true",
        intent.epoch,
      );
      if (!active) return;
      if (intent.payee)
        oncreated({ ...intent.payee, name: intent.payee.name.trim() });
      work = saveLocationWork({ ...work, pending: null }, work);
      reportPending();
      localStorage.removeItem(locationCancelKey(intent));
      notice = response.conflicts?.length
        ? "A newer location edit was retained."
        : intent.method === "POST"
          ? "Location saved in your budget."
          : "Location forgotten.";
      await snapshot();
    } catch (e) {
      if (!active) return;
      if (
        e instanceof RequestError &&
        !e.uncertain &&
        e.code === "operation_canceled"
      ) {
        try {
          work = saveLocationWork({ ...work!, pending: null }, work);
          reportPending();
          localStorage.removeItem(locationCancelKey(intent));
          notice = "Pending location change canceled.";
          await snapshot();
        } catch (failure) {
          error = errorMessage(failure);
        }
      } else error = errorMessage(e);
    } finally {
      if (active) busy = false;
    }
  }
  async function cancelPending() {
    if (busy || !work?.pending || !online) return;
    const original = work.pending;
    busy = true;
    error = "";
    try {
      work = saveLocationWork({ ...work }, work);
      const info = await request<{ data: { capabilities: string[] } }>("/info");
      if (!active) return;
      if (!info.data.capabilities.includes("location_cancellation"))
        throw new Error(
          "This server cannot safely cancel location changes. Retry unchanged.",
        );
      work = saveLocationWork({ ...work }, work);
      if (work.pending !== original)
        throw new Error("Location work changed. Reload saved location work.");
      localStorage.setItem(locationCancelKey(original), "true");
      busy = false;
      await send();
    } catch (e) {
      if (active) error = errorMessage(e);
    } finally {
      if (active) busy = false;
    }
  }
  function correctedTime() {
    if (locked) return;
    try {
      if (work) work = saveLocationWork({ ...work }, work);
      resetCorrectedClock();
      for (const row of locations) observeVersions(row.field_versions);
      clockRecovery = false;
      error = "";
      notice = "Device time adopted. Retry saving the location.";
    } catch (e) {
      error = errorMessage(e);
    }
  }
  onMount(() => {
    active = true;
    secure = window.isSecureContext;
    supported = !!navigator.geolocation;
    online = offlineStatus.reachable;
    recover();
    const unsubscribe = subscribeOffline(
      () => (online = offlineStatus.reachable),
    );
    return () => {
      active = false;
      unsubscribe();
    };
  });
</script>

<div
  class={[
    "payee-locations",
    { recovery: busy || !!work?.pending || !!error || !!notice },
  ]}
  aria-label="Payee locations"
  aria-busy={busy}
>
  <p class="hint">
    Optional: find payees within 500 m. Your position stays on this device until
    you tap Save location. Saved locations sync with your budget; no third-party
    service.
  </p>
  {#if !secure}<p class="hint">
      Location needs HTTPS or localhost. Expense entry works over HTTP.
    </p>{:else if !supported}<p class="hint">
      Location unavailable in this browser. Choose a payee normally.
    </p>{/if}
  {#if !online}<p role="status">
      Reconnect to use payee locations. You can still enter expenses.
    </p>{/if}
  <div class="location-actions">
    <button
      type="button"
      onclick={requestPosition}
      disabled={locked || !secure || !supported}
      >{point ? "Refresh nearby payees" : "Request location"}</button
    >
    {#if suggestions.length}<button
        type="button"
        onclick={() => onselect(suggestions[0].payee)}
        disabled={locked}>Use nearest payee</button
      >{/if}
    {#if (chosen || payeeId === "new") && !hasNearby}<button
        type="button"
        onclick={saveHere}
        disabled={locked || !secure || !supported}>Save location</button
      >{/if}
  </div>
  {#if busy}<p role="status">Locating or saving…</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>
    <button type="button" onclick={recover} disabled={busy}
      >Reload saved location work</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if clockRecovery && !work?.pending}<button
      type="button"
      onclick={correctedTime}
      disabled={locked}>Use corrected device time</button
    >{/if}
  {#if work?.pending}<div class="pending-location">
      <p>
        Location change unconfirmed. Retry keeps the original request; Cancel
        checks whether it was already saved.
      </p>
      <div class="location-actions">
        <button type="button" onclick={send} disabled={busy || !online}
          >Retry location change</button
        ><button
          type="button"
          onclick={cancelPending}
          disabled={busy || !online}>Cancel location change</button
        >
      </div>
    </div>{/if}
  {#if point && loaded && suggestions.length}<h3>Nearby payees</h3>
    <ul>
      {#each suggestions as item (item.payee.id)}<li>
          <button
            type="button"
            class="choose-payee"
            onclick={() => onselect(item.payee)}
            disabled={locked}
            >{item.payee.name}<small>{Math.round(item.distance)} m away</small
            ></button
          ><button
            type="button"
            onclick={() => forget(item.location.id)}
            aria-label={`Forget nearby location for ${item.payee.name}`}
            disabled={locked}>Forget</button
          >
        </li>{/each}
    </ul>{/if}
</div>

<style>
  .payee-locations {
    display: none;
    margin-block: 0.75rem;
  }
  .payee-locations.recovery {
    display: block;
  }
  @media (max-width: 700px) {
    .payee-locations {
      display: block;
    }
  }
  .location-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  h3 {
    font-size: 1rem;
    margin-block: 1.25rem 0.5rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    border-bottom: 1px solid var(--line);
    padding-block: 0.5rem;
  }
  .choose-payee {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow-wrap: anywhere;
  }
  small {
    display: block;
  }
  .pending-location {
    border-block: 1px solid var(--line);
    padding-block: 0.75rem;
  }
</style>
