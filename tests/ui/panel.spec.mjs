import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
});

test("remove terminal newlines defaults on, persists both values, tags named and searched records and retains originals", async ({ page }) => {
  await page.evaluate(() => {
    const t = window.__TP_TEST__;
    t.change([
      t.makeRecord("text", "text", "DSH 0.2.1-rc.2\r\n", { pinnedAt: 1 }),
      t.makeRecord("named", "text", "    first\n\tsecond\n", { title: "标题" }),
      t.makeRecord("plain", "text", "没有末尾换行"),
      t.makeRecord("url", "link", "https://example.com\r"),
      t.makeRecord("hex", "color", "#abc\n"),
    ]);
  });
  await expect(page.locator(".trailing-newline-tag")).toHaveCount(0);
  await page.locator("#settings-button").click();
  const toggle = page.getByRole("checkbox", { name: "移除末尾换行", exact: true });
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.removeTrailingNewlines)).toBe(false);
  await page.keyboard.press("Escape");
  await expect(page.locator(".trailing-newline-tag")).toHaveCount(4);
  await expect(page.locator('[data-id="plain"] .trailing-newline-tag')).toHaveCount(0);
  await expect(page.locator('[data-id="named"] .trailing-newline-tag')).toHaveText("有换行");
  await page.locator("#search").fill("DSH");
  await expect(page.locator(".trailing-newline-tag")).toHaveText("有换行");
  await page.locator("#settings-button").click();
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.removeTrailingNewlines)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(page.locator(".trailing-newline-tag")).toHaveCount(0);
  expect(await page.evaluate(() => window.__TP_TEST__.state.records.find(r => r.id === "text").body)).toBe("DSH 0.2.1-rc.2\r\n");
});

test("development tools release hidden content immediately and reopen without losing records", async ({ page }) => {
  const count = await page.evaluate(() => window.__TP_TEST__.state.records.length);
  await page.locator("#settings-button").click();
  await expect(page.locator(".settings-group").first()).toHaveAttribute("id", "settings-development");
  await page.locator("#background-resident").check();
  await expect(page.locator("#release-panel-now")).toBeDisabled();
  await page.locator("#background-resident").uncheck();
  await page.locator("#release-panel-now").click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.visible)).toBe(false);
  await expect(page.locator(".record")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.released?.rows)).toBe(0);
  expect(await page.evaluate(() => window.__TP_TEST__.state.records.length)).toBe(count);
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="text"]')).toBeFocused();
  await expect(page.locator("#settings-development")).toBeHidden();
});

test("closing from another group reopens the default group and focuses its first record", async ({ page }) => {
  for (const view of ["favorite", "group:1"]) {
    await page.locator(`[data-view="${view}"]`).click();
    await expect(page.locator(`[data-view="${view}"]`)).toHaveClass(/active/);
    await page.locator("#close").click();
    await expect.poll(() => page.evaluate(() => window.__TP_TEST__.visible)).toBe(false);
    await page.evaluate(() => window.__TP_TEST__.show());
    await expect(page.locator('[data-view="default"]')).toHaveClass(/active/);
    await expect(page.locator('.record[data-id="text"]')).toBeFocused();
    await expect(page.locator('.record[data-id="link"]')).toBeVisible();
  }
});

for (const close of ["button", "Escape"]) test(`first reopen survives a delayed ${close} hide reply`, async ({ page }) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (name, args) => {
      const result = await invoke(name, args);
      if (name === "hide_panel") await new Promise(resolve => { window.__TP_TEST__.finishHide = resolve; });
      return result;
    };
  });
  if (close === "button") await page.locator("#close").click();
  else await page.keyboard.press("Escape");
  await page.evaluate(() => window.__TP_TEST__.idle());
  await expect(page.locator("#groups button")).toHaveCount(0);
  await expect(page.locator(".record")).toHaveCount(0);
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="text"]')).toBeFocused();
  await page.evaluate(async () => {
    window.__TP_TEST__.finishHide();
    await new Promise(resolve => setTimeout(resolve, 0));
  });
  await expect(page.locator('[data-view="default"]')).toHaveClass(/active/);
  await expect(page.locator('.record[data-id="text"]')).toBeFocused();
  await page.keyboard.press("F2");
  await expect(page.locator(".record-rename-input")).toBeFocused();
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
for (const shortcut of ["1", "Control+1"]) test(`${shortcut} applies the matching scrolled visible ordinal`, async ({ page }) => {
  await page.locator("#list").evaluate(list => { list.scrollTop = 1000; });
  await expect(page.locator(".ordinal").filter({ hasText: /^1$/ })).toHaveCount(1);
  const firstVisible = page.locator(".record").filter({ has: page.locator(".ordinal").filter({ hasText: /^1$/ }) });
  // The scroll event updates ordinals asynchronously; wait until the old first row is replaced.
  await expect(firstVisible).not.toHaveAttribute("data-id", "text");
  const id = await firstVisible.getAttribute("data-id");
  await page.locator("#list").click({ position: { x: 20, y: 20 } }); await page.keyboard.press(shortcut);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.name === "apply_record")?.args.id)).toBe(id);
});
for (const search of ["", "React"]) {
  for (const code of ["Digit2", "Numpad2"]) test(`Ctrl+${code} applies the second visible result with search focused (${search || "empty"})`, async ({ page }) => {
    await page.locator("#search").fill(search);
    await expect(page.locator("#search")).toBeFocused();
    await expect(page.locator('.record[data-id="link"] .ordinal')).toHaveText("2");
    // Playwright maps Numpad2 to ArrowDown; dispatch its numeric event to model NumLock being on.
    if (code === "Numpad2") await page.locator("#search").dispatchEvent("keydown", { key: "2", code, location: 3, ctrlKey: true });
    else await page.keyboard.press(`Control+${code}`);
    await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "apply_record").map(call => call.args.id))).toEqual(["link"]);
    await page.evaluate(() => window.__TP_TEST__.show());
    await expect(page.locator('.record[data-id="link"]')).toBeVisible();
    await expect(page.locator("#search")).toHaveValue(search);
  });
}

