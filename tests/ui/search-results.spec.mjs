import { test, expect } from "@playwright/test";

test("search excerpts use the available row width and keep distant hits visible", async ({ page }) => {
  await page.setViewportSize({ width: 714, height: 674 });
  await page.goto("/tests/fixtures/search-results.html");
  await expect(page.locator('.record[data-id="short"]')).toBeVisible();
  const path = '"D:\\Workspace\\project\\micai-ohoskills"';
  await page.locator("#search").fill("micai-");
  const short = page.locator('[data-id="short"] .record-text');
  const long = page.locator('[data-id="long"] .record-text');
  await expect(short).toHaveText(path);
  await expect(page.locator('[data-id="path"] .record-text')).toHaveText("D:\\Workspace\\project\\micai-ohoskills");
  const check = async locator => {
    await expect(locator.locator(".search-hit")).toHaveText("micai-");
    await expect.poll(() => locator.evaluate(element => {
      const bounds = element.getBoundingClientRect(), hit = element.querySelector("mark").getBoundingClientRect();
      return hit.left >= bounds.left && hit.right <= bounds.right && element.scrollWidth <= element.clientWidth;
    })).toBe(true);
  };
  await check(long);
  await check(page.locator(".media-search-match"));
  await expect(long).toHaveText(/^….*micai-.*…$/);
  const wide = await long.textContent();
  await page.screenshot({ path: ".prototype/search-results.png" });
  await page.setViewportSize({ width: 420, height: 600 });
  await check(long);
  await expect.poll(async () => (await long.textContent()).length).toBeLessThan(wide.length);
  await page.locator('[data-id="long"]').hover();
  await check(long);
  await page.setViewportSize({ width: 1000, height: 674 });
  await expect(short).toHaveText(path);
  await check(long);
  await page.locator("#clear-search").click();
  await expect(short).toHaveText(path);
});

test("large search results avoid full-body canvas measurements on search and resize", async ({ page }) => {
  await page.addInitScript(() => {
    const measure = CanvasRenderingContext2D.prototype.measureText;
    window.__TP_MEASUREMENTS__ = [];
    CanvasRenderingContext2D.prototype.measureText = function (text) {
      window.__TP_MEASUREMENTS__.push(text.length);
      return measure.call(this, text);
    };
  });
  await page.goto("/tests/fixtures/search-results.html");
  await expect(page.locator('[data-id="short"]')).toBeVisible();
  await page.evaluate(() => {
    const t = window.__TP_TEST__;
    t.change([t.makeRecord("large", "text", "正文内容abc ".repeat(125_000) + "MATCH")]);
  });
  await expect(page.locator('[data-id="large"]')).toBeVisible();
  await page.locator("#search").fill("match");
  const row = page.locator('[data-id="large"] .record-text');
  await expect(row.locator(".search-hit")).toHaveText("MATCH");
  const wide = await row.textContent();
  await page.setViewportSize({ width: 700, height: 600 });
  await expect.poll(async () => (await row.textContent()).length).toBeGreaterThan(wide.length);
  const expanded = await row.textContent();
  await page.locator('[data-id="large"]').hover();
  await expect.poll(async () => (await row.textContent()).length).toBeLessThan(expanded.length);
  await expect(row.locator(".search-hit")).toHaveText("MATCH");
  const lengths = await page.evaluate(() => window.__TP_MEASUREMENTS__);
  expect(lengths.length).toBeGreaterThan(0);
  expect(Math.max(...lengths)).toBeLessThan(2000);
  expect(lengths.reduce((sum, length) => sum + length, 0)).toBeLessThan(100_000);
});
