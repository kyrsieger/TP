import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { regularFontStyle, defaultFontStyle, resolveFontSelection } = loadTs("src/domain/fonts.ts");
const lightStyle = { name: "Light", family: "微软雅黑 Light", weight: 290, stretch: 100, style: "normal", aliases: ["微软雅黑 Light", "Microsoft YaHei Light"] };

test("font family is a single quoted CSS value, with safe defaults and punctuation escaping", () => {
  const values = new Map();
  const { applyFont } = loadTs("src/services/fonts.ts", { document: { documentElement: { style: {
    setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key),
  } } } }, { "./desktop": {} });
  applyFont("Microsoft YaHei");
  assert.match(values.get("--app-font"), /^"Microsoft YaHei","Segoe UI"/);
  applyFont('Font"\\,serif');
  assert.ok(values.get("--app-font").startsWith('"Font\\"\\\\,serif",'));
  for (const value of ["", undefined, null, "bad\nfont", "a".repeat(257)]) {
    applyFont(value); assert.equal(values.has("--app-font"), false);
  }
});

function setup(save = async () => ({ status: "saved" })) {
  const { SettingsController } = loadTs("src/state/settings.ts");
  const writes = [], errors = [];
  const history = { value: { active: true, settings: { fontFamily: "", theme: "indigo" } },
    commitSettings(settings) { this.value.settings = settings; } };
  const controller = new SettingsController(history, async (_name, args) => {
    writes.push(args.value); return save(args.value, writes.length);
  }, { toast: error => errors.push(error) });
  return { controller, history, writes, errors };
}

test("font selections apply immediately, save in order with other settings, and can restore default", async () => {
  const { controller, history, writes } = setup();
  controller.selectFont("Consolas");
  assert.equal(controller.value.fontDraft.family, "Consolas");
  await controller.save({ theme: "violet" });
  assert.equal(history.value.settings.fontFamily, "Consolas");
  assert.equal(history.value.settings.theme, "violet");
  assert.equal(controller.value.fontDraft, null);
  assert.equal(writes[1].fontFamily, "Consolas");
  controller.selectFont(""); await controller.save({});
  assert.equal(history.value.settings.fontFamily, "");
});

test("a slow older font save cannot replace the latest choice, even when settings close", async () => {
  let finish;
  const pending = new Promise(resolve => { finish = resolve; });
  const { controller, history } = setup(async (_value, count) => count === 1 ? pending : { status: "saved" });
  controller.selectFont("Arial"); await Promise.resolve();
  controller.selectFont("Consolas"); controller.reset();
  finish({ status: "saved" }); await Promise.resolve(); await Promise.resolve();
  assert.equal(controller.value.fontDraft.family, "Consolas");
  await controller.save({});
  assert.equal(history.value.settings.fontFamily, "Consolas");
  assert.equal(controller.value.fontDraft, null);
});

test("failed font saves restore the last persisted font and report the error", async () => {
  const { controller, history, errors } = setup(async () => { throw Error("disk full"); });
  history.value.settings.fontFamily = "Arial";
  controller.selectFont("Consolas"); await controller.save({});
  assert.equal(controller.value.fontDraft, null);
  assert.equal(history.value.settings.fontFamily, "Arial");
  assert.match(errors[0], /disk full/);
});

test("preview font startup ignores stale reads and failed reads after a live change", async () => {
  for (const failed of [false, true]) {
    let listener, finish, reject;
    const pending = new Promise((resolve, fail) => { finish = resolve; reject = fail; });
    const values = new Map();
    const { startFontSync } = loadTs("src/services/fonts.ts", { document: { documentElement: { style: {
      setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key),
    } } } }, {
      "@tauri-apps/api/event": { listen: async (_name, callback) => { listener = callback; return () => {}; } },
      "./desktop": { call: () => pending },
    });
    const startup = startFontSync(); await Promise.resolve();
    listener({ payload: "Consolas" });
    if (failed) reject(Error("read failed")); else finish("Arial");
    await startup;
    assert.match(values.get("--app-font"), /^"Consolas",/);
    listener({ payload: "" }); assert.equal(values.has("--app-font"), false);
  }
});

