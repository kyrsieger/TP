import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator(".record").first()).toBeVisible();
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (name, args) => {
      if (["inspect_backup", "restore_backup", "export_data", "discard_backup"].includes(name)) {
        window.__TP_TEST__.calls.push({ name, args });
        if (name === "inspect_backup") return { token: "ui-test", records: 600, groups: 2, cleanupCount: 8, currentMaxOrdinary: 100, nextMaxOrdinary: 700, warnings: ["video：仅包含原路径，未包含文件本体"] };
        if (name === "restore_backup") {
          window.__TP_TEST__.state.settings.maxOrdinary = 700;
          return { status: "completed", records: 600, cleaned: 0 };
        }
        if (name === "export_data") return window.__TP_TEST__.cancelExport ? null : { path: "C:\\Exports\\tp.zip", records: 120, excludedSensitive: 1, warnings: ["文件已不可用"] };
        return;
      }
      return original(name, args);
    };
  });
  await page.locator("#settings-button").click();
});

test("storage offers only export and import with an unencrypted export confirmation", async ({ page }) => {
  await expect(page.locator(".transfer-actions button")).toHaveText(["导出", "导入"]);
  await page.locator("#export-data").click();
  const dialog = page.locator("#data-transfer-dialog");
  await expect(dialog).toBeVisible(); await expect(dialog).toContainText("ZIP 包未加密");
  await expect(dialog).toContainText("不包含应用设置");
  await dialog.getByRole("button", { name: "选择保存位置" }).click();
  await expect(dialog).toHaveCount(0); await expect(page.locator(".transfer-result")).toHaveCount(0);
  const call = await page.evaluate(() => window.__TP_TEST__.calls.find(item => item.name === "export_data"));
  expect(call.args ?? {}).toEqual({});
});

test("restore previews cleanup and raised capacity, and retains old timestamps unless requested", async ({ page }) => {
  await page.locator("#import-data").click();
  const dialog = page.locator("#data-transfer-dialog");
  await expect(dialog).not.toContainText("备份");
  await expect(dialog).toContainText("600 条记录"); await expect(dialog).toContainText("从 100 自动调整为 700");
  await expect(dialog).toContainText("8 条普通记录"); await expect(page.locator("#keep-expired")).not.toBeChecked();
  await page.screenshot({ path: "test-results/transfer-restore-preview.png" });
  await page.locator("#keep-expired").check();
  await dialog.getByRole("button", { name: "确认导入" }).click();
  await expect(dialog).toHaveCount(0); await expect(page.locator(".transfer-result")).toHaveCount(0);
  await expect(page.locator("#max-ordinary")).toHaveValue("700");
  const call = await page.evaluate(() => window.__TP_TEST__.calls.find(item => item.name === "restore_backup"));
  expect(call.args).toEqual({ token: "ui-test", keepExpired: true, expectedCleanupCount: 8, expectedMaxOrdinary: 700 });
});

test("escape cancels the restore confirmation without replacing records", async ({ page }) => {
  await page.locator("#import-data").click();
  await expect(page.locator("#keep-expired")).toBeVisible();
  await page.keyboard.press("Escape"); await expect(page.locator("#data-transfer-dialog")).toHaveCount(0);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.some(item => item.name === "restore_backup"))).toBe(false);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.some(item => item.name === "discard_backup"))).toBe(true);
});

test("cancelling the native save picker does not show success", async ({ page }) => {
  await page.evaluate(() => { window.__TP_TEST__.cancelExport = true; });
  await page.locator("#export-data").click();
  await page.getByRole("button", { name: "选择保存位置" }).click();
  await expect(page.locator("#data-transfer-dialog")).toHaveCount(0); await expect(page.locator(".transfer-result")).toHaveCount(0);
});
