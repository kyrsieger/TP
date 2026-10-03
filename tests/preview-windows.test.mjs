import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const { outputText } = ts.transpileModule(readFileSync(new URL("../src/preview-windows.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS },
});
const flush = () => new Promise(resolve => setImmediate(resolve));

function setup({ autoReady = true, autoDestroy = true } = {}) {
  const listeners = new Map();
  const timers = new Map();
  const existing = new Set();
  const calls = [];
  let nextTimer = 0;
  function emit(name, payload) {
    for (const callback of [...(listeners.get(name) ?? [])]) callback({ payload });
  }
  const events = {
    listen: async (name, callback) => {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(callback);
      return () => listeners.get(name).delete(callback);
    },
    emitTo: async (label, name) => {
      if (autoReady && name === `${label}-ping`) emit(`${label}-ready`);
    },
  };
  const core = { invoke: async (name, { label }) => {
    calls.push([name, label]);
    if (name === "ensure_preview_window") {
      existing.add(label);
      emit("preview-window-used", label);
    } else if (name === "release_preview_window") {
      if (!existing.has(label)) return false;
      if (autoDestroy) {
        existing.delete(label);
        emit(`${label}-destroyed`);
      }
      return true;
    }
  } };
  const context = {
    exports: {}, console,
    require: name => name.endsWith("/event") ? events : core,
    window: {
      setTimeout: (callback, duration) => { const id = ++nextTimer; timers.set(id, { callback, duration }); return id; },
      clearTimeout: id => timers.delete(id),
    },
  };
  runInNewContext(outputText, context);
  return {
    ...context.exports, calls, existing, listeners, timers, emit,
    fireIdle: () => {
      for (const [id, timer] of [...timers]) {
        if (timer.duration === 30000) { timers.delete(id); timer.callback(); }
      }
    },
  };
}

test("startup leaves both preview windows uncreated; first use waits for ready", async () => {
  const app = setup({ autoReady: false });
  await app.initPreviewWindows();
  assert.equal(app.calls.length, 0);
  let ready = false;
  const opening = app.ensurePreviewWindow("preview").then(() => { ready = true; });
  await flush();
  assert.equal(ready, false);
  assert.deepEqual(app.calls, [["ensure_preview_window", "preview"]]);
  app.emit("preview-ready");
  await opening;
  assert.equal(ready, true);
  assert.equal(app.listeners.get("preview-ready").size, 0);
});

test("concurrent requests share one window creation", async () => {
  const app = setup();
  await app.initPreviewWindows();
  await Promise.all([app.ensurePreviewWindow("preview"), app.ensurePreviewWindow("preview")]);
  assert.deepEqual(app.calls, [["ensure_preview_window", "preview"]]);
});

test("hidden previews release after idle and recreate on the next use", async () => {
  const app = setup();
  await app.initPreviewWindows();
  for (const label of ["preview", "large-preview"]) {
    await app.ensurePreviewWindow(label);
    app.emit("preview-window-hidden", label);
    app.fireIdle();
    await flush();
    assert.equal(app.existing.has(label), false);
    await app.ensurePreviewWindow(label);
    assert.equal(app.existing.has(label), true);
    assert.equal(app.listeners.get(`${label}-destroyed`).size, 0);
  }
});

test("reusing a hidden window cancels its idle release", async () => {
  const app = setup();
  await app.initPreviewWindows();
  await app.ensurePreviewWindow("preview");
  app.emit("preview-window-hidden", "preview");
  await app.ensurePreviewWindow("preview");
  app.fireIdle();
  await flush();
  assert.equal(app.calls.some(([name]) => name === "release_preview_window"), false);
});

test("repeated hides do not postpone release of an already idle window", async () => {
  const app = setup();
  await app.initPreviewWindows();
  await app.ensurePreviewWindow("preview");
  app.emit("preview-window-hidden", "preview");
  const timer = [...app.timers.keys()];
  app.emit("preview-window-hidden", "preview");
  assert.deepEqual([...app.timers.keys()], timer);
  app.fireIdle();
  await flush();
  assert.equal(app.existing.has("preview"), false);
});

test("reopening while destruction is pending waits for the old window to disappear", async () => {
  const app = setup({ autoDestroy: false });
  await app.initPreviewWindows();
  await app.ensurePreviewWindow("large-preview");
  app.emit("preview-window-hidden", "large-preview");
  app.fireIdle();
  await flush();
  const reopening = app.ensurePreviewWindow("large-preview");
  await flush();
  assert.equal(app.calls.filter(([name]) => name === "ensure_preview_window").length, 1);
  app.existing.delete("large-preview");
  app.emit("large-preview-destroyed");
  await reopening;
  assert.equal(app.calls.filter(([name]) => name === "ensure_preview_window").length, 2);
});
