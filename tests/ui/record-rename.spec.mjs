import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
});

async function edit(page, id) {
  await page.locator(`.record[data-id="${id}"]`).click({ button: "right" });
  await expect(page.locator("#record-menu button").first().locator("span").first()).toHaveText("重命名");
  await page.locator('[data-record-action="rename"]').click();
  await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  return page.getByLabel("记录标题", { exact: true });
}

test("all six kinds rename through the first context action and preserve original content", async ({ page }) => {
  const original = await page.evaluate(() => window.__TP_TEST__.state.records.slice(0, 6).map(({ id, body, files }) => ({ id, body, files })));
  for (const id of ["text", "link", "color", "image", "file", "video"]) {
    const media = ["image", "video"].includes(id);
    const thumbnailReads = media ? await page.evaluate(id => window.__TP_TEST__.calls.filter(call => call.name === "preview_data" && call.args.id === id && call.args.role === "thumbnail").length, id) : 0;
    const input = await edit(page, id);
    const row = page.locator(`.record[data-id="${id}"]`);
    await expect(input).toHaveAttribute("placeholder", "输入标题");
    await expect(input).toHaveCSS("font-weight", "400");
    await expect(input).toHaveCSS("outline-width", "0px");
    await expect(input).toHaveCSS("box-shadow", "none");
    await expect(row).toHaveCSS("height", media ? "212px" : "48px");
    await expect(row.locator(".ordinal, .record-indicators, .record-actions, .media-note, .media-search-match")).toHaveCount(0);
    if (media) {
      await expect(row.locator(".preview")).toBeVisible();
      if (id === "video") await expect(row.locator(".play")).toBeVisible();
      const position = await row.evaluate(element => ({ inputBottom: element.querySelector("input").getBoundingClientRect().bottom,
        previewTop: element.querySelector(".preview-wrap").getBoundingClientRect().top }));
      expect(position.inputBottom).toBeLessThan(position.previewTop);
    } else {
      await expect(row.locator(":scope > *")).toHaveCount(1);
      expect(await input.evaluate(element => element.clientWidth / element.parentElement.clientWidth)).toBeGreaterThan(.9);
    }
    await input.fill(`标题 ${id}`); await input.press("Enter");
    await expect(page.locator(`.record[data-id="${id}"] .record-title`)).toHaveText(`标题 ${id}`);
    await expect(page.locator(`.record[data-id="${id}"]`)).toHaveCSS("height", ["image", "video"].includes(id) ? "212px" : "48px");
    await expect(page.locator(`.record[data-id="${id}"] .record-actions button`)).toHaveCount(4);
    if (media) expect(await page.evaluate(id => window.__TP_TEST__.calls.filter(call => call.name === "preview_data" && call.args.id === id && call.args.role === "thumbnail").length, id)).toBe(thumbnailReads);
  }
  expect(await page.evaluate(() => window.__TP_TEST__.state.records.slice(0, 6).map(({ id, body, files }) => ({ id, body, files })))).toEqual(original);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "apply_record"))).toEqual([]);
  const geometry = await page.locator('.record[data-id="file"]').evaluate(row => {
    const title = row.querySelector(".record-name").getBoundingClientRect(), body = row.querySelector(".record-text").getBoundingClientRect();
    return { gap: body.left - title.right, titleTop: title.top, bodyTop: body.top, tagInTitle: !!row.querySelector(".record-name .file-tag") };
  });
  expect(geometry.gap).toBe(8); expect(Math.abs(geometry.titleTop - geometry.bodyTop)).toBeLessThan(2); expect(geometry.tagInTitle).toBe(true);
  await page.locator("#search").fill("标题 image");
  await expect(page.locator(".record")).toHaveCount(1); await expect(page.locator(".record-title .search-hit")).toHaveText("标题 image");
  await expect(page.locator(".media-search-match")).toHaveCount(0);
  await page.locator("#search").fill("标题 text");
  await expect(page.locator(".record-text")).toHaveText("React 迁移测试 ...");
});

