import { uuid } from "./api";
let pendingOwner: Promise<string> | null = null;
/** One durable browser engine; IndexedDB serializes simultaneous first tabs on HTTP. */
export function historyOwner(): Promise<string> {
  if (pendingOwner) return pendingOwner;
  pendingOwner = new Promise<string>((resolve, reject) => {
    const open = indexedDB.open("actualBudget.engine.v1", 1);
    open.onupgradeneeded = () => open.result.createObjectStore("metadata");
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const db = open.result;
      const tx = db.transaction("metadata", "readwrite");
      const store = tx.objectStore("metadata");
      const read = store.get("history-owner");
      let owner: string;
      let problem: unknown;
      read.onsuccess = () => {
        try {
          const candidate =
            read.result ??
            localStorage.getItem("actualBudget.history-owner.v1") ??
            uuid();
          if (
            typeof candidate !== "string" ||
            !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
              candidate,
            )
          )
            throw new Error(
              "Saved history identity is unreadable. Keep browser data for recovery.",
            );
          owner = candidate;
          if (read.result === undefined) store.put(owner, "history-owner");
        } catch (e) {
          problem = e;
          tx.abort();
        }
      };
      tx.oncomplete = () => {
        db.close();
        resolve(owner);
      };
      tx.onabort = tx.onerror = () => {
        db.close();
        reject(problem ?? tx.error);
      };
    };
  }).catch((error) => {
    pendingOwner = null;
    throw error;
  });
  return pendingOwner;
}
