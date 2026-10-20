import { test, expect } from "@playwright/test";

async function openSettings(page) {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await page.locator("#settings-button").click();
}
async function selectFont(page, family) {
  await page.locator("#font-trigger").click();
  await page.locator("#font-search").fill(family || "系统默认");
  await page.getByRole("option", { name: family || "系统默认", exact: true }).click();
}
const fontOf = (page, selector) => page.locator(selector).evaluate(element => getComputedStyle(element).fontFamily);

for (const [family, labels] of [
  ["微软雅黑", ["主题配色", "字体", "样式"]],
  ["等线", ["主题配色", "字体"]],
  ["", ["主题配色", "字体"]],
]) {
  test(`settings immediately restore appearance rows for saved font ${family || "default"}`, async ({ page }) => {
    await page.addInitScript(family => { window.__TP_TEST_INITIAL_FONT__ = family; }, family);
    await page.goto("/tests/fixtures/panel.html");
    await expect(page.locator('.record[data-id="text"]')).toBeVisible();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    // Block any further enumeration, so a late response cannot hide a missing first render.
    await page.evaluate(() => {
      window.__TP_TEST__.fontListPending = new Promise(resolve => { window.__TP_TEST__.finishFontList = resolve; });
      window.__TP_TEST__.appearanceRenders = [];
      new MutationObserver(() => {
        if (document.querySelector("#font-trigger")) window.__TP_TEST__.appearanceRenders.push(
          [...document.querySelectorAll("#settings-appearance .setting-label")].map(label => label.textContent));
      }).observe(document.querySelector("#settings"), { childList: true, subtree: true });
    });
    for (let visit = 0; visit < 3; visit++) {
      await page.locator("#settings-button").click();
      await expect(page.locator("#font-value")).toHaveText(family || "系统默认");
      await expect(page.locator("#settings-appearance .setting-label")).toHaveText(labels);
      await page.locator("#settings-button").click();
    }
    // The directory also survives the normal idle-content unmount.
    await page.evaluate(() => { window.__TP_TEST__.hide(); window.__TP_TEST__.idle(); });
    await expect(page.locator(".record")).toHaveCount(0);
    await page.evaluate(() => window.__TP_TEST__.show());
    await expect(page.locator('.record[data-id="text"]')).toBeVisible();
    await page.locator("#settings-button").click();
    const renders = await page.evaluate(() => window.__TP_TEST__.appearanceRenders);
    expect(renders.length).toBeGreaterThanOrEqual(4);
    for (const render of renders) expect(render).toEqual(labels);
    expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "get_system_fonts").length)).toBe(1);
  });
}

test("refreshing the font menu keeps the style row until new metadata arrives, including after a failed refresh", async ({ page }) => {
  await openSettings(page);
  await selectFont(page, "微软雅黑");
  await page.evaluate(() => {
    window.__TP_TEST__.fontListPending = new Promise(resolve => { window.__TP_TEST__.finishFontList = resolve; });
  });
  await page.locator("#font-trigger").click();
  await expect(page.locator("#font-style-value")).toHaveText("Regular");
  await page.evaluate(() => {
    window.__TP_TEST__.fontListError = true;
    window.__TP_TEST__.finishFontList();
  });
  await expect(page.getByRole("button", { name: "读取字体失败，点击重试" })).toBeVisible();
  await expect(page.locator("#font-style-value")).toHaveText("Regular");
  await page.evaluate(() => {
    window.__TP_TEST__.fontListError = false;
    window.__TP_TEST__.systemFonts = [{ name: "微软雅黑", aliases: [], styles: [
      { name: "Regular", family: "微软雅黑", weight: 400, stretch: 100, style: "normal" },
    ] }];
  });
  await page.getByRole("button", { name: "读取字体失败，点击重试" }).click();
  await expect(page.locator("#font-style-trigger")).toHaveCount(0);
});