test("directories gain only rename while file path actions remain after rename", async ({ page }) => {
  await page.locator('.record[data-id="file"]').click({ button: "right" });
  await expect(page.locator("#record-menu button")).toHaveText(["重命名F2", "复制路径", "打开所在目录", "另存为"]);
  await expect(page.locator("#record-menu")).toBeFocused();
  await expect(page.locator("#record-menu button:focus")).toHaveCount(0);
  await page.keyboard.press("Enter"); await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await page.keyboard.press("ArrowDown"); await expect(page.locator('[data-record-action="rename"]')).toBeFocused();
  await page.keyboard.press("ArrowDown"); await expect(page.locator('[data-record-action="copy-paths"]')).toBeFocused();
  await expect(page.locator('[data-record-action="copy-paths"]')).toHaveCSS("outline-width", "0px");
  await expect(page.locator('[data-record-action="copy-paths"]')).toHaveCSS("background-color", "rgb(246, 247, 251)");
  await page.keyboard.press("Escape");
  const fileInput = await edit(page, "file");
  await fileInput.fill("文件标题"); await fileInput.press("Enter");
  await expect(fileInput).toHaveCount(0);
  await page.locator('.record[data-id="file"]').click({ button: "right" });
  await expect(page.locator("#record-menu")).toBeFocused();
  await expect(page.locator("#record-menu button:focus")).toHaveCount(0);
  for (const button of await page.locator("#record-menu button").all()) await expect(button).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await page.keyboard.press("ArrowUp"); await expect(page.locator('[data-record-action="save-as"]')).toBeFocused();
  await page.keyboard.press("ArrowDown"); await expect(page.locator('[data-record-action="rename"]')).toBeFocused();
  await page.keyboard.press("Enter"); await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await page.evaluate(() => window.__TP_TEST__.change([window.__TP_TEST__.makeRecord("directory", "file", "", { files: [{ original: "C:\\Folder", stored: null, isDirectory: true }] })]));
  const input = await edit(page, "directory");
  await expect(page.locator("#record-menu button")).toHaveCount(1);
  await input.fill("目录别名"); await input.press("Enter");
  await expect(page.locator(".record-name .file-tag")).toHaveText("目录");
  await expect(page.locator(".record-text")).toHaveText("Folder");
});

test("title editing protects shortcuts and IME, Escape and blur cancel, empty title restores display", async ({ page }) => {
  const input = await edit(page, "text");
  await input.fill("123");
  for (const shortcut of ["F2", "Control+1", "Control+d", "Control+g", "Control+p", "Control+f", "ArrowDown"]) await input.press(shortcut);
  await expect(input).toBeFocused();
  await input.dispatchEvent("keydown", { key: "Enter", code: "Enter", isComposing: true });
  await expect(input).toBeVisible();
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => ["apply_record", "batch_records", "rename_record", "hide_panel"].includes(call.name)))).toEqual([]);
  await input.press("Escape"); await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await expect(page.locator('.record[data-id="text"] .record-title')).toHaveCount(0);
  await (await edit(page, "text")).fill("取消的标题"); await page.locator("#search").click();
  await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await (await edit(page, "image")).fill("图片名称"); await page.getByLabel("记录标题").press("Enter");
  await expect(page.locator('.record[data-id="image"]')).toHaveCSS("height", "212px");
  await (await edit(page, "image")).fill("   "); await page.getByLabel("记录标题").press("Enter");
  await expect(page.locator('.record[data-id="image"]')).toHaveCSS("height", "180px");
  await expect(page.locator('.record[data-id="image"] .record-title')).toHaveCount(0);
});

test("opening the panel makes the selected first record ready for immediate rename", async ({ page }) => {
  const first = page.locator('.record[data-id="text"]');
  await expect(first).toHaveClass(/selected/);
  await page.keyboard.press("F2");
  await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await page.locator("#settings-button").click(); await page.locator("#rename-shortcut").press("Control+Shift+r");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.renameShortcut)).toBe("Control+Shift+R");
  await page.locator("#settings-button").click();
  await page.locator("#search").fill("React");
  await page.evaluate(() => window.__TP_TEST__.hide()); await expect(page.locator(".record")).toHaveCount(0);
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(first).toHaveClass(/selected/); await expect(first).toBeFocused();
  await page.keyboard.press("Control+Shift+r"); await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await page.locator("#search").click(); await page.keyboard.press("Control+Shift+r");
  await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await expect(page.locator("#search")).toHaveValue("React");
});

