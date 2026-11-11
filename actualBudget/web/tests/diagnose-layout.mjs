import { chromium } from "@playwright/test";
const launch = chromium.launch.bind(chromium);
chromium.launch = async (...args) => {
  const browser = await launch(...args);
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async (...args) => {
    const context = await newContext(...args);
    context.on("page", (page) => {
      const evaluate = page.evaluate.bind(page);
      page.evaluate = async (fn, ...args) => {
        const result = await evaluate(fn, ...args);
        if (result === false && String(fn).includes("scrollWidth")) {
          console.error(
            "LAYOUT FAILURE",
            process.argv[1],
            await evaluate(() => ({
              width: innerWidth,
              scrollWidth: document.documentElement.scrollWidth,
              elements: [...document.querySelectorAll("body *")]
                .filter((el) => el.getBoundingClientRect().right > innerWidth)
                .slice(0, 30)
                .map((el) => ({
                  tag: el.tagName,
                  class: el.className,
                  text: el.textContent.slice(0, 80),
                  right: el.getBoundingClientRect().right,
                })),
            })),
          );
          await page.screenshot({
            path: `test-results/layout-failure-${process.pid}.png`,
            fullPage: true,
          });
        }
        return result;
      };
    });
    return context;
  };
  return browser;
};