test("font controls fit selected labels, truncate at half a row and resize menus after catalog refresh", async ({ page }) => {
  await openSettings(page);
  const longFamily = "A particularly long installed font family with many words";
  const longStyle = "An unusually long style name for a real installed face";
  await page.evaluate(({ longFamily, longStyle }) => {
    window.__TP_TEST__.systemFonts = ["Arial", "Medium length font family", longFamily].map(name => ({ name, aliases: [name], styles: [
      { name: "Regular", family: name, weight: 400, stretch: 100, style: "normal", aliases: [] },
      { name: longStyle, family: name, weight: 300, stretch: 100, style: "normal", aliases: [] },
    ] }));
  }, { longFamily, longStyle });
  await selectFont(page, "Arial");
  const shortWidth = (await page.locator("#font-trigger").boundingBox()).width;
  expect(shortWidth).toBeLessThan(120);
  const checkTruncated = async (selector) => {
    const sizes = await page.locator(selector).evaluate(element => ({
      width: element.getBoundingClientRect().width,
      row: element.closest(".setting-field").getBoundingClientRect().width,
      clipped: element.firstElementChild.scrollWidth > element.firstElementChild.clientWidth,
      ellipsis: getComputedStyle(element.firstElementChild).textOverflow,
    }));
    expect(sizes.width).toBeLessThanOrEqual(sizes.row / 2);
    expect(sizes.clipped).toBe(true);
    expect(sizes.ellipsis).toBe("ellipsis");
  };
  await selectFont(page, longFamily);
  await checkTruncated("#font-trigger");
  expect((await page.locator("#font-trigger").boundingBox()).width).toBeGreaterThan(shortWidth);
  await selectStyle(page, longStyle);
  await checkTruncated("#font-style-trigger");
  await page.locator("#font-style-trigger").click();
  const wideStyle = (await page.locator("#font-style-menu").boundingBox()).width;
  expect(wideStyle).toBeGreaterThan(200);
  await page.locator("#font-style-menu").press("Escape");
  // Keep the selected font installed; remove only an unselected longest name.
  await selectFont(page, "Arial");
  await page.locator("#font-trigger").click();
  const wide = (await page.locator("#font-menu").boundingBox()).width;
  await page.locator("#font-search").fill("Arial");
  expect((await page.locator("#font-menu").boundingBox()).width).toBe(wide);
  await page.locator("#font-search").press("Escape");
  await page.evaluate(() => { window.__TP_TEST__.systemFonts.pop(); });
  await page.locator("#font-trigger").click();
  await expect(page.getByRole("option", { name: longFamily, exact: true })).toHaveCount(0);
  await expect.poll(async () => (await page.locator("#font-menu").boundingBox()).width).toBeLessThan(wide);
  const medium = (await page.locator("#font-menu").boundingBox()).width;
  await page.locator("#font-search").press("Escape");
  await page.evaluate(() => { window.__TP_TEST__.systemFonts.pop(); window.__TP_TEST__.systemFonts[0].styles.pop(); });
  await page.locator("#font-trigger").click();
  await expect(page.getByRole("option", { name: "Medium length font family", exact: true })).toHaveCount(0);
  await expect.poll(async () => (await page.locator("#font-menu").boundingBox()).width).toBeLessThan(medium);
  await page.setViewportSize({ width: 340, height: 500 });
  const bounds = await page.locator("#font-menu").boundingBox();
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(340);
});

