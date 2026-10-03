import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const handler = source.statements.find(statement => ts.isExpressionStatement(statement)
  && ts.isCallExpression(statement.expression)
  && statement.expression.expression.getText(source) === "document.addEventListener"
  && statement.expression.arguments[0]?.text === "keydown"
  && statement.expression.arguments[1].getText(source).includes("if (!state"));
assert.ok(handler, "Missing panel keyboard handler");
const { outputText } = ts.transpileModule(handler.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2020 } });

function setup({ focused = false, settingsOpen = false, selectedId = "selected", isMacOS = false,
  records = [{ id: "selected", favorite: false }] } = {}) {
  const effects = [];
  const actions = [];
  const input = {
    id: "search", value: "existing", selectionStart: 8, selectionEnd: 8,
    matches: () => false, closest: () => input,
    focus: () => effects.push("focus-search"), blur: () => effects.push("blur-search"),
    setSelectionRange: () => {},
    setRangeText: text => { input.value += text; },
    dispatchEvent: () => effects.push("search-input"),
  };
  const row = { id: "", matches: () => false, closest: () => null };
  const menu = { hidden: true, contains: () => false };
  let keydown;
  const context = {
    state: { records }, settingsOpen, selectedId, isMacOS,
    selectedIds: new Set(records.map(record => record.id)),
    document: {
      querySelector: selector => selector === "#search" ? input : menu,
      addEventListener: (_, callback) => { keydown = callback; },
    },
    visibleRecords: () => ["visible-second", "visible-third"].map(id => ({ dataset: { id } })),
    filtered: () => context.state.records,
    apply: id => effects.push(["apply", id]),
    action: (name, args) => { effects.push([name, args.operation.kind]); actions.push(JSON.parse(JSON.stringify(args))); },
    Event: class {},
  };
  runInNewContext(outputText, context);
  return {
    effects, input, actions,
    press(key, overrides = {}) {
      const event = {
        key, code: key.length === 1 ? (/\d/.test(key) ? `Digit${key}` : `Key${key.toUpperCase()}`) : key,
        target: focused ? input : row,
        ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, repeat: false,
        isComposing: false, keyCode: 0, defaultPrevented: false,
        getModifierState: () => false,
        preventDefault() { this.defaultPrevented = true; },
        ...overrides,
      };
      keydown(event);
      return event;
    },
  };
}

for (const focused of [false, true]) {
  test(`Ctrl+F focuses search without favoriting (search focused: ${focused})`, () => {
    const { press, effects } = setup({ focused });
    assert.equal(press("f", { ctrlKey: true }).defaultPrevented, true);
    assert.deepEqual(effects, ["focus-search"]);
  });
}

test("Ctrl+F focuses search when there is no selected record", () => {
  const { press, effects } = setup({ selectedId: null });
  press("f", { ctrlKey: true });
  assert.deepEqual(effects, ["focus-search"]);
});

for (const [key, overrides] of [
  ["a", {}], [" ", { code: "Space" }], ["Backspace", {}],
  ["Process", { keyCode: 229 }], ["Dead", {}], ["中", { isComposing: true }],
]) {
  test(`${key} does not redirect input to an unfocused search`, () => {
    const { press, effects, input } = setup();
    assert.equal(press(key, overrides).defaultPrevented, false);
    assert.equal(input.value, "existing");
    assert.deepEqual(effects, []);
  });
}

for (const key of ["a", "1", "Backspace", "Delete", "中"]) {
  test(`${key} keeps native input behavior in focused search`, () => {
    const { press, effects } = setup({ focused: true });
    assert.equal(press(key).defaultPrevented, false);
    assert.deepEqual(effects, []);
  });
}

for (const code of ["Digit2", "Numpad2"]) {
  test(`${code} immediately applies the matching visible ordinal`, () => {
    const { press, effects } = setup();
    assert.equal(press("2", { code }).defaultPrevented, true);
    assert.deepEqual(effects, [["apply", "visible-third"]]);
  });
}

for (const overrides of [
  { ctrlKey: true }, { altKey: true }, { shiftKey: true }, { metaKey: true },
  { getModifierState: () => true }, { isComposing: true }, { keyCode: 229 },
]) {
  test(`digits with ${Object.keys(overrides)[0]} do not apply a record`, () => {
    const { press, effects } = setup();
    press("1", overrides);
    assert.deepEqual(effects, []);
  });
}

test("holding a digit applies only once", () => {
  const { press, effects } = setup();
  press("1");
  press("1", { repeat: true });
  assert.deepEqual(effects, [["apply", "visible-second"]]);
});

test("zero and ordinals with no visible record do not apply anything", () => {
  const { press, effects } = setup();
  press("0");
  press("9");
  assert.deepEqual(effects, []);
});

test("settings retain their input behavior", () => {
  const { press, effects } = setup({ settingsOpen: true });
  press("1");
  press("f", { ctrlKey: true });
  assert.deepEqual(effects, []);
});

for (const isMacOS of [false, true]) {
  const platform = isMacOS ? "macOS" : "Windows";
  for (const [key, ctrlKey, deletes] of [
    ["Backspace", true, true], ["Backspace", false, false],
    ["Delete", false, !isMacOS], ["Delete", true, false],
  ]) {
    for (const focused of [false, true]) {
      test(`${platform} ${ctrlKey ? "Ctrl+" : ""}${key} ${focused ? "edits search" : deletes ? "deletes records" : "does not delete records"}`, () => {
        const { press, effects } = setup({ isMacOS, focused });
        assert.equal(press(key, { ctrlKey }).defaultPrevented, deletes && !focused);
        assert.deepEqual(effects, deletes && !focused ? [["batch_records", "delete"]] : []);
      });
    }
  }
  test(`${platform} Ctrl+D toggles favorites for all selected records`, () => {
    for (const [favorites, expected] of [[[false, true], true], [[true, true], false]]) {
      const records = favorites.map((favorite, index) => ({ id: index ? "other" : "selected", favorite }));
      const { press, actions } = setup({ isMacOS, records });
      assert.equal(press("d", { ctrlKey: true }).defaultPrevented, true);
      assert.deepEqual(actions, [{ ids: ["selected", "other"], operation: { kind: "set_favorite", favorite: expected } }]);
    }
  });
  test(`${platform} repeated deletion and favorite shortcuts do not repeat actions`, () => {
    const { press, effects } = setup({ isMacOS });
    press("Backspace", { ctrlKey: true, repeat: true });
    press("d", { ctrlKey: true, repeat: true });
    assert.deepEqual(effects, []);
  });
  for (const overrides of [{ altKey: true }, { shiftKey: true }, { metaKey: true },
    { getModifierState: () => true }, { isComposing: true }, { keyCode: 229 }]) {
    test(`${platform} Ctrl+Backspace with ${Object.keys(overrides)[0]} does not delete records`, () => {
      const { press, effects } = setup({ isMacOS });
      press("Backspace", { ctrlKey: true, ...overrides });
      assert.deepEqual(effects, []);
    });
  }
}
