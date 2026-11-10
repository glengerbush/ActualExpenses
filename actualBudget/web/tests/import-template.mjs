import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
/** Source archives are immutable shared templates. Every mutating test gets a
 * public duplicate; IDs within its financial graph remain unchanged. */
export async function cloneImportedTemplate(api, budgetId) {
  const source = await fetch(`${api}/budgets/${budgetId}`);
  assert.equal(source.status, 200);
  const { data } = await source.json();
  const response = await fetch(`${api}/budgets/${budgetId}/duplicate`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Idempotency-Key": randomUUID(),
      "X-Actual-Budget-Epoch": data.epoch,
    },
    body: JSON.stringify({
      id: randomUUID(),
      name: "Isolated transaction fixture",
    }),
  });
  assert.equal(response.status, 201, await response.clone().text());
  return (await response.json()).data.id;
}