test("rename shortcut defaults to F2, stays in the list and follows the saved custom key", async ({ page }) => {
  const row = page.locator('.record[data-id="text"]');
  await row.click({ button: "right" });
  const label = page.locator('[data-record-action="rename"] .menu-shortcut');
  await expect(label).toHaveText("F2");
  const geometry = await page.locator('[data-record-action="rename"]').evaluate(button => {
    const [name, key] = [...button.children].map(element => element.getBoundingClientRect());
    return { gap: key.left - name.right, right: button.getBoundingClientRect().right - key.right };
  });
  expect(geometry.gap).toBeGreaterThan(20); expect(geometry.right).toBe(9);
  await page.keyboard.press("F2"); await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await row.click(); await page.keyboard.press("F2");
  await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  await page.getByLabel("记录标题", { exact: true }).fill("快捷键标题"); await page.keyboard.press("Enter");
  await expect(row.locator(".record-title")).toHaveText("快捷键标题");
  await page.locator("#search").click(); await page.keyboard.press("F2");
  await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await page.locator('[data-view="default"]').focus(); await page.keyboard.press("F2");
  await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await page.locator("#add-group").click(); await page.locator(".group-input").press("F2");
  await expect(page.locator(".record-rename-input")).toHaveCount(0); await page.keyboard.press("Escape");
  await row.click(); await page.locator("#settings-button").click();
  await expect(page.locator("#rename-shortcut")).toHaveValue("F2");
  await page.locator("#max-ordinary").press("F2"); await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await page.locator("#rename-shortcut").press("Control+Shift+r");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.renameShortcut)).toBe("Control+Shift+R");
  await expect(page.locator("#rename-shortcut")).toHaveValue("Ctrl+Shift+R");
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "register_panel_shortcut"))).toEqual([]);
  await page.locator("#settings-button").click(); await row.click({ button: "right" });
  await expect(label).toHaveText("Ctrl+Shift+R");
  await page.keyboard.press("Escape"); await row.click();
  await page.keyboard.press("F2"); await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await page.keyboard.press("Control+Shift+r"); await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await page.locator("#settings-button").click();
  await expect(page.locator("#rename-shortcut")).toHaveValue("Ctrl+Shift+R");
  await page.locator("#rename-shortcut").press("Control+d");
  await expect(page.locator("#settings .shortcut-error")).toContainText("已用于其他操作");
  expect(await page.evaluate(() => window.__TP_TEST__.state.settings.renameShortcut)).toBe("Control+Shift+R");
  await page.locator("#rename-shortcut").press("Delete");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.renameShortcut)).toBe("");
  await page.locator("#settings-button").click(); await row.click({ button: "right" }); await expect(label).toHaveCount(0);
  await page.keyboard.press("Escape"); await row.click(); await page.keyboard.press("Control+Shift+r");
  await expect(page.locator(".record-rename-input")).toHaveCount(0);
  await row.click({ button: "right" }); await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
  await expect(page.getByLabel("记录标题", { exact: true })).toBeFocused();
});

test("save failures retain the draft and titles take priority over original text", async ({ page }) => {
  await page.evaluate(() => { window.__TP_TEST__.renameError = "数据库写入失败"; });
  const input = await edit(page, "text"); await input.fill("待保存标题"); await input.press("Enter");
  await expect(page.locator("#toast")).toContainText("数据库写入失败");
  await expect(input).toHaveValue("待保存标题"); await expect(input).toBeFocused();
  await page.evaluate(() => { window.__TP_TEST__.renameError = null; });
  await input.fill("很长的记录名称".repeat(10)); await input.press("Enter");
  await expect(page.locator('.record[data-id="text"] .record-title')).toHaveText("很长的记录名称".repeat(10));
  await page.locator("#search").hover();
  const layout = await page.locator('.record[data-id="text"]').evaluate(row => {
    const title = row.querySelector(".record-title"), body = row.querySelector(".record-text"), content = row.querySelector(".record-content");
    return { titleTruncated: title.scrollWidth > title.clientWidth, bodyWidth: body.clientWidth, titleWidth: title.clientWidth, contentWidth: content.clientWidth };
  });
  expect(layout.titleTruncated).toBe(true); expect(layout.bodyWidth).toBe(0); expect(layout.titleWidth).toBe(layout.contentWidth);
  for (const id of ["text", "link", "file", "color"]) {
    const input = await edit(page, id); await input.fill("标题优先完整展示且正文只用余下空间"); await input.press("Enter");
    await page.locator("#search").hover();
    const row = page.locator(`.record[data-id="${id}"]`);
    const metrics = await row.evaluate(row => {
      const title = row.querySelector(".record-title"), name = row.querySelector(".record-name"), body = row.querySelector(".record-text"), content = row.querySelector(".record-content");
      return { truncated: title.scrollWidth > title.clientWidth, nameWidth: name.clientWidth, contentWidth: content.clientWidth,
        bodyWidth: body.clientWidth, gap: body.getBoundingClientRect().left - name.getBoundingClientRect().right };
    });
    expect(metrics.truncated).toBe(false); expect(metrics.nameWidth).toBeGreaterThan(metrics.contentWidth * .45);
    expect(metrics.bodyWidth).toBeGreaterThan(0); expect(metrics.gap).toBe(8);
    const long = await edit(page, id); await long.fill("很长的记录名称".repeat(10)); await long.press("Enter");
    await expect(row.locator(".record-text")).toHaveCSS("width", "0px");
    await row.hover(); await expect(row.locator(".record-actions")).toBeVisible();
    await expect(row.locator(".record-text")).toHaveCSS("width", "0px");
    const bounds = await row.evaluate(row => ({ titleRight: row.querySelector(".record-name").getBoundingClientRect().right,
      actionsLeft: row.querySelector(".record-actions").getBoundingClientRect().left }));
    expect(bounds.titleRight).toBeLessThanOrEqual(bounds.actionsLeft);
  }
});
