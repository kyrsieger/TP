import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

function setup() {
  const calls = [], openings = [], webviewWindows = [], events = [], messages = [];
  const { PreviewController } = loadTs("src/services/previews.ts", {
    window: { clearTimeout() {} },
  }, {
    "./desktop": { call: async (name, args) => {
      calls.push({ name, args });
      if (name === "open_native_large_preview") {
        return new Promise((resolve, reject) => openings.push({ resolve, reject }));
      }
    } },
    "@tauri-apps/api/event": { emitTo: async (label, event, payload) => { events.push({ label, event, payload }); } },
    "@tauri-apps/api/window": {},
    "../preview-windows": { ensurePreviewWindow: async label => { webviewWindows.push(label); } },
  });
  const records = ["first", "second"].map(id => ({ id, kind: "image", files: [], preview: true }));
  const history = { records, value: { snapshot: { records }, settings: {} } };
  const previews = new PreviewController(history, {}, {}, message => messages.push(message));
  return { previews, calls, openings, webviewWindows, events, messages };
}

for (const newPreviewReady of [false, true]) {
  test(`a cancelled native request cannot close a newer preview that is ${newPreviewReady ? "visible" : "loading"}`, async () => {
    const { previews, calls, openings, webviewWindows, messages } = setup();
    const oldRequest = previews.openLargePreview("first");
    previews.reset();
    const newRequest = previews.openLargePreview("second");
    if (newPreviewReady) {
      openings[1].resolve(true);
      await newRequest;
    }
    // The native loader returns true even when its generation was cancelled.
    openings[0].resolve(true);
    await oldRequest;
    assert.equal(previews.largePreviewId, "second");
    assert.equal(calls.filter(call => call.name === "hide_large_preview").length, 0);
    assert.equal(webviewWindows.length, 0);
    assert.equal(messages.length, 0);
    if (!newPreviewReady) {
      openings[1].resolve(true);
      await newRequest;
    }
  });
}

test("reopening the same record ignores an older native response", async () => {
  const { previews, calls, openings, webviewWindows, events } = setup();
  const oldRequest = previews.openLargePreview("first");
  previews.reset();
  const newRequest = previews.openLargePreview("first");
  openings[1].resolve(true);
  await newRequest;
  openings[0].resolve(true);
  await oldRequest;
  assert.equal(previews.largePreviewId, "first");
  assert.equal(calls.filter(call => call.name === "hide_large_preview").length, 0);
  assert.equal(webviewWindows.length, 0);
  assert.equal(events.length, 0);
});

test("an older failure cannot close a reopened record or report a stale error", async () => {
  const { previews, calls, openings, messages } = setup();
  const oldRequest = previews.openLargePreview("first");
  previews.reset();
  const newRequest = previews.openLargePreview("first");
  openings[1].resolve(true);
  await newRequest;
  openings[0].reject(new Error("old image failed"));
  await oldRequest;
  assert.equal(previews.largePreviewId, "first");
  assert.equal(calls.filter(call => call.name === "hide_large_preview").length, 0);
  assert.equal(messages.length, 0);
});
