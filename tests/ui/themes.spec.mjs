import { test, expect } from "@playwright/test";
import { loadTs } from "../load-ts.mjs";
const { sortedThemes, themeSwatches } = loadTs("src/domain/themes.ts");
const lastTheme = sortedThemes.at(-1).id;

test.beforeEach(async({page})=>{
  await page.goto("/tests/fixtures/panel.html");
  await expect(page.locator('.record[data-id="text"]')).toBeVisible();
  await page.locator("#settings-button").click();
  await page.locator("#theme-trigger").click();
});

test("dropdown shows the computed order and seven approved colors; hover changes without clicking",async({page})=>{
  const options=page.locator('#theme-menu [role=option]');
  await expect(options).toHaveCount(11);
  await expect(page.locator('#theme-menu .theme-name')).toHaveText(sortedThemes.map(t=>t.name));
  for(const theme of sortedThemes) {
    const option=page.locator(`[data-theme-option="${theme.id}"]`);
    await expect(option.locator('.theme-strip span')).toHaveCount(7);
    const colors=await option.locator('.theme-strip span').evaluateAll(elements=>elements.map(el=>el.style.backgroundColor));
    const toRgb=color=>`rgb(${color.slice(1).match(/../g).map(v=>parseInt(v,16)).join(', ')})`;
    expect(colors).toEqual(themeSwatches(theme).map(toRgb));
    await option.hover();
    await expect(page.locator('html')).toHaveAttribute('data-theme',theme.id);
    await expect(page.locator('#theme-value')).toHaveText(theme.name);
    await expect(page.locator('#theme-menu')).toBeVisible();
    await expect(option).toHaveAttribute('aria-selected','true');
    expect(await page.locator('.shell').evaluate(el=>getComputedStyle(el).backgroundColor)).toBe(toRgb(theme.bg));
  }
  await expect.poll(()=>page.evaluate(()=>window.__TP_TEST__.state.settings.theme)).toBe(lastTheme);
  await page.mouse.move(5,5);
  await page.keyboard.press('Escape');
  await expect(page.locator('#theme-menu')).toBeHidden();
  await expect(page.locator('#settings')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme',lastTheme);
  await page.locator('#settings-button').click();
  await expect(page.locator('#settings')).toBeHidden();
  await expect(page.locator('.record[data-id="text"]')).toBeFocused();
  await page.evaluate(()=>window.__TP_TEST__.hide());
  await page.evaluate(()=>window.__TP_TEST__.show());
  await expect(page.locator('html')).toHaveAttribute('data-theme',lastTheme);
});

test("latest hover survives slow settings saves and other settings writes",async({page})=>{
  await page.evaluate(()=>window.__TP_TEST__.themeSaveDelay=450);
  await page.locator('[data-theme-option="forest-mist"]').hover();
  await expect.poll(()=>page.evaluate(()=>window.__TP_TEST__.calls.filter(c=>c.name==='save_settings').length)).toBe(1);
  await page.locator('[data-theme-option="violet"]').hover();
  await expect(page.locator('html')).toHaveAttribute('data-theme','violet');
  await page.mouse.move(5,5);
  await page.keyboard.press('Escape');
  await page.locator('#retain-search-input').uncheck();
  await expect.poll(()=>page.evaluate(()=>window.__TP_TEST__.state.settings.theme)).toBe('violet');
  await expect.poll(()=>page.evaluate(()=>window.__TP_TEST__.state.settings.retainSearchInput)).toBe(false);
  await expect(page.locator('html')).toHaveAttribute('data-theme','violet');
});

test("failed saves restore the previous theme and keyboard selection works",async({page})=>{
  await page.evaluate(()=>window.__TP_TEST__.settingsError='write failed');
  await page.locator('[data-theme-option="clay"]').hover();
  await expect(page.locator('html')).toHaveAttribute('data-theme','clay');
  await expect(page.locator('#toast')).toContainText('write failed');
  await expect(page.locator('html')).toHaveAttribute('data-theme','indigo');
  await page.evaluate(()=>delete window.__TP_TEST__.settingsError);
  await page.mouse.move(5,5);
  await page.locator('[data-theme-option="indigo"]').focus();
  await page.keyboard.press('Home');
  await expect(page.locator('html')).toHaveAttribute('data-theme','indigo');
  await page.keyboard.press('End');
  await expect(page.locator('html')).toHaveAttribute('data-theme',lastTheme);
  await page.keyboard.press('Enter');
  await expect(page.locator('#theme-menu')).toBeHidden();
  await expect(page.locator('#theme-trigger')).toBeFocused();
  await expect.poll(()=>page.evaluate(()=>window.__TP_TEST__.state.settings.theme)).toBe(lastTheme);
});

test("short windows keep keyboard options visible and Escape closes the dropdown after Tab",async({page})=>{
  await page.setViewportSize({width:420,height:320});
  await page.locator('[data-theme-option="forest-mist"]').focus();
  await page.keyboard.press('End');
  const visible=await page.locator(`[data-theme-option="${lastTheme}"]`).evaluate(el=>{
    const row=el.getBoundingClientRect(),menu=el.parentElement.getBoundingClientRect();
    return row.top>=menu.top && row.bottom<=menu.bottom;
  });
  expect(visible).toBe(true);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Escape');
  await expect(page.locator('#theme-menu')).toBeHidden();
  await expect(page.locator('#settings')).toBeVisible();
});

test("preview entries restore a saved dark theme and react to live theme events",async({page})=>{
  await page.addInitScript(()=>window.__TP_TEST_INITIAL_THEME__='violet');
  for(const entry of ['preview','large-preview']) {
    await page.goto(`/tests/fixtures/${entry}.html`);
    await expect(page.locator('html')).toHaveAttribute('data-theme','violet');
    await page.evaluate(()=>window.__TP_TEST__.emit('theme-changed','forest-mist'));
    await expect(page.locator('html')).toHaveAttribute('data-theme','forest-mist');
    await page.evaluate(()=>window.__TP_TEST__.emit('theme-changed','violet'));
    await expect(page.locator('html')).toHaveAttribute('data-theme','violet');
  }
});
