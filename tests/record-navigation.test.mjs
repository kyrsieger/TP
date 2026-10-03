import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

// Run the existing navigation functions without starting the Tauri window.
const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const navigation = ["filtered", "selectRecord", "moveSelection"].map(name => {
  const declaration = source.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === name);
  assert.ok(declaration, `Missing ${name}`);
  return declaration.getText(source);
}).join("\n");
const { outputText } = ts.transpileModule(navigation, { compilerOptions: { target: ts.ScriptTarget.ES2020 } });
const records = ["first", "middle", "last"].map(id => ({ id, kind: "text", body: "match", files: [], favorite: true, groupId: 1 }));

function setup(overrides = {}) {
  const effects = [];
  const context = {
    state: { records }, view: "default", kindFilter: "all", search: "",
    selectedId: "first", selectedIds: new Set(["first"]), selectionAnchorId: "first",
    backToTopFrame: 42, previewFromKeyboard: true,
    window: { cancelAnimationFrame: () => {} }, markSelectedRows: () => {},
    closePreview: () => effects.push("close-preview"),
    updateOrdinals: () => effects.push("ordinals"),
    ensureRecordRow: id => ({
      scrollIntoView: options => effects.push(["scroll", id, options.block]),
      focus: options => effects.push(["focus", id, options.preventScroll]),
    }),
    ...overrides,
  };
  runInNewContext(outputText, context);
  return { context, effects };
}

for (const [selectedId, direction, expected] of [
  ["first", -1, "last"], ["last", 1, "first"],
  ["middle", -1, "first"], ["middle", 1, "last"],
  [null, -1, "first"], [null, 1, "first"],
]) {
  test(`move ${selectedId} by ${direction} selects ${expected}`, () => {
    const { context, effects } = setup({ selectedId, selectedIds: new Set(["first", "middle", "last"]) });
    context.moveSelection(direction);
    assert.equal(context.selectedId, expected);
    assert.deepEqual([...context.selectedIds], [expected]);
    assert.equal(context.selectionAnchorId, expected);
    assert.deepEqual(effects, ["close-preview", ["scroll", expected, "nearest"], ["focus", expected, true], "ordinals"]);
    assert.equal(context.backToTopFrame, 0);
  });
}

for (const direction of [-1, 1]) {
  test(`empty list ignores ${direction}`, () => {
    const { context, effects } = setup({ state: { records: [] }, selectedId: null, selectedIds: new Set() });
    context.moveSelection(direction);
    assert.equal(context.selectedId, null);
    assert.equal(context.selectedIds.size, 0);
    assert.deepEqual(effects, []);
  });

  test(`single record stays selected for ${direction}`, () => {
    const { context } = setup({ state: { records: [records[0]] } });
    context.moveSelection(direction);
    assert.equal(context.selectedId, "first");
    assert.deepEqual([...context.selectedIds], ["first"]);
  });

  test(`Shift movement stops at the ${direction < 0 ? "first" : "last"} record`, () => {
    const selectedId = direction < 0 ? "first" : "last";
    const { context } = setup({ selectedId, selectionAnchorId: selectedId, selectedIds: new Set([selectedId]) });
    context.moveSelection(direction, true);
    assert.equal(context.selectedId, selectedId);
    assert.deepEqual([...context.selectedIds], [selectedId]);
  });
}

test("Shift movement expands and contracts the continuous selection", () => {
  const { context } = setup();
  context.moveSelection(1, true);
  assert.deepEqual([...context.selectedIds], ["first", "middle"]);
  context.moveSelection(1, true);
  assert.deepEqual([...context.selectedIds], ["first", "middle", "last"]);
  context.moveSelection(-1, true);
  assert.deepEqual([...context.selectedIds], ["first", "middle"]);
  assert.equal(context.selectionAnchorId, "first");
});

for (const [name, excluded, filter] of [
  ["search", { body: "other" }, { search: "match" }],
  ["type", { kind: "image" }, { kindFilter: "text" }],
  ["favorites", { favorite: false }, { view: "favorite" }],
  ["group", { groupId: 2 }, { view: "group:1" }],
]) {
  test(`navigation wraps within the current ${name} results`, () => {
    const { context } = setup({
      state: { records: [{ ...records[1], ...excluded }, records[0], records[2], { ...records[1], id: "excluded", ...excluded }] },
      ...filter,
    });
    context.moveSelection(-1);
    assert.equal(context.selectedId, "last");
    context.moveSelection(1);
    assert.equal(context.selectedId, "first");
  });
}

let groupMenuBranch;
function findGroupMenuBranch(node) {
  if (ts.isIfStatement(node) && ts.isBinaryExpression(node.expression)
    && node.expression.left.getText(source) === "event.code"
    && node.expression.right.getText(source) === '"KeyG"') groupMenuBranch = node;
  ts.forEachChild(node, findGroupMenuBranch);
}
findGroupMenuBranch(source);
assert.ok(groupMenuBranch);
const { outputText: openGroupMenu } = ts.transpileModule(groupMenuBranch.getText(source), {
  compilerOptions: { target: ts.ScriptTarget.ES2020 },
});

function groupMenuSetup() {
  const frames = [];
  const scrollEvents = [];
  const trigger = {};
  const row = { querySelector: () => trigger };
  let menuOpen = false;
  const context = {
    event: { code: "KeyG" }, selectedId: "first", panelActive: true, panelGeneration: 1, records,
    ensureRecordRow: () => { scrollEvents.push(() => { menuOpen = false; }); return row; },
    recordRow: () => row,
    window: { requestAnimationFrame: callback => frames.push(callback) },
    openMoveGroupMenu: () => { menuOpen = true; },
  };
  runInNewContext(openGroupMenu, context);
  return { context, frames, scrollEvents, menuIsOpen: () => menuOpen };
}

test("Ctrl+G keeps its menu open after scrolling an unmounted selection into view", () => {
  const { frames, scrollEvents, menuIsOpen } = groupMenuSetup();
  scrollEvents.forEach(callback => callback());
  frames.forEach(callback => callback());
  assert.equal(menuIsOpen(), true);
});

for (const [name, changed] of [["hidden panel", { panelActive: false }], ["changed selection", { selectedId: "last" }]]) {
  test(`a pending Ctrl+G ignores a ${name}`, () => {
    const { context, frames, menuIsOpen } = groupMenuSetup();
    Object.assign(context, changed);
    frames.forEach(callback => callback());
    assert.equal(menuIsOpen(), false);
  });
}
