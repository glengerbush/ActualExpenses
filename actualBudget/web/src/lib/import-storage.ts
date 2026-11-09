import type { ImportStage } from "./imports-api";
export interface ImportSession {
  version: 1;
  key: string;
  file: Blob | null;
  filename: string;
  stage: ImportStage | null;
  activationKey: string;
  pendingUpload: boolean;
  pendingActivation: boolean;
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
          (value.version !== 1 ||
            typeof value.key !== "string" ||
            typeof value.activationKey !== "string" ||
            (value.file !== null && !(value.file instanceof Blob)))
        )
          reject(
            new Error(
              "Saved import could not be validated. Keep this browser's data for recovery.",
            ),
          );
        else resolve(value ?? null);
      };
      request.onerror = () =>
        reject(new Error("Cannot read saved import recovery data."));
    });
  } finally {
    db.close();
  }
}
export async function saveImportSession(session: ImportSession): Promise<void> {
  const db = await database();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction("session", "readwrite");
      const store = transaction.objectStore("session");
      const previous = store.get("current");
      let conflict = false;
      previous.onsuccess = () => {
        if (
          previous.result &&
          (previous.result.key !== session.key ||
            (previous.result.pendingActivation && !session.pendingActivation))
        ) {
          conflict = true;
          transaction.abort();
        } else {
          try {
            store.put(session, "current");
          } catch {
            transaction.abort();
          }
        }
      };
      transaction.oncomplete = () => resolve();
      transaction.onabort = transaction.onerror = () =>
        reject(
          new Error(
            conflict
              ? "Another tab has an import in progress. Reload to recover it."
              : "Import recovery data could not be saved. Free browser storage and retry the original operation safely.",
          ),
        );
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
