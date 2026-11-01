import { test, expect } from "@playwright/test";

async function openPanel(page) {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
}

async function chooseLanguage(page, language) {
  await page.locator("#language-trigger").click();
  await page.locator(`[data-language-option="${language}"]`).click();
}
async function expectLanguage(page, language) {
  await expect(page.locator(`[data-language-option="${language}"]`)).toHaveAttribute("aria-selected", "true");
}

for (const [system, resolved, settingsLabel] of [
  ["zh-CN", "zh-CN", "设置"],
  ["zh-TW", "zh-CN", "设置"],
  ["en-US", "en", "Settings"],
  ["fr-FR", "en", "Settings"],
]) {
  test(`system language ${system} resolves to ${resolved}`, async ({ page }) => {
    await page.addInitScript(language => { window.__TP_TEST_SYSTEM_LANGUAGE__ = language; }, system);
    await openPanel(page);
    await expect(page.locator("html")).toHaveAttribute("lang", resolved);
    await expect(page.locator("#settings-button")).toHaveAttribute("aria-label", settingsLabel);
    await page.locator("#settings-button").click();
    await expectLanguage(page, "system");
    await expect(page.locator(`#language-trigger [data-language-icon="${resolved}"]`)).toBeVisible();
  });
}

test("manual language choice saves, survives panel release, and preserves user content", async ({ page }) => {
  await openPanel(page);
  const original = await page.evaluate(() => structuredClone({ records: window.__TP_TEST__.state.records, groups: window.__TP_TEST__.state.groups }));
  await page.locator("#settings-button").click();
  await chooseLanguage(page, "en");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator("#settings-button")).toHaveAttribute("aria-label", "Settings");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.language)).toBe("en");
  await expectLanguage(page, "en");
  expect(await page.evaluate(() => ({ records: window.__TP_TEST__.state.records, groups: window.__TP_TEST__.state.groups }))).toEqual(original);
  await page.keyboard.press("Escape");
  await expect(page.locator('.record[data-id="text"]')).toContainText("React 迁移测试");
  await expect(page.locator('.group[data-view="group:1"]')).toContainText("工作");
  await page.evaluate(() => { window.__TP_TEST__.hide(); window.__TP_TEST__.idle(); });
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await page.locator("#settings-button").click();
  await chooseLanguage(page, "system");
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.language)).toBe("system");
});

test("saved choice overrides system and failed saves retain the saved language", async ({ page }) => {
  await page.addInitScript(() => { window.__TP_TEST_INITIAL_LANGUAGE__ = "en"; window.__TP_TEST_SYSTEM_LANGUAGE__ = "zh-CN"; });
  await openPanel(page);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await page.locator("#settings-button").click();
  await page.evaluate(() => { window.__TP_TEST__.settingsError = "write failed"; });
  await chooseLanguage(page, "zh-CN");
  await expect(page.locator("#toast")).toContainText("write failed");
  await expectLanguage(page, "en");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  expect(await page.evaluate(() => window.__TP_TEST__.state.settings.language)).toBe("en");
});

test("preview restores saved language and responds to changes without translating content", async ({ page }) => {
  await page.addInitScript(() => { window.__TP_TEST_INITIAL_LANGUAGE__ = "en"; });
  await page.goto("/tests/fixtures/preview.html?window=preview");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.some(call => call.args.event === "preview-ready"))).toBe(true);
  await page.evaluate(() => window.__TP_TEST__.emit("preview-content", {
    id: "text", kind: "text", body: "用户原文 English 中文", title: "用户标题", files: [], groupName: "工作",
    preview: true, status: "ready", width: 420, maxHeight: 580, generation: 1, search: "",
  }));
  await expect(page.locator(".text-content")).toHaveText("用户原文 English 中文");
  await expect(page.locator(".details dd")).toHaveText("工作");
  await expect(page.locator(".details dt")).toHaveText("Group");
  await page.evaluate(() => window.__TP_TEST__.emit("language-changed", { preference: "zh-CN", resolved: "zh-CN" }));
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.locator(".text-content")).toHaveText("用户原文 English 中文");
  await expect(page.locator(".details dd")).toHaveText("工作");
  await expect(page.locator(".details dt")).toHaveText("分组");
});

test("English panel and settings fit a 420px window", async ({ page }) => {
  await page.addInitScript(() => { window.__TP_TEST_INITIAL_LANGUAGE__ = "en"; });
  await openPanel(page);
  const overflow = async selector => page.locator(selector).evaluate(element => element.scrollWidth > element.clientWidth + 1);
  expect(await overflow(".shell")).toBe(false);
  expect(await overflow(".toolbar")).toBe(false);
  await page.locator("#settings-button").click();
  const settingsOverflow = await page.locator(".settings-content").evaluate(element => ({
    width: element.clientWidth,
    scrollWidth: element.scrollWidth,
    overflowing: [...element.querySelectorAll("*")].filter(child => child.clientWidth && child.scrollWidth > child.clientWidth + 1).map(child => ({
      tag: child.tagName, id: child.id, className: child.className, width: child.clientWidth, scrollWidth: child.scrollWidth,
    })),
  }));
  expect(settingsOverflow.scrollWidth, JSON.stringify(settingsOverflow)).toBeLessThanOrEqual(settingsOverflow.width + 1);
  expect(await overflow("#settings-general")).toBe(false);
  await page.locator("#settings-general").evaluate(element => element.scrollIntoView({ block: "start" }));
  await page.screenshot({ path: "test-results/i18n-settings-en.png" });
});


test("language menu follows existing popup styles and keyboard dismissal", async ({ page }) => {
  await openPanel(page);
  await page.locator("#settings-button").click();
  const trigger = page.locator("#language-trigger"), menu = page.locator("#language-menu");
  await trigger.focus();
  await page.keyboard.press("ArrowDown");
  await expect(menu).toBeVisible();
  for (const language of ["system", "zh-CN", "en"]) {
    await expect(menu.locator(`[data-language-option="${language}"] [data-language-icon="${language === "system" ? "zh-CN" : language}"]`)).toBeVisible();
  }
  await expect(trigger.locator('[data-language-icon="zh-CN"]')).toBeVisible();
  await expect(page.locator('[data-language-option="system"]')).toBeFocused();
  await page.keyboard.press("End");
  await expect(page.locator('[data-language-option="en"]')).toBeFocused();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await page.keyboard.press("Enter");
  await expect(menu).toBeHidden();
  await expect(trigger).toBeFocused();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await trigger.click();
  await expect(trigger.locator('[data-language-icon="en"]')).toBeVisible();
  await expect(menu.locator('[data-language-option="system"] [data-language-icon="zh-CN"]')).toBeVisible();
  await page.keyboard.press("Home");
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(trigger).toBeFocused();
  await expect(page.locator("#settings")).toBeVisible();
  await trigger.click();
  await page.locator("#settings-general .settings-copy").first().click();
  await expect(menu).toBeHidden();
  await trigger.click();
  await page.keyboard.press("Tab");
  await expect(menu).toBeHidden();
  await expect(page.locator("#autostart")).toBeFocused();
  await trigger.click();
  await page.locator("#settings-general").evaluate(element => element.scrollIntoView({ block: "start" }));
  const menuBox = await menu.boundingBox();
  expect(menuBox.x).toBeGreaterThanOrEqual(0);
  expect(menuBox.x + menuBox.width).toBeLessThanOrEqual(420);
  await page.screenshot({ path: "test-results/i18n-language-menu-en.png" });
});
