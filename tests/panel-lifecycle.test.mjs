import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { PanelController } = loadTs("src/state/panel.ts");
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const snapshot = id => ({ records: [{ id, body: id, files: [], kind: "text", groupId: null }], groups: [], settings: { shortcut: "", retainSearchInput: true } });
function setup(show = () => Promise.resolve(), command = () => Promise.resolve(), isVisible = async () => true) {
  const effects = [], requests = [];
  const panel = new PanelController(name => {
    effects.push(name);
    if (name === "get_state") { const pending = deferred(); requests.push(pending); return pending.promise; }
    if (name === "show_panel") return show();
    return command(name);
  }, { reset: () => effects.push("reset"), beforeRefresh: () => {}, afterRefresh: () => effects.push("render"), afterBrowse: () => {},
    place: async () => effects.push("place"), afterOpen: () => effects.push(["focus", panel.value.selectedId]), error: error => assert.fail(error), isVisible });
  panel.restore({ view: "favorite", kindFilter: "image", search: "kept" });
  return { panel, effects, requests };
}
test("hidden clipboard changes do not fetch or render the snapshot", async () => {
  const { panel, effects } = setup(); await panel.refresh(); await panel.refresh(); assert.deepEqual(effects, []);
});
test("hiding retains records, rejects in-flight refresh, and idle release drops the snapshot", async () => {
  const { panel, requests, effects } = setup(); panel.update({ active: true, snapshot: snapshot("old") });
  const refresh = panel.refresh(); panel.hide(2); requests[0].resolve(snapshot("late")); await refresh;
  assert.equal(panel.value.snapshot.records[0].id, "old");
  await panel.refresh(); assert.equal(requests.length, 1);
  assert.equal(panel.release(2), true);
  assert.equal(panel.value.snapshot, null); assert.equal(panel.value.active, false); assert.ok(!effects.includes("render"));
  assert.equal(panel.value.selectedIds.size, 0);
});
test("hiding clears search only when the saved setting disables retention", () => {
  for (const retainSearchInput of [false, true]) {
    const { panel } = setup(); panel.update({ active: true, settings: { retainSearchInput } }); panel.hide();
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

test("opening resets a restored group while retaining search and type filter", async () => {
  for (const view of ["favorite", "group:1"]) {
    const { panel, requests, effects } = setup();
    panel.restore({ view, kindFilter: "image", search: "kept" });
    const opening = panel.open();
    assert.equal(panel.value.view, "default");
    requests[0].resolve({ ...snapshot("fresh"), records: [{ id: "fresh", body: "kept", files: [], kind: "image", groupId: null }] });
    await opening;
    assert.equal(panel.value.kindFilter, "image"); assert.equal(panel.value.search, "kept");
    assert.deepEqual(panel.records.map(record => record.id), ["fresh"]);
    assert.deepEqual(effects.at(-1), ["focus", "fresh"]);
  }
});

test("closing from favorites or a custom group always reopens the default view", async () => {
  for (const view of ["favorite", "group:1"]) {
    const { panel, requests } = setup();
    panel.restore({ view: "default", kindFilter: "all", search: "" });
    const state = { ...snapshot("ordinary"), groups: [{ id: 1, name: "工作" }], records: [
      ...snapshot("ordinary").records,
      { id: "grouped", body: "grouped", files: [], kind: "text", groupId: 1, favorite: true },
    ] };
    const first = panel.open(); requests[0].resolve(state); await first;
    panel.selectGroup(view);
    assert.equal(panel.value.view, view); assert.equal(panel.value.selectedId, "grouped");
    await panel.action("hide_panel");
    const second = panel.open(); requests[1].resolve(state); await second;
    assert.equal(panel.value.view, "default"); assert.equal(panel.value.selectedId, "ordinary");
    assert.deepEqual(panel.records.map(record => record.id), ["ordinary", "grouped"]);
  }
});

test("hiding while show is pending does not restore stale list focus", async () => {
  const showing = deferred(), { panel, requests, effects } = setup(() => showing.promise);
  panel.restore({ view: "default", kindFilter: "all", search: "" });
  const opening = panel.open(); requests[0].resolve(snapshot("first"));
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(effects.includes("show_panel"));
  panel.hide(); showing.resolve(); await opening;
  assert.ok(!effects.some(effect => Array.isArray(effect) && effect[0] === "focus"));
});
test("reopening during a hidden stale request fetches a fresh snapshot", async () => {
  const { panel, requests } = setup(); const first = panel.open(); panel.hide(); const second = panel.open();
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

test("a delayed hide response cannot clear the first reopened panel", async () => {
  const hidden = deferred();
  const { panel, requests } = setup(undefined, name => name === "hide_panel" ? hidden.promise : Promise.resolve());
  panel.restore({ view: "default", kindFilter: "all", search: "" });
  const initial = panel.open(); requests[0].resolve(snapshot("old")); await initial;
  const closing = panel.action("hide_panel");
  panel.hide(); // The native hidden event arrives before the command response.
  const reopening = panel.open(); requests[1].resolve(snapshot("fresh")); await reopening;
  hidden.resolve(); await closing;
  assert.equal(panel.value.active, true);
  assert.equal(panel.value.snapshot.records[0].id, "fresh");
  assert.equal(panel.value.selectedId, "fresh");
  assert.equal(requests.length, 2);
});

test("a delayed apply visibility response cannot clear a reopened panel", async () => {
  const visible = deferred();
  const { panel, requests } = setup(undefined, undefined, () => visible.promise);
  panel.restore({ view: "default", kindFilter: "all", search: "" });
  const initial = panel.open(); requests[0].resolve(snapshot("old")); await initial;
  const applying = panel.apply("old"); await Promise.resolve();
  panel.hide();
  const reopening = panel.open(); requests[1].resolve(snapshot("fresh")); await reopening;
  visible.resolve(false); await applying;
  assert.equal(panel.value.active, true);
  assert.equal(panel.value.snapshot.records[0].id, "fresh");
  assert.equal(requests.length, 2);
});

test("late hidden and idle-release notices cannot blank a newer open or clear its cache", async () => {
  const { panel, requests } = setup();
  const first = panel.open(1); requests[0].resolve(snapshot("old")); await first;
  panel.hide(2);
  const reopened = panel.open(3); requests[1].resolve(snapshot("fresh")); await reopened;
  panel.hide(2); assert.equal(panel.release(2), false);
  assert.equal(panel.value.active, true); assert.equal(panel.value.snapshot.records[0].id, "fresh");
  panel.hide(4); assert.equal(panel.release(2), false);
  assert.equal(panel.value.snapshot.records[0].id, "fresh");
  assert.equal(panel.release(4), true); assert.equal(panel.value.snapshot, null);
  await panel.open(3); assert.equal(panel.value.active, false);
});