test("font search finds Chinese names by pinyin initials, full pinyin and English aliases without saving until confirmation", async ({ page }) => {
  await openSettings(page);
  await page.locator("#font-trigger").click();
  const search = page.getByRole("combobox", { name: "搜索字体" });
  for (const query of ["XW", "xw", "XWWK", "xiawu", "LXGW", "霞鹜"]) {
    await search.fill(query);
    await expect(page.locator("#font-list [role=option]")).toHaveText(["霞鹜文楷"]);
    await expect(page.locator("#font-value")).toHaveText("系统默认");
    expect(await page.evaluate(() => window.__TP_TEST__.calls.filter(call => call.name === "save_settings").length)).toBe(0);
  }
  await search.fill("WRYH");
  await expect(page.locator("#font-list [role=option]")).toHaveText(["微软雅黑"]);
  await search.fill("not-a-font");
  await expect(page.locator("#font-list [role=option]")).toHaveCount(0);
  await search.fill("");
  await expect(page.getByRole("option", { name: "系统默认", exact: true })).toBeVisible();
  await search.fill("XW");
  await search.press("Enter");
  await expect(page.locator("#font-value")).toHaveText("霞鹜文楷");
  await expect(page.locator("#font-menu")).toBeHidden();
  await expect(page.locator("#font-style-value")).toHaveText("Regular");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("霞鹜文楷");
});
async function selectStyle(page, name) {
  await page.locator("#font-style-trigger").click();
  await page.locator("#font-style-menu").getByRole("option", { name, exact: true }).click();
}

test("family list merges YaHei faces and exposes installed styles below the family", async ({ page }) => {
  await openSettings(page);
  await page.locator("#font-trigger").click();
  await page.locator("#font-search").fill("微软雅黑");
  await expect(page.locator("#font-list [role=option]")).toHaveText(["微软雅黑"]);
  await page.getByRole("option", { name: "微软雅黑", exact: true }).click();
  await expect(page.locator("#font-style-value")).toHaveText("Regular");
  await expect(page.locator("#settings-appearance .setting-label")).toHaveText(["主题配色", "字体", "样式"]);
  await page.locator("#font-style-trigger").click();
  await expect(page.locator("#font-style-menu [role=option]")).toHaveText(["Regular", "Light", "Bold"]);
  await page.locator("#font-style-menu").getByRole("option", { name: "Light", exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontStyle.weight)).toBe(290);
  await expect(page.locator("html")).toHaveCSS("font-weight", "290");
  await expect(page.locator("#font-trigger")).toHaveCSS("font-weight", "290");
  expect(await fontOf(page, "html")).toMatch(/^"微软雅黑 Light", 微软雅黑,/);
  await selectStyle(page, "Bold");
  await expect(page.locator("html")).toHaveCSS("font-weight", "700");
  await selectFont(page, "Arial");
  await expect(page.locator("#font-style-value")).toHaveText("Regular");
  await expect(page.locator("html")).toHaveCSS("font-weight", "400");
  await selectFont(page, "等线");
  await expect(page.locator("#font-style-trigger")).toHaveCount(0);
  await selectFont(page, "");
  await expect(page.locator("#font-style-trigger")).toHaveCount(0);
});

test("style menu supports keyboard confirmation, Escape, and a small-window scroll", async ({ page }) => {
  await page.setViewportSize({ width: 340, height: 400 });
  await openSettings(page); await selectFont(page, "Arial");
  await page.locator("#font-style-trigger").focus(); await page.keyboard.press("ArrowDown");
  await expect(page.locator("#font-style-menu").getByRole("option", { name: "Regular", exact: true })).toBeFocused();
  await page.keyboard.press("End");
  expect(await page.locator("#font-style-menu").getByRole("option", { name: "Italic", exact: true }).evaluate(element => {
    const rect = element.getBoundingClientRect(), menu = element.parentElement.getBoundingClientRect();
    return rect.top >= menu.top && rect.bottom <= menu.bottom && menu.bottom <= innerHeight;
  })).toBe(true);
  await page.keyboard.press("Enter");
  await expect(page.locator("#font-style-value")).toHaveText("Italic");
  await expect(page.locator("html")).toHaveCSS("font-style", "italic");
  await page.locator("#font-style-trigger").click();
  await page.keyboard.press("Escape");
  await expect(page.locator("#font-style-menu")).toBeHidden();
  await expect(page.locator("#settings")).toBeVisible();
  await expect(page.locator("#font-style-trigger")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("#settings")).toBeHidden();
});

