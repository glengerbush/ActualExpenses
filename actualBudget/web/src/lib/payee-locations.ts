import { uuid } from "./api";
export interface SavedLocation {
  id: string;
  payee_id: string;
  latitude: number;
  longitude: number;
  created_at: number;
  deleted: boolean;
  field_versions: Record<string, string>;
}
export interface LocationPayee {
  id: string;
  name: string;
  deleted?: boolean;
  transfer_account_id?: string | null;
}
export interface Position {
  latitude: number;
  longitude: number;
}
export function distance(a: Position, b: Position): number {
  const lat = (a.latitude * Math.PI) / 180,
    other = (b.latitude * Math.PI) / 180;
  return (
    6371000 *
    Math.acos(
      Math.min(
        1,
        Math.max(
          -1,
          Math.cos(lat) *
            Math.cos(other) *
            Math.cos(((b.longitude - a.longitude) * Math.PI) / 180) +
            Math.sin(lat) * Math.sin(other),
        ),
      ),
    )
  );
}
export function nearby(
  point: Position,
  locations: SavedLocation[],
  payees: LocationPayee[],
) {
  const active = new Map(
    payees
      .filter((p) => !p.deleted && !p.transfer_account_id)
      .map((p) => [p.id, p]),
  );
  const nearest = new Map<
    string,
    { payee: LocationPayee; location: SavedLocation; distance: number }
  >();
  for (const location of locations) {
    const payee = active.get(location.payee_id);
    if (!payee || location.deleted) continue;
    const meters = distance(point, location),
      previous = nearest.get(payee.id);
    if (meters <= 500 && (!previous || meters < previous.distance))
      nearest.set(payee.id, { payee, location, distance: meters });
  }
  return [...nearest.values()]
    .sort(
      (a, b) => a.distance - b.distance || a.payee.id.localeCompare(b.payee.id),
    )
    .slice(0, 10);
}
export interface LocationIntent {
  path: string;
  method: "POST" | "PATCH";
  body: Record<string, unknown>;
  key: string;
  clock: string;
  epoch: string;
  payee?: LocationPayee;
}
export interface LocationWork {
  version: 1;
  budget: string;
  revision: string;
  pending: LocationIntent | null;
}
export const locationWorkKey = (budget: string) =>
  `actualBudget.locations.v1.${budget}`;
export const locationCancelKey = (intent: LocationIntent) =>
  `actualBudget.locations.cancel.${intent.key}`;
export function readLocationWork(budget: string): LocationWork | null {
  const raw = localStorage.getItem(locationWorkKey(budget));
  if (!raw) return null;
  const work = JSON.parse(raw) as LocationWork,
    p = work?.pending;
  if (
    work?.version !== 1 ||
    work.budget !== budget ||
    typeof work.revision !== "string" ||
    (p &&
      (!p.path.startsWith(`/budgets/${budget}/payee-locations`) ||
        !["POST", "PATCH"].includes(p.method) ||
        typeof p.key !== "string" ||
        typeof p.epoch !== "string" ||
        typeof p.clock !== "string" ||
        !p.body ||
        typeof p.body !== "object"))
  )
    throw new Error(
      "Saved location work is unreadable. Keep browser data for recovery.",
    );
  return work;
}
export function saveLocationWork(
  work: LocationWork,
  previous: LocationWork | null,
): LocationWork {
  if (
    (readLocationWork(work.budget)?.revision ?? null) !==
    (previous?.revision ?? null)
  )
    throw new Error(
      "Another tab changed location work. Reload saved location work first.",
    );
  const next = { ...work, revision: uuid() };
  localStorage.setItem(locationWorkKey(work.budget), JSON.stringify(next));
  return next;
}