test("plain digits still edit focused search and Ctrl+digits leave empty results alone", async ({ page }) => {
  await page.locator("#search").fill("React");
  await page.keyboard.press("2");
  await expect(page.locator("#search")).toHaveValue("React2");
  await expect(page.locator(".record")).toHaveCount(0);
  await page.keyboard.press("Control+1");
  await expect(page.locator("#search")).toHaveValue("React2");
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "apply_record"))).toEqual([]);
});

test("digit shortcuts preserve group editing and settings input", async ({ page }) => {
  await page.locator("#add-group").click();
  await page.locator(".group-input").fill("分组");
  await page.keyboard.press("1"); await page.keyboard.press("Control+2");
  await expect(page.locator(".group-input")).toHaveValue("分组1");
  await page.keyboard.press("Escape");
  await page.locator("#settings-button").click();
  await page.locator("#max-ordinary").focus(); await page.keyboard.press("Control+1");
  await expect(page.locator("#max-ordinary")).toHaveValue("300");
  await expect(page.locator(".shortcut-table")).toContainText("Ctrl+1～9");
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "apply_record"))).toEqual([]);
});

test("Esc hides without background refresh; idle release clears DOM before acknowledging", async ({ page }) => {
  await page.locator("#search").fill("React"); await page.keyboard.press("Escape");
  await expect(page.locator("#search")).toHaveValue(""); await expect(page.locator("#search")).not.toBeFocused();
  await expect(page.locator(".record").first()).toBeVisible(); await page.keyboard.press("Escape");
  await expect(page.locator(".record").first()).toBeAttached();
  const before = await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "get_state").length);
  await page.evaluate(() => window.__TP_TEST__.emit("history-changed"));
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "get_state").length)).toBe(before);
  await page.evaluate(() => window.__TP_TEST__.idle());
  await expect(page.locator(".record")).toHaveCount(0); await expect(page.locator("#groups button")).toHaveCount(0);
  expect(await page.evaluate(() => window.__TP_TEST__.released)).toMatchObject({ rows: 0, images: 0 });
  await page.evaluate(() => window.__TP_TEST__.show()); await expect(page.locator(".record").first()).toBeVisible();
});

