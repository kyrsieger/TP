import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const functions = ["refresh", "clearHiddenSearch", "releasePanelContent", "openPanel"].map(name => {
  const declaration = source.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === name);
  assert.ok(declaration, `Missing ${name}`);
  return declaration.getText(source);
}).join("\n");
const { outputText } = ts.transpileModule(functions, { compilerOptions: { target: ts.ScriptTarget.ES2020 } });

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

function setup() {
  const effects = [];
  const requests = [];
  const image = { removeAttribute: name => effects.push(`image-remove-${name}`) };
  const elements = new Map(["#list", "#groups", "#settings", "#search", "#clear-search"].map(selector => [selector, {
    value: "", hidden: false, scrollTop: 0,
    querySelectorAll: () => [image], replaceChildren: () => effects.push(`clear-${selector}`),
  }]));
  const snapshot = body => ({ records: [{ id: body, body }], groups: [], settings: { shortcut: "Control+Shift+V", retainSearchInput: true } });
  const context = {
    panelActive: false, panelGeneration: 0, refreshing: null, refreshAgain: false, openingPanel: null,
    state: snapshot("old"), view: "favorite", kindFilter: "image", search: "kept",
    settingsOpen: false, backToTopFrame: 1, largePreviewId: "old", largePreviewSizeRequest: 0,
    settingsReturnFocus: null, groupEdit: null, backToTopPosition: null,
    selectedId: "old", selectedIds: new Set(["old"]), selectionAnchorId: "old",
    listRecords: [{ id: "old" }], offsets: [0, 48], renderedRange: { start: 0, end: 1 },
    shortcutError: null, activeShortcut: "", thumbnailObserver: { disconnect: () => effects.push("disconnect-images") },
    document: { querySelector: selector => elements.get(selector) },
    window: { cancelAnimationFrame: () => {} },
    render: () => effects.push(["render", context.state.records[0]?.body]),
    refreshCacheSize: () => effects.push("cache-size"),
    savePanelSession: () => effects.push(["session", context.view, context.kindFilter, context.search]),
    closePreview: () => effects.push("close-preview"), closeSettings: () => { context.settingsOpen = false; },
    closeGroupMenu: () => {}, closeMoveGroupMenu: () => {}, closeRecordMenu: () => {}, setFilterMenuOpen: () => {},
    placePanel: async () => effects.push("place"),
    call: name => {
      if (name === "get_state") {
        const request = deferred(); requests.push(request); effects.push("get-state"); return request.promise;
      }
      effects.push(name); return Promise.resolve();
    },
  };
  runInNewContext(outputText, context);
  return { context, effects, requests, snapshot, elements };
}

test("hidden clipboard changes do not fetch or render the snapshot", async () => {
  const { context, effects } = setup();
  await context.refresh(); await context.refresh();
  assert.deepEqual(effects, []);
});

test("startup installs wake and hide listeners before reporting ready without loading history", async () => {
  const declaration = source.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === "start");
  const { outputText: startup } = ts.transpileModule(declaration.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2020 } });
  const effects = [];
  const context = {
    view: "default", kindFilter: "all", search: "", shortcutError: null,
    initPreviewWindows: async () => {}, listen: async name => effects.push(`listen-${name}`),
    releasePanelContent: () => {},
    panel: { onMoved: async () => {}, onResized: async () => {} },
    call: async name => {
      effects.push(name);
      return name === "get_panel_session" ? { view: "favorite", kindFilter: "image", search: "restored" } : null;
    },
    toast: message => assert.fail(message),
  };
  runInNewContext(startup, context);
  await context.start();
  assert.ok(effects.indexOf("listen-open-requested") < effects.indexOf("panel_ready"));
  assert.ok(effects.indexOf("listen-panel-hidden") < effects.indexOf("panel_ready"));
  assert.ok(!effects.includes("get_state"));
  assert.deepEqual([context.view, context.kindFilter, context.search], ["favorite", "image", "restored"]);
});

