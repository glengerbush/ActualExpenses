import { expect } from "@playwright/test";

export async function checkIphonePayeePicker(browser, url, call, root) {
  const context = await browser.newContext({
    userAgent:
      "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1",
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  await context.addInitScript(() =>
    Object.defineProperty(navigator, "standalone", { value: true }),
  );
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  try {
    await page.goto(url);
    await page
      .getByRole("button", { name: "+ Add expense", exact: true })
      .tap();
    await page.getByLabel("Amount", { exact: true }).fill("2.50");
    const input = page.getByRole("combobox", { name: "Payee", exact: true });
    const name = "iPhone bakery";
    const create = page.getByRole("option", {
      name: `Create payee “${name}”`,
      exact: true,
    });
    await input.tap();
    await input.pressSequentially(name);
    await expect(create).toBeEnabled();
    const field = await input.boundingBox();
    const keyboardTop = field.y + field.height + 8;
    // Headless engines have no software keyboard. Model its actual browser seam:
    // visual viewport resize/pan while the layout viewport stays unchanged.
    await page.evaluate((height) => {
      Object.defineProperty(visualViewport, "height", {
        configurable: true,
        get: () => height,
      });
      visualViewport.dispatchEvent(new Event("resize"));
    }, keyboardTop);
    await expect
      .poll(async () => {
        const r = await create.boundingBox();
        return !!r && r.y >= 8 && r.y + r.height <= keyboardTop - 8;
      })
      .toBe(true);
    // Pan the visual viewport (e.g. Safari scrolls the focused input above its keyboard).
    await page.evaluate((bottom) => {
      Object.defineProperty(visualViewport, "offsetTop", {
        configurable: true,
        get: () => 120,
      });
      Object.defineProperty(visualViewport, "height", {
        configurable: true,
        get: () => bottom - 120,
      });
      visualViewport.dispatchEvent(new Event("scroll"));
    }, keyboardTop);
    await expect
      .poll(async () => {
        const r = await create.boundingBox();
        return !!r && r.y >= 128 && r.y + r.height <= keyboardTop - 8;
      })
      .toBe(true);
    // Native keyboard dismissal can blur the input without moving focus to another control.
    await input.evaluate((element) => element.blur());
    await expect(create).toBeVisible();
    await expect(create).toBeEnabled();
    await page.evaluate(() => {
      delete visualViewport.height;
      delete visualViewport.offsetTop;
      visualViewport.dispatchEvent(new Event("resize"));
    });
    await expect(create).toBeInViewport();
    await input.tap();
    await input.fill("");
    await expect(
      page.getByRole("option", { name: "Create payee", exact: true }),
    ).toBeDisabled();
    await input.pressSequentially(name);
    await expect(create).toBeEnabled();
    await page.screenshot({ path: "../../.scratch/payee-iphone-create.png" });
    await create.tap();
    await expect(input).toHaveValue(name);
    await expect(page.getByRole("listbox")).toHaveCount(0);
    await page
      .getByRole("button", { name: "Save transaction", exact: true })
      .tap();
    await expect
      .poll(async () =>
        (await call(root + "/transactions")).some(
          (row) => row.payee_name === name && row.amount === -250,
        ),
      )
      .toBe(true);
    // Changing focus or tapping elsewhere still dismisses the picker.
    await page
      .getByRole("button", { name: "+ Add expense", exact: true })
      .tap();
    await input.tap();
    await expect(page.getByRole("listbox")).toBeVisible();
    await page.getByLabel("Amount", { exact: true }).focus();
    await expect(page.getByRole("listbox")).toHaveCount(0);
    await input.tap();
    await page
      .getByRole("heading", { name: "Add transaction", exact: true })
      .tap();
    await expect(page.getByRole("listbox")).toHaveCount(0);
    expect(errors).toEqual([]);
  } finally {
    await context.close();
  }
}
