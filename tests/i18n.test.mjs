import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

test("system language resolves all Chinese variants and falls back to English", () => {
  const { resolveLanguage } = loadTs("src/i18n.ts");
  for (const locale of ["zh-CN", "zh-TW", "zh-HK", "zh", "ZH-hans"]) assert.equal(resolveLanguage("system", locale), "zh-CN");
  for (const locale of ["en-US", "de-DE", "fr", "ja-JP", ""]) assert.equal(resolveLanguage("system", locale), "en");
  assert.equal(resolveLanguage("zh-CN", "en-US"), "zh-CN");
  assert.equal(resolveLanguage("en", "zh-CN"), "en");
});

test("language changes update subscribers and interpolation does not reinterpret user data", () => {
  const document = { documentElement: { lang: "" } };
  const { applyLanguage, t, subscribeLanguage } = loadTs("src/i18n.ts", { document, navigator: { language: "zh-CN" } });
  let calls = 0;
  const unsubscribe = subscribeLanguage(() => calls++);
  assert.equal(t("设置"), "设置");
  applyLanguage({ preference: "en", resolved: "en" });
  assert.equal(t("设置"), "Settings");
  assert.equal(t("启动失败：{0}", "原文 {1} <text>"), "Startup failed: 原文 {1} <text>");
  assert.equal(document.documentElement.lang, "en");
  assert.equal(calls, 1);
  unsubscribe();
  applyLanguage({ preference: "system", resolved: "zh-CN" });
  assert.equal(calls, 1);
});

test("language startup and refresh ignore stale results after a live change", async () => {
  let handler, resolveRead, languagechange;
  const changes = [];
  const { startLanguageSync } = loadTs("src/services/language.ts", {
    window: { addEventListener: (_, fn) => { languagechange = fn; }, removeEventListener() {} },
  }, {
    "@tauri-apps/api/event": { listen: async (_, fn) => { handler = fn; return () => {}; } },
    "./desktop": { call: () => new Promise(resolve => { resolveRead = resolve; }) },
    "../i18n": { applyLanguage: state => changes.push(state.resolved), refreshSystemLanguage: () => changes.push("fallback") },
  });
  const start = startLanguageSync();
  await new Promise(setImmediate);
  handler({ payload: { preference: "en", resolved: "en" } });
  resolveRead({ preference: "zh-CN", resolved: "zh-CN" });
  const dispose = await start;
  assert.deepEqual(changes, ["en"]);
  languagechange();
  handler({ payload: { preference: "zh-CN", resolved: "zh-CN" } });
  resolveRead({ preference: "en", resolved: "en" });
  await new Promise(setImmediate);
  assert.deepEqual(changes, ["en", "zh-CN"]);
  dispose();
});


test("English catalog preserves every interpolation placeholder", () => {
  const { english } = loadTs("src/translations.ts");
  for (const [source, translated] of Object.entries(english)) {
    const slots = text => [...text.matchAll(/\{\d+\}/g)].map(match => match[0]).sort();
    assert.deepEqual(slots(translated), slots(source), source);
    assert.ok(translated.trim(), source);
  }
});
