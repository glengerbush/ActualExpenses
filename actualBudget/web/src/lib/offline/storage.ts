const databaseName = "actualBudget.offline.v1";
let opening: Promise<IDBDatabase> | null = null;
function database(): Promise<IDBDatabase> {
  if (opening) return opening;
  opening = new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(databaseName, 1);
    request.onupgradeneeded = () => {
      for (const store of ["cache", "operations", "receipts"])
        if (!request.result.objectStoreNames.contains(store))
          request.result.createObjectStore(store);
    };
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("Close other tabs and retry."));
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => {
        db.close();
        opening = null;
      };
      resolve(db);
    };
  }).catch((error) => {
    opening = null;
    throw error;
  });
  return opening;
}
export function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
/** Only IndexedDB requests inside work: never hold a transaction across network I/O. */
export async function transaction<T>(
  stores: string[],
  mode: IDBTransactionMode,
  work: (tx: IDBTransaction) => Promise<T>,
): Promise<T> {
  try {
    const db = await database();
    const tx = db.transaction(stores, mode);
    const done = new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = tx.onabort = () =>
        reject(tx.error ?? new Error("Storage transaction aborted"));
    });
    // A failed work callback must abort all writes, including a partial enqueue.
    try {
      const value = await work(tx);
      await done;
      return value;
    } catch (error) {
      try {
        tx.abort();
      } catch {}
      await done.catch(() => {});
      throw error;
    }
  } catch (error) {
    if (error instanceof OfflineStateError) throw error;
    throw new OfflineStateError(
      "Cannot save or read offline work. Free device space or enable browser storage, then retry. Existing queued work has been kept.",
    );
  }
}
export class OfflineStateError extends Error {}
export function readValue<T>(key: string): Promise<T | undefined> {
  return transaction(["cache"], "readonly", (tx) =>
    result(tx.objectStore("cache").get(key)),
  );
}
export function writeValue(key: string, value: unknown): Promise<void> {
  return transaction(["cache"], "readwrite", async (tx) => {
    await result(tx.objectStore("cache").put(value, key));
  });
}
