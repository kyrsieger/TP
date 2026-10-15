import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { SettingsController } = loadTs("src/state/settings.ts");
function setup(save, confirm = () => false) {
  const effects = [], elements = new Map([["#search", { value: "kept" }], ["#clear-search", { hidden: false }]]);
  const original = { maxOrdinary: 100, retentionDays: 3, shortcut: "", rememberPosition: false };
  const context = { state: { settings: original }, panelActive: true, search: "kept" };
  const history = { get value() { return { settings: context.state.settings, active: context.panelActive }; }, commitSettings(next) {
    context.state.settings = next;
    if (!context.panelActive) {
      if (next.retainSearchInput === false) { context.search = ""; elements.get("#search").value = ""; elements.get("#clear-search").hidden = true; }
      effects.push(["session", context.search]);
    }
  } };
  const controller = new SettingsController(history, async (name,args) => {
    effects.push([name,args.value,args.confirmedCount]);
    const result = await save(args.value,args.confirmedCount);
    return typeof result === "boolean" ? (result ? {status:"saved"} : {status:"confirmation_required",count:50}) : result;
  }, { toast: error=>effects.push(["toast",error]), place: async()=>effects.push("place-panel"), position: async()=>({x:1200,y:200}) });
  let requested = null, revision = 0;
  controller.subscribe(() => {
    const count=controller.value.confirmationCount;
    if (count!==null && count!==requested) {
      requested=count; effects.push(["confirm",count]); Promise.resolve(confirm(count)).then(result=>controller.confirm(result));
    }
    if (count===null) requested=null;
    if (revision!==controller.value.revision && context.state.settings===original) effects.push("render-settings");
    revision=controller.value.revision;
  });
  context.saveSetting = (...args) => { context.settingsSave = controller.save(...args); };
  return { context, effects, original, elements, controller };
}

test("background residency is saved immediately and a failed save preserves the prior choice", async () => {
  const good = setup(() => true); good.original.backgroundResident = false;
  await good.controller.save({ backgroundResident: true });
  assert.equal(good.context.state.settings.backgroundResident, true);
  const bad = setup(() => { throw new Error("save failed"); }); bad.original.backgroundResident = true;
  await bad.controller.save({ backgroundResident: false });
  assert.equal(bad.context.state.settings.backgroundResident, true);
});

test("remember-position changes capture the current coordinates and never move the visible panel", async () => {
  const { controller, context, effects } = setup(() => true);
  await controller.save({ rememberPosition: true });
  assert.equal(context.state.settings.rememberPosition, true);
  assert.equal(context.state.settings.panelX, 1200);
  assert.equal(context.state.settings.panelY, 200);
  await controller.save({ rememberPosition: false });
  assert.equal(context.state.settings.rememberPosition, false);
  assert.equal(context.state.settings.panelX, 1200);
  assert.equal(context.state.settings.panelY, 200);
  assert.ok(!effects.includes("place-panel"));
});

test("local rename shortcut saves without global registration and failed saves preserve the active key and draft", async () => {
  const event = { key: "F3", repeat: false, isComposing: false, keyCode: 0, ctrlKey: false, altKey: false,
    shiftKey: false, metaKey: false, getModifierState: () => false };
  const good = setup(() => true); good.original.renameShortcut = "F2";
  good.controller.captureRenameShortcut(event); await good.controller.save({});
  assert.equal(good.context.state.settings.renameShortcut, "F3");
  assert.equal(good.controller.value.renameShortcutDraft, null);
  assert.ok(good.effects.every(effect => effect[0] !== "register_panel_shortcut"));
  const bad = setup(() => { throw new Error("write failed"); }); bad.original.renameShortcut = "F2";
  bad.controller.captureRenameShortcut(event); await bad.controller.save({});
  assert.equal(bad.context.state.settings.renameShortcut, "F2");
  assert.equal(bad.controller.value.renameShortcutDraft, "F3");
  assert.match(bad.controller.value.renameShortcutError, /write failed/);
});
test("retention saved after the panel hides clears and saves the hidden search", async () => {
  let resolve;
  const saved = new Promise(done => { resolve = done; });
  const { context, effects, elements } = setup(() => saved);
  context.saveSetting({ retainSearchInput: false });
  await new Promise(done => setImmediate(done));
  context.panelActive = false;
  resolve({ status: "saved" });
  await context.settingsSave;
  assert.equal(context.state.settings.retainSearchInput, false);
  assert.equal(context.search, "");
  assert.equal(elements.get("#search").value, "");
  assert.equal(elements.get("#clear-search").hidden, true);
  assert.ok(effects.some(effect => effect[0] === "session" && effect[1] === ""));
});