test("style saves roll back on failure and the latest fast choice survives closing settings", async ({ page }) => {
  await openSettings(page); await selectFont(page, "微软雅黑"); await selectStyle(page, "Light");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontStyle.name)).toBe("Light");
  await page.evaluate(() => window.__TP_TEST__.settingsError = "write failed");
  await selectStyle(page, "Bold");
  await expect(page.locator("#toast")).toContainText("write failed");
  await expect(page.locator("#font-style-value")).toHaveText("Light");
  await expect(page.locator("html")).toHaveCSS("font-weight", "290");
  await page.evaluate(() => { delete window.__TP_TEST__.settingsError; window.__TP_TEST__.themeSaveDelay = 450; });
  await selectStyle(page, "Bold"); await selectStyle(page, "Regular");
  await page.locator("#settings-button").click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontStyle.name)).toBe("Regular");
  await expect(page.locator("html")).toHaveCSS("font-weight", "400");
  await page.evaluate(() => { window.__TP_TEST__.hide(); window.__TP_TEST__.show(); });
  await expect(page.locator("html")).toHaveCSS("font-weight", "400");
});

test("a saved legacy Light family migrates to the family plus Light style", async ({ page }) => {
  await page.addInitScript(() => window.__TP_TEST_INITIAL_FONT__ = "微软雅黑 Light");
  await openSettings(page);
  await expect(page.locator("#font-value")).toHaveText("微软雅黑");
  await expect(page.locator("#font-style-value")).toHaveText("Light");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("微软雅黑");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontStyle.weight)).toBe(290);
  await expect(page.locator("html")).toHaveCSS("font-weight", "290");
});

test("both preview windows restore and synchronize the family and selected style", async ({ page }) => {
  await page.addInitScript(() => {
    window.__TP_TEST_INITIAL_FONT__ = "微软雅黑";
    window.__TP_TEST_INITIAL_FONT_STYLE__ = { name: "Light", family: "微软雅黑 Light", weight: 290, stretch: 100, style: "normal" };
  });
  for (const entry of ["preview", "large-preview"]) {
    await page.goto(`/tests/fixtures/${entry}.html`);
    await expect(page.locator("html")).toHaveCSS("font-weight", "290");
    await page.evaluate(() => window.__TP_TEST__.emit("font-changed", {
      family: "Arial", style: { name: "Bold Italic", family: "Arial", weight: 700, stretch: 100, style: "italic" },
    }));
    await expect(page.locator("html")).toHaveCSS("font-weight", "700");
    await expect(page.locator("html")).toHaveCSS("font-style", "italic");
  }
});

test("font setting follows theme, searches system families and applies globally with persistence", async ({ page }) => {
  await openSettings(page);
  const fields = page.locator("#settings-appearance .setting-field");
  await expect(fields).toHaveCount(2);
  await expect(fields.nth(0)).toContainText("主题配色");
  await expect(fields.nth(1)).toContainText("字体");
  await page.locator("#font-label").click();
  await expect(page.locator("#font-menu")).toBeHidden();
  await selectFont(page, "Consolas");
  await expect(page.locator("#font-value")).toHaveText("Consolas");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("Consolas");
  for (const selector of ["html", "#font-trigger", "#shortcut", '.record[data-id="text"] .record-text']) {
    expect(await fontOf(page, selector)).toMatch(/^Consolas,/);
  }
  await page.locator("#retain-search-input").uncheck();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.retainSearchInput)).toBe(false);
  expect(await fontOf(page, "html")).toMatch(/^Consolas,/);
  await page.evaluate(() => { window.__TP_TEST__.hide(); window.__TP_TEST__.idle(); });
  await page.evaluate(() => window.__TP_TEST__.show());
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  expect(await fontOf(page, "html")).toMatch(/^Consolas,/);
  await page.locator("#settings-button").click();
  await selectFont(page, "");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("");
  expect(await fontOf(page, "html")).toMatch(/^"Segoe UI",/);
});

