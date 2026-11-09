import { uuid } from "./api";
export class LocalClockError extends Error {}
const zero = "1970-01-01T00:00:00.000Z-0000-0000000000000000";
const storageKey = "actualBudget.clock.v1";
const node = uuid().replaceAll("-", "").slice(-16);
let observed = zero;
export function validClock(value: unknown): value is string {
  return (
    typeof value === "string" &&
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z-[0-9A-F]{4}-[0-9a-fA-F]{16}$/.test(
      value,
    ) &&
    Number.isFinite(Date.parse(value.slice(0, 24))) &&
    Date.parse(value.slice(0, 24)) >= 0 &&
    new Date(value.slice(0, 24)).toISOString() === value.slice(0, 24)
  );
}
export function observeVersions(versions?: Record<string, string>) {
  for (const clock of Object.values(versions ?? {})) {
    if (!validClock(clock))
      throw new Error("The server returned an invalid edit clock.");
    if (clock > observed) observed = clock;
  }
}
/** Called once when preparing an intent; the saved pending operation owns it thereafter. */
export function nextEditClock(versions?: Record<string, string>): string {
  observeVersions(versions);
  const saved = localStorage.getItem(storageKey);
  if (saved !== null) {
    if (!validClock(saved))
      throw new Error(
        "The saved edit clock is unreadable. Recover saved work before making changes.",
      );
    if (saved > observed) observed = saved;
  }
  const physical = Date.now(),
    last = Date.parse(observed.slice(0, 24)),
    millis = Math.max(physical, last);
  if (millis - physical > 300_000)
    throw new LocalClockError(
      "A device clock is more than five minutes ahead. Correct the clock and retry.",
    );
  const counter =
    millis === last ? parseInt(observed.slice(25, 29), 16) + 1 : 0;
  if (counter > 65535)
    throw new LocalClockError(
      "Logical clock counter exhausted. Retry after time advances.",
    );
  const clock = `${new Date(millis).toISOString()}-${counter.toString(16).toUpperCase().padStart(4, "0")}-${node}`;
  localStorage.setItem(storageKey, clock);
  observed = clock;
  return clock;
}

/** Explicit recovery only: immutable pending operations keep their own clocks. */
export function resetCorrectedClock() {
  localStorage.removeItem(storageKey);
  observed = zero;
}