test("warm reopen retains decoded image nodes without source changes or another thumbnail request", async ({ page }) => {
  await expect(page.locator('img[data-preview="image"]')).toHaveJSProperty("naturalWidth", 320);
  await page.evaluate(() => {
    const node = document.querySelector('img[data-preview="image"]');
    window.__TP_TEST__.cachedImage = node;
    window.__TP_TEST__.imageChanges = [];
    new MutationObserver(records => window.__TP_TEST__.imageChanges.push(...records.map(record => record.attributeName)))
      .observe(node, { attributes: true, attributeFilter: ["src"] });
    window.__TP_TEST__.imageReads = window.__TP_TEST__.calls.filter(call => call.name === "preview_data" && call.args.id === "image").length;
  });
  for (let i = 0; i < 3; i++) {
    await page.locator("#close").click();
    const hidden = await page.evaluate(() => window.__TP_TEST__.generation);
    await page.evaluate(() => window.__TP_TEST__.show());
    await expect(page.locator('.record[data-id="text"]')).toBeFocused();
    await page.evaluate(token => { window.__TP_TEST__.emit("panel-hidden", token); window.__TP_TEST__.idle(token); }, hidden);
    expect(await page.evaluate(() => {
      const t = window.__TP_TEST__, node = document.querySelector('img[data-preview="image"]');
      return { same: node === t.cachedImage, complete: node.complete, width: node.naturalWidth, changes: t.imageChanges.length,
        reads: t.calls.filter(call => call.name === "preview_data" && call.args.id === "image").length - t.imageReads };
    })).toEqual({ same: true, complete: true, width: 320, changes: 0, reads: 0 });
  }
});
test("background residency below autostart keeps image nodes and shows before a delayed refresh", async ({ page }) => {
  await expect(page.locator('img[data-preview="image"]')).toHaveAttribute("src", /data:image/);
  await page.evaluate(() => { window.__TP_TEST__.cachedImage = document.querySelector('img[data-preview="image"]'); });
  await page.locator("#settings-button").click();
  const toggle = page.getByLabel("后台常驻", { exact: true });
  await expect(toggle).not.toBeChecked();
  expect(await page.locator('#settings input[type="checkbox"]').evaluateAll(nodes => nodes.slice(0, 2).map(node => node.id))).toEqual(["autostart", "background-resident"]);
  await toggle.check();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.backgroundResident)).toBe(true);
  await page.keyboard.press("Escape");
  await page.locator("#close").click();
  await page.evaluate(() => window.__TP_TEST__.idle());
  expect(await page.evaluate(() => document.querySelector('img[data-preview="image"]') === window.__TP_TEST__.cachedImage)).toBe(true);
  expect(await page.evaluate(() => window.__TP_TEST__.released)).toBeUndefined();
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (name, args) => {
      if (name === "get_state") await new Promise(resolve => { window.__TP_TEST__.finishRefresh = resolve; });
      return invoke(name, args);
    };
    window.__TP_TEST__.originalInvoke = invoke;
    window.__TP_TEST__.show();
  });
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.visible && !!window.__TP_TEST__.finishRefresh)).toBe(true);
  await expect(page.locator('.record[data-id="text"]')).toBeFocused();
  expect(await page.evaluate(() => document.querySelector('img[data-preview="image"]') === window.__TP_TEST__.cachedImage)).toBe(true);
  await page.evaluate(() => { window.__TAURI_INTERNALS__.invoke = window.__TP_TEST__.originalInvoke; window.__TP_TEST__.finishRefresh(); });
  await page.locator("#settings-button").click();
  await toggle.uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.backgroundResident)).toBe(false);
  await page.keyboard.press("Escape"); await page.locator("#close").click();
  await page.evaluate(() => window.__TP_TEST__.idle());
  await expect(page.locator(".record")).toHaveCount(0);
});

test("taskbar icon preference defaults on, saves immediately and survives page recreation", async ({ page }) => {
  await page.locator("#settings-button").click();
  const toggle = page.getByLabel("在任务栏显示图标", { exact: true });
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.showTaskbarIcon)).toBe(false);
  const saved = await page.evaluate(() => window.__TP_TEST__.state.settings);
  await page.reload();
  await page.evaluate(settings => { Object.assign(window.__TP_TEST__.state.settings, settings); window.__TP_TEST__.show(); }, saved);
  await page.locator("#settings-button").click();
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.showTaskbarIcon)).toBe(true);
});

for (const close of ["button", "Escape", "apply"]) test(`type retention defaults on and disabling resets the filter after ${close}`, async ({ page }) => {
  await page.locator("#kind-filter").click();
  await page.locator('[data-kind="image"]').click();
  await page.locator("#close").click(); await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="image"]')).toBeVisible();
  await expect(page.locator(".record")).toHaveCount(1);
  await page.locator("#settings-button").click();
  const toggle = page.getByLabel("保留类型筛选", { exact: true });
  await expect(toggle).toBeChecked();
  expect(await page.locator('#settings input[type="checkbox"]').evaluateAll(nodes => {
    const index = nodes.findIndex(node => node.id === "retain-search-input"); return nodes[index + 1].id;
  })).toBe("retain-type-filter");
  await toggle.uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.retainTypeFilter)).toBe(false);
  await page.keyboard.press("Escape");
  await expect(page.locator(".record")).toHaveCount(1);
  if (close === "button") await page.locator("#close").click();
  else if (close === "Escape") await page.keyboard.press("Escape");
  else { await page.locator('.record[data-id="image"]').focus(); await page.keyboard.press("Enter"); }
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.visible)).toBe(false);
  await page.evaluate(() => { window.__TP_TEST__.idle(); window.__TP_TEST__.show(); });
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await expect(page.locator("#clear-filter")).toBeHidden();
  await page.locator("#settings-button").click();
  await expect(toggle).not.toBeChecked();
});