test("keyboard search commits only on Enter, Escape closes the list first, and clicks outside dismiss it", async ({ page }) => {
  await openSettings(page);
  await page.locator("#font-trigger").focus(); await page.keyboard.press("ArrowDown");
  await expect(page.locator("#font-search")).toBeFocused();
  await page.locator("#font-search").fill("consol");
  await expect(page.locator("#font-list [role=option]")).toHaveText(["Consolas"]);
  expect(await page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("");
  await page.keyboard.press("Enter");
  await expect(page.locator("#font-menu")).toBeHidden();
  await expect(page.locator("#font-trigger")).toBeFocused();
  await expect(page.locator("#font-value")).toHaveText("Consolas");
  await page.locator("#font-trigger").click(); await page.keyboard.press("Escape");
  await expect(page.locator("#settings")).toBeVisible();
  await expect(page.locator("#font-menu")).toBeHidden();
  await page.locator("#font-trigger").click();
  await page.locator("#font-search").fill("no such font");
  await expect(page.getByRole("status").filter({ hasText: "没有匹配的字体" })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.locator("#font-menu")).toBeVisible();
  await page.locator("#font-label").click();
  await expect(page.locator("#font-menu")).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.locator("#settings")).toBeHidden();
});

test("small windows keep a long font list inside the viewport and scroll the active option", async ({ page }) => {
  await page.setViewportSize({ width: 340, height: 400 });
  await openSettings(page); await page.locator("#font-trigger").click();
  await page.locator("#font-search").fill("Test Font");
  await expect(page.locator("#font-list [role=option]")).toHaveCount(120);
  await page.keyboard.press("ArrowUp");
  const visible = await page.locator("#font-search").evaluate(input => {
    const active = document.getElementById(input.getAttribute("aria-activedescendant"));
    const row = active.getBoundingClientRect(), list = document.querySelector("#font-list").getBoundingClientRect();
    const menu = document.querySelector("#font-menu").getBoundingClientRect();
    return row.top >= list.top && row.bottom <= list.bottom && menu.left >= 0 && menu.right <= innerWidth && menu.bottom <= innerHeight;
  });
  expect(visible).toBe(true);
  await page.keyboard.press("Enter");
  await expect(page.locator("#font-value")).toHaveText("Test Font 119");
});

test("font saves restore the last saved selection on failure and survive fast consecutive choices", async ({ page }) => {
  await openSettings(page);
  await selectFont(page, "Arial");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("Arial");
  await page.evaluate(() => window.__TP_TEST__.settingsError = "write failed");
  await selectFont(page, "Consolas");
  await expect(page.locator("#toast")).toContainText("write failed");
  await expect(page.locator("#font-value")).toHaveText("Arial");
  expect(await fontOf(page, "html")).toMatch(/^Arial,/);
  await page.evaluate(() => { delete window.__TP_TEST__.settingsError; window.__TP_TEST__.themeSaveDelay = 500; });
  await selectFont(page, "Consolas"); await selectFont(page, "Segoe UI");
  await page.locator("#settings-button").click();
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.state.settings.fontFamily)).toBe("Segoe UI");
  expect(await fontOf(page, "html")).toMatch(/^"Segoe UI",/);
});

test("font enumeration failure supports retry and keeps the default usable", async ({ page }) => {
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await page.evaluate(() => window.__TP_TEST__.fontListError = true);
  await page.locator("#settings-button").click(); await page.locator("#font-trigger").click();
  await expect(page.getByRole("button", { name: "读取字体失败，点击重试" })).toBeVisible();
  await expect(page.getByRole("option", { name: "系统默认", exact: true })).toBeVisible();
  await page.evaluate(() => window.__TP_TEST__.fontListError = false);
  await page.getByRole("button", { name: "读取字体失败，点击重试" }).click();
  await page.locator("#font-search").fill("等线");
  await expect(page.getByRole("option", { name: "等线", exact: true })).toBeVisible();
});

