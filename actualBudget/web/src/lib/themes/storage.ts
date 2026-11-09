let opened: Promise<IDBDatabase> | null = null;
function database(): Promise<IDBDatabase> {
  if (!opened)
    opened = new Promise((resolve, reject) => {
      const request = indexedDB.open("actualBudget.deviceThemes.v1", 1);
      request.onupgradeneeded = () =>
        request.result.createObjectStore("records");
      request.onsuccess = () => {
        request.result.onversionchange = () => {
          request.result.close();
          opened = null;
        };
        resolve(request.result);
      };
      request.onerror = () => {
        opened = null;
        reject(
          new Error(
            "Cannot open device theme storage. Enable browser storage and retry.",
          ),
        );
      };
      request.onblocked = () => {
        opened = null;
        reject(
          new Error(
            "Theme storage is blocked by another tab. Close that tab and retry.",
          ),
        );
      };
    });
  return opened;
}
export async function readThemeValue<T>(key: string): Promise<T | undefined> {
  try {
    const db = await database();
    return await new Promise((resolve, reject) => {
      const request = db
        .transaction("records", "readonly")
        .objectStore("records")
        .get(key);
      request.onsuccess = () => resolve(request.result as T | undefined);
      request.onerror = () =>
        reject(
          new Error(
            "Cannot read saved device themes. Existing sources have been kept.",
          ),
        );
    });
  } catch {
    throw new Error(
      "Cannot read saved device themes. Enable browser storage, close other tabs and retry. Existing sources have been kept.",
    );
  }
}
export async function writeThemeValue(
  key: string,
  value: unknown,
): Promise<void> {
  try {
    const db = await database();
    return await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("records", "readwrite");
      tx.objectStore("records").put(value, key);
      tx.oncomplete = () => resolve();
      tx.onerror = tx.onabort = () =>
        reject(
          new Error(
            "Cannot save device theme storage. Free space or enable storage, then retry. The previous theme remains active.",
          ),
        );
    });
  } catch {
    throw new Error(
      "Cannot save device theme storage. Free space or enable storage, then retry. The previous theme remains active.",
    );
  }
}
