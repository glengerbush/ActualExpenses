import { expect } from "@playwright/test";
// Register rows retain table semantics. Secondary operations live in the selection toolbar.
export function transactionRows(root, options = { name: /^Edit / }) {
  return root
    .getByRole("row", options)
    .or(root.locator("xpath=self::tr[@data-transaction-id]"));
}
export async function transactionAction(root, options) {
  const page = typeof root.page === "function" ? root.page() : root;
  const name =
    options.name instanceof RegExp
      ? new RegExp(
          options.name.source.replace(
            /Duplicate|Unlock|Schedule future/,
            "Edit",
          ),
          options.name.flags,
        )
      : /^Edit /;
  const candidates = transactionRows(root, { name });
  const row = (
    ["Edit splits", "Select split entries"].includes(options.name)
      ? candidates.filter({ hasText: /[0-9]+ splits/ })
      : candidates
  ).first();
  const selection = row.getByRole("checkbox");
  await expect(selection).toBeEnabled();
  const targetId = await row.getAttribute("data-transaction-id");
  const selectedIds = await page
    .locator(".register-table tr[data-transaction-id]:has(input:checked)")
    .evaluateAll((rows) => rows.map((row) => row.dataset.transactionId));
  for (const id of selectedIds) {
    if (id !== targetId)
      await page
        .locator(`tr[data-transaction-id="${id}"]`)
        .getByRole("checkbox")
        .uncheck();
  }
  await selection.check();
  const menu = page.locator(".transaction-options");
  if ((await menu.getAttribute("open")) === null)
    await menu
      .getByText("Selected transaction options", { exact: true })
      .click();
  return menu.getByRole("button", options);
}