test("main and both preview entries restore a saved font and previews follow live changes", async ({ page }) => {
  await page.addInitScript(() => window.__TP_TEST_INITIAL_FONT__ = "Consolas");
  for (const entry of ["panel", "preview", "large-preview"]) {
    await page.goto(`/tests/fixtures/${entry}.html`);
    await expect.poll(() => fontOf(page, "html")).toMatch(/^Consolas,/);
    if (entry === "panel") continue;
    await page.evaluate(() => window.__TP_TEST__.emit("font-changed", "Arial"));
    await expect.poll(() => fontOf(page, "html")).toMatch(/^Arial,/);
    await page.evaluate(() => window.__TP_TEST__.emit("font-changed", ""));
    await expect.poll(() => fontOf(page, "html")).toMatch(/^"Segoe UI",/);
  }
});

test("changing font refits existing search excerpts to keep the keyword visible", async ({ page }) => {
  await page.addInitScript(() => window.__TP_TEST_INITIAL_FONT__ = "Arial");
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await page.evaluate(() => window.__TP_TEST__.change([
    window.__TP_TEST__.makeRecord("text", "text", "i".repeat(120) + "needle" + "i".repeat(120)),
  ]));
  await page.locator("#search").fill("needle");
  const excerpt = page.locator('.record[data-id="text"] .record-text');
  await expect(excerpt.locator(".search-hit")).toHaveText("needle");
  const before = (await excerpt.innerText()).length;
  await page.locator("#settings-button").click();
  await selectFont(page, "Consolas");
  await page.locator("#settings-button").click();
  await expect.poll(async () => (await excerpt.innerText()).length).toBeLessThan(before);
  await expect(excerpt.locator(".search-hit")).toHaveText("needle");
  expect(await excerpt.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
});

test("file preview text uses the selected font and a live change remeasures the preview", async ({ page }) => {
  await page.addInitScript(() => window.__TP_TEST_INITIAL_FONT__ = "Arial");
  await page.goto("/tests/fixtures/preview.html?window=preview");
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.some(call => call.args.event === "preview-ready"))).toBe(true);
  await page.evaluate(() => window.__TP_TEST__.emit("preview-content", {
    id: "file", kind: "file", body: "", files: [{ original: "C:\\Files\\notes.md" }], groupName: null,
    preview: true, status: "ready", width: 210, maxHeight: 580, generation: 1, search: "",
  }));
  await expect(page.locator(".file-text-content")).toBeVisible();
  expect(await fontOf(page, ".file-text-content")).toMatch(/^Arial,/);
  await page.evaluate(() => window.__TP_TEST__.emit("font-changed", "Consolas"));
  await expect.poll(() => fontOf(page, ".file-text-content")).toMatch(/^Consolas,/);
  // A narrow text card changes height when a different local font wraps its lines.
  await page.evaluate(() => window.__TP_TEST__.emit("preview-content", {
    id: "text", kind: "text", body: "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii",
    files: [], groupName: null, preview: true, status: "ready", width: 210, maxHeight: 580, generation: 2, search: "",
  }));
  await expect(page.locator(".text-content")).toBeVisible();
  const before = await page.locator(".card").evaluate(element => element.offsetHeight);
  await page.evaluate(() => window.__TP_TEST__.emit("font-changed", "Arial"));
  await expect.poll(() => page.locator(".card").evaluate(element => element.offsetHeight)).not.toBe(before);
  const height = await page.locator(".card").evaluate(element => element.offsetHeight);
  await expect.poll(() => page.evaluate(() => window.__TP_TEST__.calls.findLast(call => call.args.event === "preview-measured")?.args.payload.height)).toBe(height);
});
