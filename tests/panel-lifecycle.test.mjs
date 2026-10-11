import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { PanelController } = loadTs("src/state/panel.ts");
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const snapshot = id => ({ records: [{ id, body: id, files: [], kind: "text", groupId: null }], groups: [], settings: { shortcut: "", retainSearchInput: true } });
function setup(show = () => Promise.resolve()) {
  const effects = [], requests = [];
  const panel = new PanelController(name => {
    effects.push(name);
    if (name === "get_state") { const pending = deferred(); requests.push(pending); return pending.promise; }
    if (name === "show_panel") return show();
    return Promise.resolve();
  }, { reset: () => effects.push("reset"), beforeRefresh: () => {}, afterRefresh: () => effects.push("render"), afterBrowse: () => {},
    place: async () => effects.push("place"), afterOpen: () => effects.push(["focus", panel.value.selectedId]), error: error => assert.fail(error), isVisible: async () => true });
  panel.restore({ view: "favorite", kindFilter: "image", search: "kept" });
  return { panel, effects, requests };
}
test("hidden clipboard changes do not fetch or render the snapshot", async () => {
  const { panel, effects } = setup(); await panel.refresh(); await panel.refresh(); assert.deepEqual(effects, []);
});
test("hiding drops records and rejects an in-flight snapshot", async () => {
  const { panel, requests, effects } = setup(); panel.update({ active: true, snapshot: snapshot("old") });
  const refresh = panel.refresh(); panel.release(); requests[0].resolve(snapshot("late")); await refresh;
  assert.equal(panel.value.snapshot, null); assert.equal(panel.value.active, false); assert.ok(!effects.includes("render"));
  assert.equal(panel.value.selectedIds.size, 0);
});
test("hiding clears search only when the saved setting disables retention", () => {
  for (const retainSearchInput of [false, true]) {
    const { panel } = setup(); panel.update({ active: true, settings: { retainSearchInput } }); panel.release();
    assert.equal(panel.value.search, retainSearchInput ? "kept" : "");
    assert.equal(panel.value.view, "favorite"); assert.equal(panel.value.kindFilter, "image");
  }
});
test("retention disabled after hiding clears the late search without retaining a snapshot", () => {
  const { panel } = setup(); panel.commitSettings({ retainSearchInput: false });
  assert.equal(panel.value.search, ""); assert.equal(panel.value.snapshot, null);
});
test("reopening waits for the latest snapshot before positioning and showing", async () => {
  const { panel, requests, effects } = setup();
  panel.restore({ view: "default", kindFilter: "all", search: "" });
  const opening = panel.open(); assert.equal(requests.length, 1); assert.ok(!effects.includes("show_panel"));
  requests[0].resolve(snapshot("fresh")); await opening;
  assert.equal(panel.value.snapshot.records[0].id, "fresh");
  assert.ok(effects.indexOf("render") < effects.indexOf("place")); assert.ok(effects.indexOf("place") < effects.indexOf("show_panel"));
  assert.deepEqual(effects.at(-1), ["focus", "fresh"]);
});

test("hiding while show is pending does not restore stale list focus", async () => {
  const showing = deferred(), { panel, requests, effects } = setup(() => showing.promise);
  panel.restore({ view: "default", kindFilter: "all", search: "" });
  const opening = panel.open(); requests[0].resolve(snapshot("first"));
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(effects.includes("show_panel"));
  panel.release(); showing.resolve(); await opening;
  assert.ok(!effects.some(effect => Array.isArray(effect) && effect[0] === "focus"));
});
test("reopening during a hidden stale request fetches a fresh snapshot", async () => {
  const { panel, requests } = setup(); const first = panel.open(); panel.release(); const second = panel.open();
  requests[0].resolve(snapshot("stale")); await first; await Promise.resolve();
  assert.equal(requests.length, 2); requests[1].resolve(snapshot("fresh")); await second;
  assert.equal(panel.value.snapshot.records[0].id, "fresh");
});
test("many clipboard changes coalesce while still loading the latest snapshot", async () => {
  const { panel, requests } = setup(); panel.update({ active: true });
  const first = panel.refresh(); const second = panel.refresh(); panel.refresh();
  assert.equal(first, second); assert.equal(requests.length, 1);
  requests[0].resolve(snapshot("first")); await new Promise(resolve => setImmediate(resolve));
  assert.equal(requests.length, 2); requests[1].resolve(snapshot("latest")); await first;
  assert.equal(panel.value.snapshot.records[0].id, "latest");
});