test("settings restores the previous focus and saves search retention immediately", async ({ page }) => {
  await page.locator("#search").fill("React"); await page.locator("#settings-button").click();
  await expect(page.locator("#app-version")).toHaveText("1.0.2");
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

for (const choice of ["confirm", "cancel", "escape", "enter", "hide"]) test(`manual cache dialog ${choice} keeps confirmation and protected records`, async ({ page }) => {
  let nativeDialogs = 0;
  page.on("dialog", async dialog => { nativeDialogs++; await dialog.dismiss(); });
  const before = await page.evaluate(() => window.__TP_TEST__.state.records.map(record => ({ id: record.id, protected: record.favorite || record.groupId !== null })));
  await page.locator("#settings-button").click();
  await page.locator("#clear-cache").click();
  await expect(page.locator("#settings-cleanup-confirmation")).toBeVisible();
  await expect(page.locator("#cleanup-confirmation-title")).toHaveText("清理缓存？");
  await expect(page.locator("#settings-cleanup-confirmation [data-cancel]")).toBeFocused();
  expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "clear_cache").length)).toBe(0);
  if (choice === "escape") await page.keyboard.press("Escape");
  else if (choice === "enter") await page.keyboard.press("Enter");
  else if (choice === "hide") await page.evaluate(() => window.__TP_TEST__.hide());
  else await page.locator(choice === "confirm" ? "[data-confirm]" : "[data-cancel]").click();
  await expect(page.locator("#settings-cleanup-confirmation")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "clear_cache").length)).toBe(choice === "confirm" ? 1 : 0);
  const remaining = await page.evaluate(() => window.__TP_TEST__.state.records.map(record => record.id));
  expect(remaining).toEqual(before.filter(record => choice !== "confirm" || record.protected).map(record => record.id));
  expect(nativeDialogs).toBe(0);
  if (choice !== "hide") {
    await expect(page.locator("#settings")).toBeVisible();
    await expect(page.locator("#clear-cache")).toBeFocused();
  }
});

test("remembering and forgetting the dragged position do not reposition the current panel", async ({ page }) => {
  await page.locator("#settings-button").click();
  const places = () => page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "place_panel").length);
  const before = await places();
  await page.locator("#remember").check();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.rememberPosition)).toBe(true);
  expect(await page.evaluate(() => ({ x: window.__TP_TEST__.state.settings.panelX, y: window.__TP_TEST__.state.settings.panelY }))).toEqual({ x: 1200, y: 200 });
  expect(await places()).toBe(before);
  await page.locator("#remember").uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.rememberPosition)).toBe(false);
  expect(await places()).toBe(before);
  await page.locator("#settings-button").click();
  await page.locator("#close").click();
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await expect.poll(places).toBe(before + 1);
  expect(await page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.name === "place_panel").args.value.rememberPosition)).toBe(false);
});
test("groups can be created, renamed and removed through the original UI", async ({ page }) => {
  await page.locator("#add-group").click(); await page.locator(".group-input").fill("新分组"); await page.keyboard.press("Enter");
  await expect(page.locator('[data-view="group:3"]')).toHaveText("新分组"); await page.locator('[data-view="group:3"]').dblclick();
  await page.locator(".group-input").fill("重命名"); await page.keyboard.press("Enter"); await expect(page.locator('[data-view="group:3"]')).toHaveText("重命名");
  await page.locator('[data-view="group:3"]').click({ button: "right" });
  await expect(page.locator("#group-menu")).toBeFocused();
  await expect(page.locator("#group-menu button:focus")).toHaveCount(0);
  await page.keyboard.press("Enter"); await expect(page.locator(".group-input")).toHaveCount(0);
  for (const action of ["rename", "delete"]) {
    const item = page.locator(`[data-group-action="${action}"]`);
    await page.keyboard.press("ArrowDown");
    await expect(item).toBeFocused();
    await expect(item).toHaveCSS("outline-width", "0px");
    await expect(item).toHaveCSS("background-color", "rgb(246, 247, 251)");
  }
  await expect(page.locator('[data-group-action="delete"]')).toHaveCSS("color", "rgb(179, 91, 91)");
  await page.keyboard.press("Escape");
  await page.locator('[data-view="group:3"]').click({ button: "right" });
  await expect(page.locator("#group-menu button:focus")).toHaveCount(0);
  for (const button of await page.locator("#group-menu button").all()) await expect(button).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await page.locator('[data-group-action="delete"]').click();
  await expect(page.locator('[data-view="group:3"]')).toHaveCount(0);
});
test("file context actions and both preview triggers retain command payloads", async ({ page }) => {
  await page.locator('.record[data-id="file"]').click({ button: "right" }); await page.locator('[data-record-action="copy-paths"]').click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.name === "copy_record_paths")?.args.id)).toBe("file");
  await page.locator('.record[data-id="image"]').click(); await page.keyboard.press("ArrowRight");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "preview-content")?.args.payload.id)).toBe("image");
  await page.keyboard.press("ArrowLeft"); await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "large-preview-content")?.args.payload)).toEqual({ id: "image", kind: "image", count: 1, theme: "indigo" });
});
