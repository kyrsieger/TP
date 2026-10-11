import { test, expect } from "@playwright/test";

const payload = (id, kind, body = "", files = []) => ({ id, kind, body, files, groupName: "工作", preview: true, status: "ready", width: 420, maxHeight: 580, generation: 1, search: "中文" });
async function openPreview(page, large = false) {
  const label = large ? "large-preview" : "preview";
  await page.goto(`/tests/fixtures/${label}.html?window=${label}`);
  await expect.poll(() => page.evaluate(label => window.__TP_TEST__.calls.some(call => call.args.event === label + "-ready"), label)).toBe(true);
}
async function send(page, event, content) { await page.evaluate(({ event, content }) => window.__TP_TEST__.emit(event, content), { event, content }); }
test("hover text preview preserves highlighting, group details, measurement and stale-clear protection", async ({ page }) => {
  await openPreview(page); await send(page, "preview-content", payload("text", "text", "第一行\n中文内容\n最后一行"));
  await expect(page.locator(".text-content .search-hit")).toHaveText("中文"); await expect(page.locator(".details dd")).toHaveText("工作");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "preview-measured")?.args.payload.height)).toBeGreaterThan(0);
  await send(page, "preview-clear", 0); await expect(page.locator(".card")).toBeVisible();
  await send(page, "preview-clear", 1); await expect(page.locator(".card")).toHaveCount(0);
});
test("hover file preview loads the single text file and keeps its original path", async ({ page }) => {
  await openPreview(page); await send(page, "preview-content", payload("file", "file", "", [{ original: "C:\\Files\\中文.md" }]));
  await expect(page.locator(".file-text-content")).toContainText("中文文本"); await expect(page.locator(".path strong")).toHaveText("中文.md");
  await send(page, "preview-clear", null); await expect(page.locator(".card")).toHaveCount(0);
});

test("renamed hover preview displays and searches title separately from the full original body", async ({ page }) => {
  await openPreview(page);
  await send(page, "preview-content", { ...payload("named", "text", "第一行\n第二行"), title: "中文标题" });
  await expect(page.locator(".record-preview-title .search-hit")).toHaveText("中文");
  await expect(page.locator(".text-content")).toHaveText("第一行\n第二行");
});
test("hover color and single image preview preserve their visuals and image lifetime", async ({ page }) => {
  await openPreview(page); await send(page, "preview-content", payload("color", "color", "#4f46e5"));
  await expect(page.locator(".color-disc")).toHaveCSS("background-color", "rgb(79, 70, 229)");
  await send(page, "preview-content", payload("image", "image")); await expect(page.locator("#large-preview")).toBeVisible();
  await send(page, "preview-clear", null); await expect(page.locator("img")).toHaveCount(0);
});
test("hover carousel changes pages, preloads adjacent media and clears all slides", async ({ page }) => {
  await openPreview(page); const files = Array.from({ length: 4 }, (_, index) => ({ original: `C:\\Files\\${index}.png` }));
  await send(page, "preview-content", payload("images", "image", "", files));
  await expect(page.locator(".media-slide img")).toBeVisible(); await page.locator(".media.multi").hover(); await page.locator('.page-indicator[data-index="2"]').click();
  await expect(page.locator('.page-indicator[data-index="2"]')).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.some(call => call.name === "preview_data" && call.args.index === 3))).toBe(true);
  await send(page, "preview-clear", null); await expect(page.locator(".media-slide")).toHaveCount(0);
});
test("large single-image preview reports native image size and closes by Space", async ({ page }) => {
  await openPreview(page, true); await send(page, "large-preview-content", { id: "image", kind: "image", count: 1 });
  await expect(page.locator("#image")).toBeVisible(); await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "large-preview-loaded")?.args.payload)).toEqual({ id: "image", fixed: false, width: 320, height: 180 });
  await page.keyboard.press("Space"); await expect(page.locator("#image")).not.toHaveAttribute("src", /.+/);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.some(call => call.name === "hide_large_preview"))).toBe(true);
});
test("large video carousel uses first-frame previews, switches pages and releases content", async ({ page }) => {
  await openPreview(page, true); await send(page, "large-preview-content", { id: "video", kind: "video", count: 3 });
  await expect(page.locator("#kind")).toHaveText("视频首帧"); await expect(page.locator(".media-slide img")).toBeVisible();
  await page.locator('.page-indicator[data-index="1"]').click(); await expect(page.locator('.page-indicator[data-index="1"]')).toHaveAttribute("aria-pressed", "true");
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "preview_data").every(call => call.args.role === "preview"))).toBe(true);
  await send(page, "large-preview-clear", null); await expect(page.locator(".media-slide")).toHaveCount(0);
});
