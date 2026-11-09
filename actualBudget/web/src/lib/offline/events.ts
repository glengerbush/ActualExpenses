export interface OfflineStatus {
  reachable: boolean;
  compatible: boolean;
  error: string;
}
const listeners = new Set<() => void>();
export const offlineStatus: OfflineStatus = {
  reachable: navigator.onLine,
  compatible: true,
  error: "",
};
const channel =
  typeof BroadcastChannel === "undefined"
    ? null
    : new BroadcastChannel("actualBudget.offline.v1");
channel?.addEventListener("message", () => emit(false));
export function emit(broadcast = true) {
  for (const listener of listeners) listener();
  if (broadcast) channel?.postMessage("changed");
}
export function subscribeOffline(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
export function setReachable(reachable: boolean, error = "") {
  offlineStatus.reachable = reachable;
  offlineStatus.error = error;
  emit(false);
}
