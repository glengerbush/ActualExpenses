import { errorMessage, request, uuid } from "./api";
import { transaction, result, OfflineStateError } from "./offline/storage";
import { setReachable, offlineStatus } from "./offline/events";
export interface RecoveryPreview {
  epoch: string;
  snapshot: string;
  preview_token: string;
  name: string;
  accounts: {
    id: string;
    name: string;
    before: { total: number; cleared: number; uncleared: number };
    after: { total: number; cleared: number; uncleared: number };
  }[];
  repairs: {
    id: string;
    field: string;
    before: unknown;
    after: unknown;
    reason: string;
  }[];
  issues: { domain: string; id: string; message: string; blocking: boolean }[];
  can_apply: boolean;
  backup_restorable: boolean;
}
export interface RepairOperation {
  version: 1;
  origin: string;
  budgetId: string;
  key: string;
  epoch: string;
  body: { preview_token: string; snapshot: string; confirm_name: string };
}
export interface RecoveryResult {
  backup: { id: string; label: string };
  repaired_fields: number;
  recalculated_accounts: number;
  epoch: string;
  snapshot: string;
  edit_clock: string;
}
const repairKey = (budget: string) => `recovery-operation:${budget}`;
function validate(
  value: unknown,
  budget: string,
): asserts value is RepairOperation {
  const v = value as RepairOperation;
  if (
    !v ||
    v.version !== 1 ||
    v.origin !== location.origin ||
    v.budgetId !== budget ||
    !/^[a-f0-9]{32}$/.test(v.epoch) ||
    !/^[a-f0-9-]{36}$/.test(v.key) ||
    !/^[a-f0-9]{64}$/.test(v.body?.preview_token) ||
    !/^c1[a-f0-9]{48}$/.test(v.body.snapshot) ||
    typeof v.body.confirm_name !== "string"
  )
    throw new OfflineStateError(
      "Saved repair is unreadable. Keep this browser's data and recover the original request; no repair was sent.",
    );
}
export async function readRepair(
  budget: string,
): Promise<RepairOperation | null> {
  return transaction(["cache"], "readonly", async (tx) => {
    const value = await result(tx.objectStore("cache").get(repairKey(budget)));
    if (!value) return null;
    validate(value, budget);
    return value;
  });
}
export function prepareRepair(
  budget: string,
  preview: RecoveryPreview,
): RepairOperation {
  return {
    version: 1,
    origin: location.origin,
    budgetId: budget,
    key: uuid(),
    epoch: preview.epoch,
    body: {
      preview_token: preview.preview_token,
      snapshot: preview.snapshot,
      confirm_name: preview.name,
    },
  };
}
export async function saveRepair(operation: RepairOperation) {
  validate(operation, operation.budgetId);
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      key = repairKey(operation.budgetId),
      prior = await result(store.get(key));
    if (prior && JSON.stringify(prior) !== JSON.stringify(operation))
      throw new OfflineStateError(
        "Another repair is saved in this browser. Recover it before starting a new one.",
      );
    await result(store.put(operation, key));
  });
}
export async function completeRepair(operation: RepairOperation) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      key = repairKey(operation.budgetId),
      prior = await result(store.get(key));
    if (prior && JSON.stringify(prior) !== JSON.stringify(operation))
      throw new OfflineStateError(
        "Saved repair changed in another tab. Reload before clearing it.",
      );
    await result(store.delete(key));
  });
}
export async function sendRepair(
  operation: RepairOperation,
  cancel = false,
): Promise<RecoveryResult> {
  validate(operation, operation.budgetId);
  const { data } = await request<{ data: RecoveryResult }>(
    `/budgets/${operation.budgetId}/recovery`,
    operation.body,
    operation.key,
    "POST",
    undefined,
    cancel,
    operation.epoch,
  );
  return data;
}
/** Explicit allowlist: the 'cache' store ALSO holds irreplaceable lifecycle work.
 * Never clear it wholesale, nor touch operations/receipts/other databases/drafts. */
export async function resetDownloadedData(): Promise<void> {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      keys = await result(store.getAllKeys());
    for (const key of keys)
      if (
        typeof key === "string" &&
        (key === "catalog" ||
          key.startsWith("budget:") ||
          key.startsWith("view:"))
      )
        await result(store.delete(key));
  });
  try {
    for (const key of Object.keys(localStorage))
      if (key.startsWith("actualBudget.preferencesCache.v1."))
        localStorage.removeItem(key);
  } catch (error) {
    throw new OfflineStateError(
      `Downloaded budget views were reset, but a display-preference cache could not be removed. Pending work is intact. ${errorMessage(error)}`,
    );
  }
}
export async function checkConnection() {
  const { data } = await request<{ data: { api_version: number } }>("/info");
  if (data.api_version !== 1)
    throw new Error(
      "Update this app and server together before reconnecting to this API version.",
    );
  offlineStatus.compatible = true;
  setReachable(true);
}