test("failed retention changes keep the saved preference and search", async () => {
  const { context, original } = setup(() => { throw new Error("save failed"); });
  original.retainSearchInput = true;
  context.saveSetting({ retainSearchInput: false });
  await context.settingsSave;
  assert.equal(context.state.settings.retainSearchInput, true);
  assert.equal(context.search, "kept");
});

for (const patch of [{ maxOrdinary: 50 }, { retentionDays: 1 }]) {
  test(`cancelled cleanup restores the saved ${Object.keys(patch)[0]}`, async () => {
    const { context, effects, original } = setup(() => false);
    context.saveSetting(patch);
    await context.settingsSave;
    assert.equal(context.state.settings, original);
    assert.equal(effects.filter(effect => effect === "render-settings").length, 1);
    assert.equal(effects.filter(effect => effect[0] === "toast").length, 0);
  });
}

test("settings stay unchanged while deletion confirmation is pending", async () => {
  let resolve;
  const pending = new Promise(done => { resolve = done; });
  const { context, original } = setup(() => pending);
  context.saveSetting({ maxOrdinary: 50 });
  await Promise.resolve();
  assert.equal(context.state.settings, original);
  resolve(true);
  await context.settingsSave;
  assert.equal(context.state.settings.maxOrdinary, 50);
});

test("confirmed settings are applied and later saves use them", async () => {
  const { context, effects } = setup(() => true);
  context.saveSetting({ maxOrdinary: 50 });
  context.saveSetting({ retentionDays: 1 });
  await context.settingsSave;
  assert.equal(context.state.settings.maxOrdinary, 50);
  assert.equal(context.state.settings.retentionDays, 1);
  assert.equal(effects[1][1].maxOrdinary, 50);
});

test("input blur during confirmation does not queue a second identical prompt", async () => {
  let resolve;
  const pending = new Promise(done => { resolve = done; });
  const { context, effects, original } = setup(() => pending);
  context.saveSetting({ maxOrdinary: 50 });
  await Promise.resolve();
  context.saveSetting({ maxOrdinary: 50 });
  resolve(false);
  await context.settingsSave;
  assert.equal(context.state.settings, original);
  assert.equal(effects.filter(effect => effect[0] === "save_settings").length, 1);
});

test("a queued change after cancellation uses the previous saved settings", async () => {
  const { context, effects } = setup(value => value.maxOrdinary !== 50);
  context.saveSetting({ maxOrdinary: 50 });
  context.saveSetting({ retentionDays: 1 });
  await context.settingsSave;
  assert.equal(context.state.settings.maxOrdinary, 100);
  assert.equal(context.state.settings.retentionDays, 1);
  assert.equal(effects.filter(effect => effect[0] === "save_settings")[1][1].maxOrdinary, 100);
});

test("failed saves restore the saved value and report the error", async () => {
  const { context, original, effects } = setup(() => { throw new Error("failed"); });
  context.saveSetting({ maxOrdinary: 50 });
  await context.settingsSave;
  assert.equal(context.state.settings, original);
  assert.ok(effects.some(effect => effect[0] === "toast"));
});

test("confirmation sends the approved count only after the user accepts", async () => {
  let resolve;
  const decision = new Promise(done => { resolve = done; });
  const { context, effects, original } = setup((_value, count) => count === 50 ? { status: "saved" } : { status: "confirmation_required", count: 50 }, () => decision);
  context.saveSetting({ maxOrdinary: 50 });
  await new Promise(done => setImmediate(done));
  assert.equal(context.state.settings, original);
  assert.equal(effects.filter(effect => effect[0] === "save_settings").length, 1);
  resolve(true);
  await context.settingsSave;
  assert.equal(context.state.settings.maxOrdinary, 50);
  assert.equal(effects.filter(effect => effect[0] === "save_settings")[1][2], 50);
});

test("an increased deletion count requires a new confirmation", async () => {
  const { context, effects } = setup((_value, count) => count === undefined ? { status: "confirmation_required", count: 2 }
    : count === 2 ? { status: "confirmation_required", count: 3 } : { status: "saved" }, () => true);
  context.saveSetting({ maxOrdinary: 50 });
  await context.settingsSave;
  assert.deepEqual(effects.filter(effect => effect[0] === "confirm"), [["confirm", 2], ["confirm", 3]]);
});

test("a panel hidden before the response does not open a confirmation or apply settings", async () => {
  const { context, effects, original } = setup(() => false);
  context.saveSetting({ maxOrdinary: 50 });
  context.panelActive = false;
  await context.settingsSave;
  assert.equal(context.state.settings, original);
  assert.equal(effects.filter(effect => effect[0] === "confirm").length, 0);
  assert.equal(effects.filter(effect => effect[0] === "save_settings").length, 1);
});
