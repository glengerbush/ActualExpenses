import { request, RequestError, uuid, type Budget } from "./api";
import { transaction, result, OfflineStateError } from "./offline/storage";

export interface ManagedBudget extends Budget {
  deleted?: boolean;
  auto_due?: number | null;
  last_auto?: number | null;
  backup_error?: string | null;
}
export interface BackupPoint {
  id: string;
  budget_id: string;
  label: string;
  kind: string;
  created_at: string;
  bytes: number;
}
export interface RestoreStage {
  id: string;
  budget_id: string;
  name: string;
  status: string;
  report: { domains: Record<string, number>; receipt_count: number };
}
export type LifecycleKind =
  "renamed" | "duplicated" | "backup" | "deleted" | "restored";
export interface LifecycleOperation {
  version: 1;
  origin: string;
  key: string;
  budgetId: string;
  kind: LifecycleKind;
  path: string;
  method: "POST" | "PATCH" | "DELETE";
  body: Record<string, unknown>;
  epoch?: string;
  cancelRequested?: boolean;
}
export interface RestoreUpload {
  version: 1;
  origin: string;
  key: string;
  file: Blob;
  filename: string;
  stage: RestoreStage | null;
}
const operationKey = (budget: string) => `lifecycle-operation:${budget}`;
const uploadKey = "lifecycle-upload";
function validOperation(
  value: unknown,
  budget: string,
): asserts value is LifecycleOperation {
  const v = value as LifecycleOperation;
  if (
    !v ||
    v.version !== 1 ||
    v.origin !== location.origin ||
    v.budgetId !== budget ||
    typeof v.key !== "string" ||
    !["renamed", "duplicated", "backup", "deleted", "restored"].includes(
      v.kind,
    ) ||
    !["POST", "PATCH", "DELETE"].includes(v.method) ||
    typeof v.path !== "string" ||
    !v.path.startsWith(`/budgets/${budget}`) ||
    !v.body ||
    typeof v.body !== "object" ||
    (v.epoch !== undefined && !/^[0-9a-f]{32}$/.test(v.epoch))
  )
    throw new OfflineStateError(
      "Saved budget operation is unreadable. Keep this browser’s data for recovery.",
    );
  const root = `/budgets/${budget}`;
  const expected =
    v.kind === "renamed"
      ? ["PATCH", root]
      : v.kind === "deleted"
        ? ["DELETE", root]
        : v.kind === "duplicated"
          ? ["POST", root + "/duplicate"]
          : v.kind === "backup"
            ? ["POST", root + "/backups"]
            : null;
  if (
    expected
      ? v.method !== expected[0] || v.path !== expected[1]
      : v.method !== "POST" ||
        !new RegExp(
          `^${root}/(backups/[0-9a-f-]{36}/restore|restores/[0-9a-f-]{36}/activate)$`,
        ).test(v.path)
  )
    throw new OfflineStateError(
      "Saved budget operation has an invalid destination. No request was sent.",
    );
}
export async function readLifecycle(
  budget: string,
): Promise<LifecycleOperation | null> {
  return transaction(["cache"], "readonly", async (tx) => {
    const value = await result(
      tx.objectStore("cache").get(operationKey(budget)),
    );
    if (!value) return null;
    validOperation(value, budget);
    return value;
  });
}
export async function saveLifecycle(operation: LifecycleOperation) {
  validOperation(operation, operation.budgetId);
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      key = operationKey(operation.budgetId);
    const current = await result(store.get(key));
    if (current && JSON.stringify(current) !== JSON.stringify(operation))
      throw new OfflineStateError(
        "Another budget operation is saved. Reload to recover it before making a new change.",
      );
    await result(store.put(operation, key));
  });
}
export async function completeLifecycle(operation: LifecycleOperation) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      key = operationKey(operation.budgetId);
    const current = await result(store.get(key));
    if (current && JSON.stringify(current) !== JSON.stringify(operation))
      throw new OfflineStateError(
        "Saved work changed in another tab. Reload before continuing.",
      );
    await result(store.delete(key));
  });
}
export function lifecycleOperation(
  budget: Pick<ManagedBudget, "id" | "epoch">,
  kind: LifecycleKind,
  path: string,
  method: LifecycleOperation["method"],
  body: LifecycleOperation["body"],
): LifecycleOperation {
  return {
    version: 1,
    origin: location.origin,
    key: uuid(),
    budgetId: budget.id,
    kind,
    path,
    method,
    body,
    epoch: budget.epoch,
  };
}
export async function cancelLifecycle(
  operation: LifecycleOperation,
): Promise<LifecycleOperation> {
  return transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      key = operationKey(operation.budgetId);
    const current = await result(store.get(key));
    if (!current || current.key !== operation.key)
      throw new OfflineStateError(
        "Saved work changed. Reload before canceling.",
      );
    validOperation(current, operation.budgetId);
    const value = { ...current, cancelRequested: true };
    await result(store.put(value, key));
    return value;
  });
}
export async function sendLifecycle(operation: LifecycleOperation) {
  const saved = await readLifecycle(operation.budgetId);
  const cancel =
    operation.cancelRequested ||
    (saved?.key === operation.key && saved.cancelRequested);
  if (cancel) {
    const info = await request<{ data: { capabilities: string[] } }>("/info");
    if (!info.data.capabilities.includes("budget_lifecycle_cancellation"))
      throw new Error(
        "This server cannot safely cancel budget operations. Keep the saved request and retry after updating the server.",
      );
  }
  return request<{
    data:
      | ManagedBudget
      | BackupPoint
      | { budget: ManagedBudget; recovery_point_id: string | null };
  }>(
    operation.path,
    operation.body,
    operation.key,
    operation.method,
    undefined,
    !!cancel,
    operation.epoch,
  );
}
export async function readRestoreUpload(): Promise<RestoreUpload | null> {
  return transaction(["cache"], "readonly", async (tx) => {
    const v = (await result(tx.objectStore("cache").get(uploadKey))) as
      RestoreUpload | undefined;
    if (!v) return null;
    if (
      v.version !== 1 ||
      v.origin !== location.origin ||
      typeof v.key !== "string" ||
      !(v.file instanceof Blob)
    )
      throw new OfflineStateError(
        "Saved archive is unreadable. Keep this browser’s data and the original file for recovery.",
      );
    return v;
  });
}
export async function saveRestoreUpload(value: RestoreUpload) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      current = (await result(store.get(uploadKey))) as
        RestoreUpload | undefined;
    if (
      current &&
      (current.key !== value.key || (current.stage && !value.stage))
    )
      throw new OfflineStateError(
        "Another archive is saved in this browser. Reload to recover it.",
      );
    await result(store.put(value, uploadKey));
  });
}
export async function clearRestoreUpload(value: RestoreUpload) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache"),
      current = (await result(store.get(uploadKey))) as
        RestoreUpload | undefined;
    if (current?.key !== value.key)
      throw new OfflineStateError(
        "Archive selection changed in another tab. Reload before continuing.",
      );
    if (
      current.stage &&
      (await result(store.get(operationKey(current.stage.budget_id))))
    )
      throw new OfflineStateError(
        "A restore is unconfirmed. Resolve the saved restore before changing files.",
      );
    await result(store.delete(uploadKey));
  });
}
export async function completeRestoreUpload(
  upload: RestoreUpload,
  operation: LifecycleOperation,
) {
  await transaction(["cache"], "readwrite", async (tx) => {
    const store = tx.objectStore("cache");
    const savedUpload = await result(store.get(uploadKey));
    const savedOperation = await result(
      store.get(operationKey(operation.budgetId)),
    );
    if (
      savedUpload?.key !== upload.key ||
      (savedOperation &&
        JSON.stringify(savedOperation) !== JSON.stringify(operation))
    )
      throw new OfflineStateError(
        "Restore recovery changed in another tab. Reload to recover its result.",
      );
    await result(store.delete(operationKey(operation.budgetId)));
    await result(store.delete(uploadKey));
  });
}
export async function uploadRestore(
  value: RestoreUpload,
): Promise<RestoreStage> {
  let response: Response;
  try {
    response = await fetch("/api/v1/restores", {
      method: "POST",
      headers: {
        "Content-Type": "application/zip",
        "Idempotency-Key": value.key,
      },
      body: value.file,
      signal: AbortSignal.timeout(30000),
    });
  } catch {
    throw new RequestError(
      "The upload is unconfirmed. Retry the saved archive.",
      true,
    );
  }
  const body = await response.json().catch(() => null);
  if (!response.ok || !body?.data)
    throw new RequestError(
      body?.error?.message ??
        "The archive could not be reviewed. Retry the saved upload.",
      response.status >= 500 || !body,
      body?.error?.code,
    );
  return body.data;
}
