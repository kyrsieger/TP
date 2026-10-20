import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

const { SettingsController } = loadTs("src/state/settings.ts");
function setup(call) {
  const messages = [];
  const history = { value: { active: true, settings: { backgroundResident: false, shortcut: "" } },
    commitSettings(settings) { this.value.settings = settings; } };
  const controller = new SettingsController(history, call, {
    toast: message => messages.push(message), place: async () => {}, position: async () => ({ x: 0, y: 0 }),
  });
  controller.update({ open: true });
  return { controller, history, messages };
}

test("development release waits for pending settings and prevents duplicate requests", async () => {
  const calls = [];
  let finishSave;
  const pending = new Promise(resolve => { finishSave = resolve; });
  const { controller, history } = setup(async name => {
    calls.push(name);
    if (name === "save_settings") { await pending; return { status: "saved" }; }
  });
  const saving = controller.save({ retainSearchInput: false });
  const releasing = controller.releasePanelNow();
  await controller.releasePanelNow();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(calls, ["save_settings"]);
  finishSave();
  await Promise.all([saving, releasing]);
  assert.deepEqual(calls, ["save_settings", "debug_release_panel"]);
  assert.equal(history.value.settings.retainSearchInput, false);
  assert.equal(controller.value.releasingPanel, false);
});

test("development release protects resident and closed panels", async () => {
  const calls = [];
  const { controller, history, messages } = setup(async name => calls.push(name));
  history.value.settings.backgroundResident = true;
  await controller.releasePanelNow();
  assert.match(messages[0], /后台常驻/);
  history.value.settings.backgroundResident = false;
  controller.update({ open: false });
  await controller.releasePanelNow();
  assert.deepEqual(calls, []);
});

test("failed development release reports its error and allows retry", async () => {
  let calls = 0;
  const { controller, messages } = setup(async () => { calls++; throw new Error("native error"); });
  await controller.releasePanelNow();
  assert.match(messages[0], /释放面板失败.*native error/);
  assert.equal(controller.value.releasingPanel, false);
  await controller.releasePanelNow();
  assert.equal(calls, 2);
});
