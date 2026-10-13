import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

function setup({ delayListeners = false } = {}) {
  const calls = [], events = new Map(), domEvents = new Map(), pending = [];
  const noop = () => {};
  const subscribe = async (name, callback) => {
    calls.push(`listen:${name}`); events.set(name, callback);
    const dispose = () => { if (events.get(name) === callback) events.delete(name); };
    return delayListeners ? new Promise(resolve => pending.push(() => resolve(dispose))) : dispose;
  };
  const dom = { addEventListener: (name, callback) => domEvents.set(name, callback), removeEventListener: name => domEvents.delete(name) };
  class PreviewController { reset() {} closePreview() {} reconcile() {} async start() { return noop; } }
  class GeometryController { dispose() {} async placePanel() {} }
  const { createApplication } = loadTs("src/application.ts", { document: dom, window: { ...dom, clearTimeout: noop } }, {
    "react-dom": { flushSync: callback => callback() },
    "@tauri-apps/api/event": { listen: subscribe },
    "./services/desktop": { isMacOS: false, panelWindow: {
      onMoved: callback => subscribe("moved", callback), onResized: callback => subscribe("resized", callback),
    }, call: async name => {
      calls.push(name);
      if (name === "get_panel_session") return { view: "favorite", kindFilter: "image", search: "restored" };
      return null;
    } },
    "./services/previews": { PreviewController }, "./services/geometry": { GeometryController },
  });
  const list = { reset: noop, cancelScroll: noop };
  return { app: createApplication(list), calls, events, domEvents, pending };
}
test("wake and hide listeners are installed before ready, without reading hidden records", async () => {
  const { app, calls } = setup(); await app.start();
  assert.ok(calls.indexOf("listen:open-requested") < calls.indexOf("panel_ready"));
  assert.ok(calls.indexOf("listen:panel-hidden") < calls.indexOf("panel_ready"));
  assert.ok(!calls.includes("get_state")); assert.equal(app.history.value.search, "restored"); app.dispose();
});
test("React effect remount reattaches window listeners and removes them on final cleanup", async () => {
  const { app, events, domEvents, calls } = setup(); await app.start(); app.dispose();
  assert.equal(events.size, 0); assert.equal(domEvents.size, 0);
  await app.start(); assert.ok(events.has("open-requested")); assert.ok(events.has("panel-hidden"));
  assert.equal(calls.filter(name => name === "panel_ready").length, 2);
  app.dispose(); assert.equal(events.size, 0); assert.equal(domEvents.size, 0);
});

test("startup registers listeners concurrently but announces ready only after all finish", async () => {
  const { app, calls, pending } = setup({ delayListeners: true });
  const starting = app.start();
  assert.equal(pending.length, 6);
  assert.ok(calls.includes("get_panel_session"));
  assert.ok(!calls.includes("panel_ready"));
  pending.slice(0, -1).forEach(resolve => resolve());
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(!calls.includes("panel_ready"));
  pending.at(-1)(); await starting;
  assert.ok(calls.includes("panel_ready")); app.dispose();
});

test("disposing during concurrent startup removes late listeners and never announces ready", async () => {
  const { app, calls, events, pending } = setup({ delayListeners: true });
  const starting = app.start(); app.dispose(); pending.forEach(resolve => resolve()); await starting;
  assert.equal(events.size, 0); assert.ok(!calls.includes("panel_ready"));
});
