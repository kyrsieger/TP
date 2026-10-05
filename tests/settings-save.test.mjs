import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const functions = ["saveSetting", "clearHiddenSearch"].map(name => source.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === name).getText(source));
const { outputText } = ts.transpileModule(functions.join("\n"), { compilerOptions: { target: ts.ScriptTarget.ES2020 } });

function setup(save, confirm = () => false) {
  const effects = [];
  const elements = new Map([["#search", { value: "kept" }], ["#clear-search", { hidden: false }]]);
  const original = { maxOrdinary: 100, retentionDays: 3, shortcut: "", rememberPosition: false };
  const context = {
    state: { settings: original }, panelActive: true, settingsSave: Promise.resolve(), pendingSettings: null, activeShortcut: "", shortcutDraft: null, shortcutError: null,
    call: async (name, args) => {
      effects.push([name, args.value, args.confirmedCount]);
      const result = await save(args.value, args.confirmedCount);
      return typeof result === "boolean" ? (result ? { status: "saved" } : { status: "confirmation_required", count: 50 }) : result;
    },
    confirmSettingsCleanup: count => { effects.push(["confirm", count]); return confirm(count); },
    renderSettings: () => effects.push("render-settings"), toast: error => effects.push(["toast", error]),
    placePanel: async () => effects.push("place-panel"),
    search: "kept", document: { querySelector: selector => elements.get(selector) },
    savePanelSession: () => effects.push(["session", context.search]),
  };
  runInNewContext(outputText, context);
  return { context, effects, original, elements };
}

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

const dialogDeclaration = source.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === "confirmSettingsCleanup");
const { outputText: dialogScript } = ts.transpileModule(dialogDeclaration.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2020 } });

for (const action of ["confirm", "cancel", "escape"]) {
  test(`the styled modal resolves ${action} and removes itself`, async () => {
    const buttons = new Map([["[data-cancel]", {}], ["[data-confirm]", {}]]);
    const listeners = new Map();
    const dialog = {
      returnValue: "", removed: false, open: false,
      setAttribute() {}, querySelector: selector => buttons.get(selector),
      addEventListener: (name, handler) => listeners.set(name, handler),
      showModal() { this.open = true; },
      close(value = "") { this.returnValue = value; this.open = false; listeners.get("close")(); },
      remove() { this.removed = true; },
    };
    const context = { document: { createElement: () => dialog }, root: { append() {} } };
    runInNewContext(dialogScript, context);
    const result = context.confirmSettingsCleanup(7);
    assert.equal(dialog.open, true);
    assert.match(dialog.innerHTML, /<strong>7<\/strong>/);
    if (action === "escape") dialog.close();
    else buttons.get(action === "confirm" ? "[data-confirm]" : "[data-cancel]").onclick();
    assert.equal(await result, action === "confirm");
    assert.equal(dialog.removed, true);
  });
}
