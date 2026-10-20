import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('[data-view="group:1"]')).toBeVisible();
});
const order = page => page.locator('#groups .group').evaluateAll(nodes => nodes.map(node => node.dataset.view));
async function hold(page) {
  const box = await page.locator('[data-view="group:1"]').boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await expect(page.locator('#groups .dragging')).toBeVisible();
}
test("long press saves order without selecting, clicking and renaming still work", async ({ page }) => {
  await hold(page);
  const last = await page.locator('[data-view="group:2"]').boundingBox();
  await page.mouse.move(last.x + last.width + 4, last.y + last.height / 2);
  await page.mouse.up();
  await expect.poll(() => order(page)).toEqual(['default', 'favorite', 'group:2', 'group:1']);
  await expect(page.locator('[data-view="default"]')).toHaveClass(/active/);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.groups.map(group => group.id))).toEqual([2, 1]);
  await page.locator('[data-view="group:1"]').click();
  await expect(page.locator('[data-view="group:1"]')).toHaveClass(/active/);
  await page.locator('[data-view="group:2"]').dblclick();
  await expect(page.getByRole('textbox', { name: '重命名分组' })).toBeVisible();
});
test("Escape cancels dragging and built-in groups stay fixed", async ({ page }) => {
  await hold(page);
  await page.mouse.move(360, 85);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(page.locator('#groups .dragging')).toHaveCount(0);
  expect(await order(page)).toEqual(['default', 'favorite', 'group:1', 'group:2']);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === 'reorder_groups').length)).toBe(0);
  const box = await page.locator('[data-view="default"]').boundingBox();
  await page.mouse.move(box.x + 8, box.y + 8); await page.mouse.down();
  await page.waitForTimeout(450); await page.mouse.move(350, box.y + 8); await page.mouse.up();
  expect(await order(page)).toEqual(['default', 'favorite', 'group:1', 'group:2']);
});
test("moving before the long press cancels sorting", async ({ page }) => {
  const box = await page.locator('[data-view="group:1"]').boundingBox();
  await page.mouse.move(box.x + 8, box.y + 8); await page.mouse.down();
  await page.mouse.move(box.x + 30, box.y + 8); await page.waitForTimeout(450);
  await expect(page.locator('#groups .dragging')).toHaveCount(0);
  await page.mouse.up();
  expect(await order(page)).toEqual(['default', 'favorite', 'group:1', 'group:2']);
});

test("edge dragging scrolls to offscreen groups and allows moving back to the start", async ({ page }) => {
  await page.evaluate(() => {
    window.__TP_TEST__.state.groups = Array.from({ length: 16 }, (_, index) => ({ id: index + 1, name: `分组 ${index + 1}` }));
    window.__TP_TEST__.emit('history-changed');
  });
  await expect(page.locator('[data-view="group:16"]')).toHaveCount(1);
  await hold(page);
  const nav = await page.locator('#groups').boundingBox();
  await page.mouse.move(nav.x + nav.width - 3, nav.y + nav.height / 2);
  await expect.poll(() => page.locator('#groups').evaluate(nav => nav.scrollLeft)).toBeGreaterThan(400);
  await expect.poll(() => page.locator('#groups').evaluate(nav => nav.scrollLeft >= nav.scrollWidth - nav.clientWidth - 1)).toBe(true);
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.groups.at(-1).id)).toBe(1);
  const last = await page.locator('[data-view="group:1"]').boundingBox();
  await page.mouse.move(last.x + last.width / 2, last.y + last.height / 2); await page.mouse.down();
  await expect(page.locator('#groups .dragging')).toBeVisible();
  await page.mouse.move(nav.x + 2, nav.y + nav.height / 2);
  await expect.poll(() => page.locator('#groups').evaluate(nav => nav.scrollLeft)).toBe(0);
  await page.mouse.up();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.groups[0].id)).toBe(1);
});
