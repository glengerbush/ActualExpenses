import { expect } from "@playwright/test";

// Recovery starts after a real IndexedDB commit. Immediate navigation can abort
// an in-flight write, so do not use a rendered input value as a durability signal.
export async function waitForReportingDraft(page, budget, expectedMetadata) {
  await expect
    .poll(() =>
      page.evaluate(async (budget) => {
        const db = await new Promise((resolve, reject) => {
          const request = indexedDB.open("actualBudget.offline.v1", 1);
          request.onsuccess = () => resolve(request.result);
          request.onerror = () => reject(request.error);
        });
        try {
          return await new Promise((resolve, reject) => {
            const request = db
              .transaction("cache")
              .objectStore("cache")
              .get(`reporting-draft:${budget}`);
            request.onsuccess = () =>
              resolve(request.result?.document?.config?.meta);
            request.onerror = () => reject(request.error);
          });
        } finally {
          db.close();
        }
      }, budget),
    )
    .toMatchObject(expectedMetadata);
}
