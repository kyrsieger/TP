import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
});
test("renders all original record kinds with matching row heights and lazy images", async ({ page }) => {
  const errors = []; page.on("pageerror", error => errors.push(error.message));
  await expect(page.locator('.preview[data-preview="image"]')).toHaveAttribute("src", /data:image/);
  const heights = await page.locator(".record").evaluateAll(rows => rows.slice(0, 6).map(row => Math.round(row.getBoundingClientRect().height)));
  expect(heights).toEqual([48, 48, 48, 180, 48, 180]);
  expect(await page.locator(".record").count()).toBeLessThan(20);
  await expect(page.locator('.record[data-id="text"] .ordinal')).toHaveText("1");
  await expect(page.locator('.record[data-id="text"] .record-text')).toHaveText("React 迁移测试 ...");
  expect(errors).toEqual([]);
});
test("search, type filter and group selection keep their original scope", async ({ page }) => {
  await page.locator("#search").fill("React");
  await expect(page.locator(".record")).toHaveCount(2); await expect(page.locator(".search-hit").first()).toBeVisible();
  await page.locator("#clear-search").click(); await page.locator("#kind-filter").click(); await page.locator('[data-kind="image"]').click();
  await expect(page.locator(".record")).toHaveCount(1);
  await page.locator('[data-view="favorite"]').click(); await expect(page.locator("#kind-filter")).toBeDisabled();
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await page.locator('[data-view="group:1"]').click(); await expect(page.locator('.record[data-id="file"]')).toBeVisible();
});
test("continuous selection and batch favorite, pin and group actions keep selected IDs", async ({ page }) => {
  await page.locator('.record[data-id="text"]').click(); await page.keyboard.press("Shift+ArrowDown");
  await expect(page.locator(".record.selected")).toHaveCount(2); await page.keyboard.press("Control+d");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "batch_records").at(-1)?.args)).toEqual({ ids: ["text", "link"], operation: { kind: "set_favorite", favorite: true } });
  await page.keyboard.press("Control+p"); await expect(page.locator(".record.selected .pin-indicator")).toHaveCount(2);
  await page.keyboard.press("Control+g"); await expect(page.locator("#move-group-menu")).toBeVisible();
  await page.keyboard.press("ArrowDown"); await page.keyboard.press("Enter");
  await expect(page.locator(".record.selected .group-indicator")).toHaveCount(2);
});
test("Ctrl+G opens after a distant virtual row is mounted", async ({ page }) => {
  await page.locator('.record[data-id="text"]').click(); await page.keyboard.press("ArrowUp");
  await expect(page.locator('.record[data-id="row-119"]')).toBeVisible(); await page.keyboard.press("Control+g");
  await expect(page.locator("#move-group-menu")).toBeVisible(); await page.keyboard.press("Escape");
  await expect(page.locator("#move-group-menu")).toBeHidden();
  await expect(page.locator('.record[data-id="row-119"]')).toHaveClass(/selected/);
});
test("visible ordinals apply the matching scrolled record", async ({ page }) => {
  await page.locator("#list").evaluate(list => { list.scrollTop = 1000; });
  await expect(page.locator(".ordinal").filter({ hasText: /^1$/ })).toHaveCount(1);
  const id = await page.locator(".record").filter({ has: page.locator(".ordinal").filter({ hasText: /^1$/ }) }).getAttribute("data-id");
  await page.locator("#list").click({ position: { x: 20, y: 20 } }); await page.keyboard.press("1");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.name === "apply_record")?.args.id)).toBe(id);
});
test("Esc clears search and focus, then hides and releases rendered records", async ({ page }) => {
  await page.locator("#search").fill("React"); await page.keyboard.press("Escape");
  await expect(page.locator("#search")).toHaveValue(""); await expect(page.locator("#search")).not.toBeFocused();
  await expect(page.locator(".record").first()).toBeVisible(); await page.keyboard.press("Escape");
  await expect(page.locator(".record")).toHaveCount(0); await expect(page.locator("#groups button")).toHaveCount(0);
  const before = await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "get_state").length);
  await page.evaluate(() => window.__TP_TEST__.emit("history-changed"));
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "get_state").length)).toBe(before);
  await page.evaluate(() => window.__TP_TEST__.show()); await expect(page.locator(".record").first()).toBeVisible();
});
test("settings restores the previous focus and saves search retention immediately", async ({ page }) => {
  await page.locator("#search").fill("React"); await page.locator("#settings-button").click();
  await expect(page.locator("#app-version")).toHaveText("1.0.0");
  await page.locator("#retain-search-input").uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.retainSearchInput)).toBe(false);
  await page.keyboard.press("Escape"); await expect(page.locator("#search")).toBeFocused();
  await page.locator("#close").click(); await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator("#search")).toHaveValue("");
});
for (const choice of ["confirm", "cancel", "escape"]) test(`cleanup modal ${choice} preserves finished-input and confirmation behavior`, async ({ page }) => {
  await page.evaluate(() => window.__TP_TEST__.setConfirmationCount(7)); await page.locator("#settings-button").click();
  await page.locator("#max-ordinary").fill("50");
  await page.waitForTimeout(350);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "save_settings").length)).toBe(0);
  await page.locator("#max-ordinary").press("Enter"); await expect(page.locator("#settings-cleanup-confirmation")).toBeVisible();
  await expect(page.locator("#cleanup-confirmation-description strong")).toHaveText("7");
  if (choice === "escape") await page.keyboard.press("Escape"); else await page.locator(choice === "confirm" ? "[data-confirm]" : "[data-cancel]").click();
  await expect(page.locator("#settings-cleanup-confirmation")).toHaveCount(0);
  await expect(page.locator("#max-ordinary")).toHaveValue(choice === "confirm" ? "50" : "300");
  expect(await page.evaluate(() => window.__TP_TEST__.state.settings.maxOrdinary)).toBe(choice === "confirm" ? 50 : 300);
});
test("invalid limits restore on blur and sensitive seconds save after typing", async ({ page }) => {
  await page.locator("#settings-button").click(); await page.locator("#max-ordinary").fill(""); await page.locator("#shortcut").focus();
  await expect(page.locator("#max-ordinary")).toHaveValue("300"); await expect(page.locator("#toast")).toHaveText("请输入有效的设置值");
  await page.locator("#sensitive").fill("60"); await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.sensitiveSeconds)).toBe(60);
});
test("groups can be created, renamed and removed through the original UI", async ({ page }) => {
  await page.locator("#add-group").click(); await page.locator(".group-input").fill("新分组"); await page.keyboard.press("Enter");
  await expect(page.locator('[data-view="group:3"]')).toHaveText("新分组"); await page.locator('[data-view="group:3"]').dblclick();
  await page.locator(".group-input").fill("重命名"); await page.keyboard.press("Enter"); await expect(page.locator('[data-view="group:3"]')).toHaveText("重命名");
  await page.locator('[data-view="group:3"]').click({ button: "right" }); await page.locator('[data-group-action="delete"]').click();
  await expect(page.locator('[data-view="group:3"]')).toHaveCount(0);
});
test("file context actions and both preview triggers retain command payloads", async ({ page }) => {
  await page.locator('.record[data-id="file"]').click({ button: "right" }); await page.locator('[data-record-action="copy-paths"]').click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.name === "copy_record_paths")?.args.id)).toBe("file");
  await page.locator('.record[data-id="image"]').click(); await page.keyboard.press("ArrowRight");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "preview-content")?.args.payload.id)).toBe("image");
  await page.keyboard.press("ArrowLeft"); await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "large-preview-content")?.args.payload)).toEqual({ id: "image", kind: "image", count: 1 });
});
