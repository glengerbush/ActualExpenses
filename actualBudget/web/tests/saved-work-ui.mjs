import { expect } from "@playwright/test";
// Silent delivery has no status row. Read durable work when verifying exact retries.
export async function savedWork(page) {
  return page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const opening = indexedDB.open("actualBudget.offline.v1", 1);
        opening.onerror = () => reject(opening.error);
        opening.onsuccess = () => {
          const db = opening.result;
          const request = db
            .transaction("operations")
            .objectStore("operations")
            .getAll();
          request.onsuccess = () => {
            db.close();
            resolve(request.result);
          };
          request.onerror = () => {
            db.close();
            reject(request.error);
          };
        };
      }),
  );
}
export async function expectSavedWork(page, count) {
  await expect.poll(async () => (await savedWork(page)).length).toBe(count);
}
export async function openSavedWork(page) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("navigation", { name: "Settings sections" })
    .getByRole("button", { name: "Recovery", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Troubleshoot saved changes", exact: true })
    .click();
}
export async function history(page) {
  await page
    .getByRole("button", { name: "Transaction history", exact: true })
    .click();
}
