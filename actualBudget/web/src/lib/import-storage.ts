import type { ImportStage, ImportSource } from "./imports-api";
export interface ImportSession {
  version: 1;
  sourceKind?: ImportSource;
  key: string;
  file: Blob | null;
  filename: string;
  stage: ImportStage | null;
  activationKey: string;
  pendingUpload: boolean;
  pendingActivation: boolean;
  destination?: { budgetId: string; epoch: string };
}
async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("actualBudget.imports.v1", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("session");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(
        new Error(
          "Cannot save import recovery data on this device. Enable browser storage and retry.",
        ),
      );
  });
}
export async function readImportSession(): Promise<ImportSession | null> {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction("session", "readonly");
      const request = transaction.objectStore("session").get("current");
      request.onsuccess = () => {
        const value = request.result;
        if (
          value &&
          (![1, 2].includes(value.version) ||
            typeof value.key !== "string" ||
            (value.sourceKind !== undefined &&
              !["actual", "ynab4", "nynab"].includes(value.sourceKind)) ||
            typeof value.activationKey !== "string" ||
            (value.destination !== undefined &&
              (!/^[0-9a-f-]{36}$/i.test(value.destination?.budgetId ?? "") ||
                !/^[0-9a-f]{32}$/.test(value.destination?.epoch ?? ""))) ||
            (value.file !== null &&
              !(value.version === 1
                ? value.file instanceof Blob
                : value.file instanceof ArrayBuffer)))
        )
          reject(
            new Error(
              "Saved import could not be validated. Keep this browser's data for recovery.",
            ),
          );
        else
          resolve(
            value
              ? {
                  ...value,
                  version: 1,
                  file:
                    value.version === 2 && value.file !== null
                      ? new Blob([value.file])
                      : value.file,
                }
              : null,
          );
      };
      request.onerror = () =>
        reject(new Error("Cannot read saved import recovery data."));
    });
  } finally {
    db.close();
  }
}
export async function saveImportSession(session: ImportSession): Promise<void> {
  // Safari/WebKit can reject Blob/File values in IndexedDB even for tiny files.
  // Read before opening a transaction: awaiting file I/O inside it can close it.
  // Keep the original bytes and request identities, with legacy Blob reads intact.
  const stored = {
    ...session,
    version: 2,
    file: session.file ? await session.file.arrayBuffer() : null,
  };
  const db = await database();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction("session", "readwrite");
      const store = transaction.objectStore("session");
      const previous = store.get("current");
      let conflict = false;
      let failure: unknown;
      previous.onsuccess = () => {
        if (
          previous.result &&
          (previous.result.key !== session.key ||
            (previous.result.sourceKind ?? "actual") !==
              (session.sourceKind ?? "actual") ||
            previous.result.filename !== session.filename ||
            (previous.result.pendingActivation &&
              (!session.pendingActivation ||
                previous.result.activationKey !== session.activationKey ||
                JSON.stringify(previous.result.destination) !==
                  JSON.stringify(session.destination))))
        ) {
          conflict = true;
          transaction.abort();
        } else {
          try {
            store.put(stored, "current");
          } catch (error) {
            failure = error;
            transaction.abort();
          }
        }
      };
      transaction.oncomplete = () => resolve();
      transaction.onerror = (event) => {
        failure ??= (event.target as IDBRequest)?.error ?? transaction.error;
      };
      transaction.onabort = () => {
        const cause = failure ?? transaction.error;
        const quota =
          cause instanceof DOMException && cause.name === "QuotaExceededError";
        reject(
          new Error(
            conflict
              ? "Another tab has an import in progress. Reload to recover it."
              : quota
                ? "Import recovery data could not be saved because browser storage is full. Free storage and retry the original operation safely."
                : "Import recovery data could not be saved. Check that browser storage is enabled and retry. Keep the original export and retry the saved operation safely.",
            { cause },
          ),
        );
      };
    });
  } finally {
    db.close();
  }
}
export async function clearImportSession(
  key: string,
  confirmed = false,
): Promise<void> {
  const db = await database();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction("session", "readwrite");
      const store = transaction.objectStore("session");
      const previous = store.get("current");
      previous.onsuccess = () => {
        if (previous.result?.key === key) {
          if (
            !confirmed &&
            (previous.result.pendingUpload || previous.result.pendingActivation)
          ) {
            transaction.abort();
            return;
          }
          store.delete("current");
        }
      };
      transaction.oncomplete = () => resolve();
      transaction.onabort = transaction.onerror = () =>
        reject(
          new Error(
            "Saved import may have an unconfirmed operation in another tab. Reload and retry it safely.",
          ),
        );
    });
  } finally {
    db.close();
  }
}
