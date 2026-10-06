import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

function setup() {
  const calls = [], events = [], timers = new Map();
  let timerId = 0;
  const noop = () => {};
  const panelWindow = {
    outerPosition: async () => ({ x: 0, y: 0 }), outerSize: async () => ({ width: 420, height: 600 }), scaleFactor: async () => 1,
  };
  const desktop = { isMacOS: false, panelWindow, call: async (name, args) => { calls.push({ name, args }); } };
  const row = { getBoundingClientRect: () => ({ top: 100 }) };
  const { createApplication } = loadTs("src/application.ts", {
    window: { setTimeout: callback => { timers.set(++timerId, callback); return timerId; }, clearTimeout: id => timers.delete(id) },
  }, {
    "react-dom": { flushSync: callback => callback() },
    "./services/desktop": desktop, "./desktop": desktop,
    "@tauri-apps/api/event": { emitTo: async (label, event, payload) => { events.push({ label, event, payload }); } },
    "@tauri-apps/api/window": { monitorFromPoint: async () => ({ workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1080 } } }) },
    "../preview-windows": { ensurePreviewWindow: async () => {}, initPreviewWindows: async () => noop },
    "./services/geometry": { GeometryController: class { dispose() {} } },
  });
  const app = createApplication({ reset: noop, cancelScroll: noop, row: () => row });
  const record = (id, kind, favorite, groupId) => ({ id, kind, body: id, files: [], favorite, groupId, preview: true, status: "ready", assetPath: null });
  app.history.update({ active: true, settings: {}, snapshot: {
    records: [record("original", "image", false, 2), record("other", "image", true, 1), record("video", "video", true, 1)],
    groups: [{ id: 1, name: "Work" }, { id: 2, name: "Original" }], settings: {},
  }, selectedId: "original", selectedIds: new Set(["original"]) });
  return { app, calls, events, runTimers: () => { const pending = [...timers.values()]; timers.clear(); pending.forEach(callback => callback()); } };
}

for (const [name, change, nextId] of [
  ["search", history => history.browse({ search: "other" }), "other"],
  ["type filter", history => history.browse({ kindFilter: "video" }), "video"],
  ["favorites", history => history.selectGroup("favorite"), "other"],
  ["custom group", history => history.selectGroup("group:1"), "other"],
]) test(`${name} closes a filtered-out large preview and allows subsequent previews`, async () => {
  const { app, calls, events, runTimers } = setup();
  try {
    await app.previews.openLargePreview("original");
    change(app.history);
    assert.equal(app.previews.largePreviewId, null);
    assert.equal(calls.filter(call => call.name === "hide_large_preview").length, 1);
    assert.ok(!app.history.records.some(record => record.id === "original"));
    app.previews.hoverRecord(nextId);
    runTimers();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(events.findLast(event => event.event === "preview-content")?.payload.id, nextId);
    await app.previews.openLargePreview(nextId);
    assert.equal(events.findLast(event => event.event === "large-preview-content")?.payload.id, nextId);
    assert.equal(app.previews.largePreviewId, nextId);
  } finally { app.dispose(); }
});

for (const [name, change] of [
  ["search", history => history.browse({ search: "original" })],
  ["type filter", history => history.browse({ kindFilter: "image" })],
  ["custom group", history => history.selectGroup("group:2")],
]) test(`${name} retains a large preview whose record still matches`, async () => {
  const { app, calls } = setup();
  try {
    await app.previews.openLargePreview("original");
    change(app.history);
    assert.equal(app.previews.largePreviewId, "original");
    assert.ok(!calls.some(call => call.name === "hide_large_preview"));
  } finally { app.dispose(); }
});