test("hiding releases image sources and records and rejects an in-flight snapshot", async () => {
  const { context, effects, requests, snapshot } = setup();
  context.panelActive = true;
  const refresh = context.refresh();
  context.releasePanelContent();
  requests[0].resolve(snapshot("stale"));
  await refresh;
  assert.equal(context.state.records.length, 0);
  assert.equal(context.listRecords.length, 0);
  assert.equal(context.selectedIds.size, 0);
  assert.ok(effects.includes("image-remove-src"));
  assert.ok(effects.includes("clear-#list"));
  assert.ok(effects.includes("clear-#settings"));
  assert.ok(!effects.some(effect => Array.isArray(effect) && effect[0] === "render"));
  assert.deepEqual(effects.find(effect => Array.isArray(effect) && effect[0] === "session"), ["session", "favorite", "image", "kept"]);
});

test("opening coalesces wake requests and loads current data before showing", async () => {
  const { context, effects, requests, snapshot, elements } = setup();
  const first = context.openPanel();
  assert.equal(context.openPanel(), first);
  requests[0].resolve(snapshot("fresh"));
  await first;
  assert.equal(context.panelActive, true);
  assert.equal(elements.get("#search").value, "kept");
  assert.deepEqual(effects, ["get-state", ["render", "fresh"], "place", "show_panel"]);
});

for (const retain of [true, false]) {
  test(`hiding and reopening ${retain ? "retains" : "clears"} search without changing group or type`, async () => {
    const { context, elements, effects, requests, snapshot } = setup();
    context.panelActive = true;
    context.state.settings.retainSearchInput = retain;
    elements.get("#search").value = "kept";
    context.releasePanelContent();
    const expected = retain ? "kept" : "";
    assert.equal(context.search, expected);
    assert.equal(elements.get("#search").value, expected);
    if (!retain) assert.equal(elements.get("#clear-search").hidden, true);
    assert.deepEqual(effects.find(effect => Array.isArray(effect) && effect[0] === "session"), ["session", "favorite", "image", expected]);
    context.releasePanelContent();
    const opened = context.openPanel();
    const fresh = snapshot("fresh");
    fresh.settings.retainSearchInput = retain;
    requests[0].resolve(fresh);
    await opened;
    assert.equal(elements.get("#search").value, expected);
    assert.equal(elements.get("#clear-search").hidden, !expected);
    assert.equal(context.search, expected);
  });
}

test("turning retention off leaves the current visible search intact", () => {
  const { context, elements } = setup();
  context.panelActive = true;
  context.state.settings.retainSearchInput = false;
  elements.get("#search").value = "kept";
  context.clearHiddenSearch();
  assert.equal(context.search, "kept");
  assert.equal(elements.get("#search").value, "kept");
});

test("a canceled open never shows the panel and a later wake can reopen", async () => {
  const { context, effects, requests, snapshot } = setup();
  const first = context.openPanel();
  context.releasePanelContent();
  const second = context.openPanel();
  requests[0].resolve(snapshot("stale"));
  await first;
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(requests.length, 2);
  requests[1].resolve(snapshot("reopened"));
  await second;
  assert.deepEqual(effects.filter(effect => Array.isArray(effect) && effect[0] === "render"), [["render", "reopened"]]);
  assert.equal(effects.filter(effect => effect === "show_panel").length, 1);
});

test("many clipboard changes coalesce while still loading the latest snapshot", async () => {
  const { context, requests, snapshot, effects } = setup();
  context.panelActive = true;
  const refresh = context.refresh();
  assert.equal(context.refresh(), refresh);
  assert.equal(context.refresh(), refresh);
  requests[0].resolve(snapshot("first"));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(requests.length, 2);
  requests[1].resolve(snapshot("latest"));
  await refresh;
  assert.equal(context.state.records[0].body, "latest");
  assert.equal(effects.filter(effect => effect === "get-state").length, 2);
});
