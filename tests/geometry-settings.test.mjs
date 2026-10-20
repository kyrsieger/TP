import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

test("a delayed geometry save never overwrites a newly enabled clipboard preference", async () => {
  let timer, finish;
  const calls = [];
  const pending = new Promise(resolve => { finish = resolve; });
  const { GeometryController } = loadTs("src/services/geometry.ts", {
    window: { setTimeout(callback) { timer = callback; return 1; }, clearTimeout() {} },
  }, {
    "@tauri-apps/api/window": { PhysicalPosition: class {}, PhysicalSize: class {} },
    "./desktop": { isMacOS: false, call: async (name, args) => { calls.push({ name, args }); await pending; } },
  });
  const history = {
    value: { active: true, settings: { removeTrailingNewlines: false, theme: "indigo" } },
    commitSettings(value) { this.value.settings = value; },
  };
  const controller = new GeometryController(history, {
    outerPosition: async () => ({ x: 100, y: 200 }),
    innerSize: async () => ({ width: 420, height: 600 }),
  }, () => {});
  controller.scheduleGeometrySave();
  const saving = timer();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [{ name: "save_panel_geometry", args: { value: { panelX: 100, panelY: 200, panelWidth: 420, panelHeight: 600 } } }]);
  history.commitSettings({ removeTrailingNewlines: true, theme: "forest-mist" });
  finish(); await saving;
  assert.equal(history.value.settings.removeTrailingNewlines, true);
  assert.equal(history.value.settings.theme, "forest-mist");
  assert.equal(history.value.settings.panelWidth, 420);
});
