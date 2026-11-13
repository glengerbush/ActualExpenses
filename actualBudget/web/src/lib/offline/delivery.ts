import { request, RequestError, errorMessage } from "../api";
import { mutate, type Transaction } from "../register-api";
import { t } from "../i18n.svelte";
import { observeVersions } from "../edit-clock";
import { entries, claimNext, settle, release } from "./outbox";
import { offlineStatus, setReachable, emit, subscribeOffline } from "./events";
export { subscribeOffline, offlineStatus } from "./events";
let running: Promise<void> | null = null;
let rerun = false;
let users = 0,
  timer: ReturnType<typeof setTimeout> | undefined;
export function deliverPending(): Promise<void> {
  if (running) {
    rerun = true;
    return running;
  }
  running = drain().finally(() => {
    running = null;
    if (rerun) {
      rerun = false;
      queueMicrotask(() => void deliverPending());
    }
  });
  return running;
}
async function drain() {
  if (!offlineStatus.reachable) return;
  try {
    if (!(await entries()).some((e) => !e.blocked)) return;
    const { data: info } = await request<{
      data: { api_version: number; capabilities?: string[] };
    }>("/info");
    if (info.api_version !== 1) {
      offlineStatus.compatible = false;
      offlineStatus.error =
        "This server API is incompatible. Saved work remains on this device.";
      emit(false);
      return;
    }
    offlineStatus.compatible = true;
    for (;;) {
      const entry = await claimNext();
      if (!entry) break;
      try {
        const response = await mutate<Transaction>(
          structuredClone(entry.pending),
          entry.cancelRequested,
        );
        observeVersions(response.data.field_versions);
        await settle(entry, {
          version: 1,
          budgetId: entry.budgetId,
          key: entry.key,
          at: new Date().toISOString(),
          row: response.data,
          message:
            response.data.deleted && entry.pending.method !== "DELETE"
              ? t(
                  "This transaction remains deleted. Its newer field edits were retained without restoring it.",
                )
              : response.conflicts?.length
                ? t("A newer edit replaced these fields:") +
                  " " +
                  response.conflicts
                    .map((c) =>
                      t(
                        (
                          {
                            account_id: "Account",
                            amount: "Amount",
                            date: "Date",
                            cleared: "Cleared",
                            notes: "Notes",
                            payee_id: "Payee",
                            category_id: "Category",
                          } as Record<string, string>
                        )[c.field] ?? "Transaction details",
                      ),
                    )
                    .join(", ")
                : entry.cancelRequested
                  ? "This change had already been saved. Its confirmed result was recovered."
                  : entry.pending.method === "DELETE"
                    ? "Transaction deleted."
                    : "",
        });
        setReachable(true);
      } catch (error) {
        if (
          error instanceof RequestError &&
          !error.uncertain &&
          [
            "operation_canceled",
            "deletion_superseded",
            "transaction_deleted",
          ].includes(error.code ?? "")
        ) {
          await settle(entry, {
            version: 1,
            budgetId: entry.budgetId,
            key: entry.key,
            at: new Date().toISOString(),
            message: error.message,
          });
          continue;
        }
        if (error instanceof RequestError && !error.uncertain && error.code) {
          await release(entry, error.message);
          continue;
        }
        await release(entry);
        setReachable(false, errorMessage(error));
        break;
      }
    }
  } catch (error) {
    offlineStatus.error = errorMessage(error);
    emit(false);
  }
  // A lease left by a crashed tab may still exist. Only actual pending work schedules a retry.
  const pending = await entries().catch(() => []);
  const until = Math.min(
    ...pending.filter((e) => e.claim && !e.blocked).map((e) => e.claim!.until),
  );
  if (users && Number.isFinite(until)) {
    clearTimeout(timer);
    timer = setTimeout(
      () => void deliverPending(),
      Math.max(100, until - Date.now() + 20),
    );
  }
}
export function startDelivery(): () => void {
  users++;
  const online = () => {
    setReachable(true);
    void deliverPending();
  };
  const offline = () => setReachable(false);
  const focus = () => {
    if (document.visibilityState === "visible" && navigator.onLine) online();
  };
  window.addEventListener("online", online);
  window.addEventListener("offline", offline);
  document.addEventListener("visibilitychange", focus);
  let scheduled = false;
  const stop = subscribeOffline(() => {
    if (!scheduled && offlineStatus.reachable) {
      scheduled = true;
      queueMicrotask(() => {
        scheduled = false;
        void deliverPending();
      });
    }
  });
  void deliverPending();
  return () => {
    stop();
    window.removeEventListener("online", online);
    window.removeEventListener("offline", offline);
    document.removeEventListener("visibilitychange", focus);
    if (--users === 0) clearTimeout(timer);
  };
}
