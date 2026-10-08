import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

function setup({ focused = false, settingsOpen = false, selectedId = "selected", isMacOS = false,
  search = "existing", visibleIds = ["visible-second", "visible-third"],
  records = [{ id: "selected", favorite: false }] } = {}) {
  const effects = [], actions = [];
  const input = { id: "search", value: search, matches: () => false, closest: () => input,
    focus: () => effects.push("focus-search"), blur: () => effects.push("blur-search") };
  const row = { id: "", matches: () => false, closest: () => null };
  const menu = { hidden: true, contains: () => false };
  const { createPanelKeydown } = loadTs("src/input/keyboard.ts", { document: { querySelector: selector => selector === "#search" ? input : menu } });
  const context = {
    history: { value: { snapshot: { records }, selectedId, selectedIds: new Set(records.map(record => record.id)) },
      records, apply: id => effects.push(["apply", id]), action: (name,args) => { effects.push([name,args.operation.kind]); actions.push(JSON.parse(JSON.stringify(args))); } },
    settings: { value: { open: settingsOpen } }, menus: { update() {} }, previews: {},
    list: { visibleRecords: () => visibleIds.map(id => ({dataset:{id}})) }, isMacOS,
  };
  const keydown = createPanelKeydown(context);
  return { effects, input, actions, press(key, overrides = {}) {
    const event = { key, code: key.length === 1 ? (/\d/.test(key) ? 'Digit'+key : 'Key'+key.toUpperCase()) : key,
      target: focused ? input : row, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, repeat: false,
      isComposing: false, keyCode: 0, defaultPrevented: false, getModifierState: () => false,
      preventDefault() { this.defaultPrevented = true; }, ...overrides };
    keydown(event); return event;
  } };
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

for (const isMacOS of [false, true]) {
  const platform = isMacOS ? "macOS" : "Windows";
  for (const ctrlKey of [false, true]) {
    for (const code of ["Digit2", "Numpad2"]) {
      test(`${platform} ${ctrlKey ? "Ctrl+" : ""}${code} applies the visible ordinal outside search with or without keywords`, () => {
        for (const search of ["", "existing"]) {
          const { press, effects, input } = setup({ isMacOS, search });
          assert.equal(press("2", { code, ctrlKey }).defaultPrevented, true);
          assert.equal(input.value, search);
          assert.deepEqual(effects, [["apply", "visible-third"]]);
        }
      });
    }
  }
  for (const code of ["Digit2", "Numpad2"]) {
    test(`${platform} Ctrl+${code} applies the visible ordinal in focused search with or without keywords`, () => {
      for (const search of ["", "existing"]) {
        const { press, effects, input } = setup({ isMacOS, focused: true, search, selectedId: null });
        assert.equal(press("2", { code, ctrlKey: true }).defaultPrevented, true);
        assert.equal(input.value, search);
        assert.deepEqual(effects, [["apply", "visible-third"]]);
      }
    });
  }
}

for (const overrides of [
  { altKey: true }, { shiftKey: true }, { metaKey: true },
  { getModifierState: () => true }, { isComposing: true }, { keyCode: 229 },
]) {
  test(`digits with ${Object.keys(overrides)[0]} do not apply a record`, () => {
    for (const ctrlKey of [false, true]) {
      for (const focused of [false, true]) {
        const { press, effects } = setup({ focused });
        assert.equal(press("1", { ctrlKey, ...overrides }).defaultPrevented, false);
        assert.deepEqual(effects, []);
      }
    }
  });
}

test("holding a digit or Ctrl+digit applies only once", () => {
  for (const [ctrlKey, focused] of [[false, false], [true, false], [true, true]]) {
    const { press, effects } = setup({ focused });
    assert.equal(press("1", { ctrlKey }).defaultPrevented, true);
    assert.equal(press("1", { ctrlKey, repeat: true }).defaultPrevented, true);
    assert.deepEqual(effects, [["apply", "visible-second"]]);
  }
});

test("both digit shortcuts cover ordinals one through nine", () => {
  const visibleIds = Array.from({ length: 10 }, (_, index) => `visible-${index + 1}`);
  for (const ctrlKey of [false, true]) {
    const { press, effects } = setup({ visibleIds });
    for (let ordinal = 1; ordinal <= 9; ordinal++) press(String(ordinal), { ctrlKey });
    assert.deepEqual(effects, visibleIds.slice(0, 9).map(id => ["apply", id]));
    assert.equal(press("0", { ctrlKey }).defaultPrevented, false);
    assert.equal(effects.length, 9);
  }
});

test("zero and ordinals with no visible record do not apply anything", () => {
  for (const [ctrlKey, focused] of [[false, false], [true, false], [true, true]]) {
    const { press, effects } = setup({ focused });
    assert.equal(press("0", { ctrlKey }).defaultPrevented, false);
    assert.equal(press("9", { ctrlKey }).defaultPrevented, true);
    assert.deepEqual(effects, []);
    const empty = setup({ focused, visibleIds: [] });
    empty.press("1", { ctrlKey });
    assert.deepEqual(empty.effects, []);
  }
});

test("digit shortcuts preserve other editable controls", () => {
  for (const id of ["group-name", "textarea", "editable"]) {
    const target = { id, matches: () => false, closest: () => target };
    const { press, effects } = setup();
    for (const ctrlKey of [false, true]) {
      assert.equal(press("1", { target, ctrlKey }).defaultPrevented, false);
    }
    assert.deepEqual(effects, []);
  }
});

test("settings retain their input behavior", () => {
  const { press, effects } = setup({ settingsOpen: true });
  press("1");
  press("1", { ctrlKey: true });
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