test("family metadata resolves legacy Light names and prefers Regular without inventing missing styles", () => {
  const regular = { ...regularFontStyle, family: "微软雅黑" };
  const group = { name: "微软雅黑", aliases: ["微软雅黑", "微软雅黑 Light", "Microsoft YaHei Light"], styles: [lightStyle, regular] };
  assert.equal(defaultFontStyle(group).name, "Regular");
  for (const legacy of ["微软雅黑 Light", "Microsoft YaHei Light"]) {
    const resolved = resolveFontSelection([group], legacy, regularFontStyle);
    assert.equal(resolved.family, "微软雅黑"); assert.equal(resolved.style.weight, 290);
  }
  assert.equal(defaultFontStyle({ ...group, styles: [lightStyle] }).name, "Light");
  const separate = resolveFontSelection([group], "Unrelated Light", regularFontStyle);
  assert.equal(separate.family, "Unrelated Light");
});

test("style and family save together, switching family resets Regular, and failed styles restore the persisted choice", async () => {
  const good = setup();
  good.controller.selectFont("微软雅黑", lightStyle);
  assert.equal(good.controller.value.fontDraft.style.weight, 290);
  await good.controller.save({ theme: "violet" });
  assert.equal(good.history.value.settings.fontFamily, "微软雅黑");
  assert.equal(good.history.value.settings.fontStyle.name, "Light");
  assert.equal(good.writes[1].fontStyle.weight, 290);
  good.controller.selectFont("Arial"); await good.controller.save({});
  assert.equal(good.history.value.settings.fontStyle.name, "Regular");
  assert.equal(good.history.value.settings.fontStyle.weight, 400);
  const bad = setup(async () => { throw Error("write failed"); });
  bad.history.value.settings.fontFamily = "微软雅黑";
  bad.history.value.settings.fontStyle = lightStyle;
  bad.controller.selectFont("微软雅黑", { ...lightStyle, name: "Bold", weight: 700 });
  await bad.controller.save({});
  assert.equal(bad.controller.value.fontDraft, null);
  assert.equal(bad.history.value.settings.fontStyle.weight, 290);
});

test("applying a style uses its actual render family, weight and slope, and resetting clears them", () => {
  const values = new Map();
  const { applyFont } = loadTs("src/services/fonts.ts", { document: { documentElement: { style: {
    setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key),
  } } } }, { "./desktop": {} });
  applyFont({ family: "微软雅黑", style: lightStyle });
  assert.match(values.get("--app-font"), /^"微软雅黑 Light","微软雅黑",/);
  assert.equal(values.get("--app-font-weight"), "290");
  applyFont({ family: "Example", style: { ...regularFontStyle, style: "italic", stretch: 75 } });
  assert.equal(values.get("--app-font-style"), "italic");
  assert.equal(values.get("--app-font-stretch"), "75%");
  applyFont({ family: "", style: lightStyle });
  for (const name of ["--app-font", "--app-font-weight", "--app-font-style", "--app-font-stretch"]) assert.equal(values.has(name), false);
});

test("live preview style survives a stale startup response and can return to Regular", async () => {
  let listener, finish;
  const pending = new Promise(resolve => { finish = resolve; });
  const values = new Map();
  const { startFontSync } = loadTs("src/services/fonts.ts", { document: { documentElement: { style: {
    setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key),
  } } } }, {
    "@tauri-apps/api/event": { listen: async (_name, callback) => { listener = callback; return () => {}; } },
    "./desktop": { call: () => pending },
  });
  const startup = startFontSync(); await Promise.resolve();
  listener({ payload: { family: "微软雅黑", style: lightStyle } });
  finish({ family: "Arial", style: regularFontStyle }); await startup;
  assert.equal(values.get("--app-font-weight"), "290");
  listener({ payload: { family: "Arial", style: regularFontStyle } });
  assert.equal(values.get("--app-font-weight"), "400");
});
