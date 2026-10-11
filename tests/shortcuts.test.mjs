import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { recordRenameShortcutFromKey, recordRenameShortcutError } = loadTs("src/domain/shortcuts.ts");
const key = (key, overrides = {}) => ({ key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false,
  isComposing: false, keyCode: 0, getModifierState: () => false, ...overrides });

test("local shortcut capture accepts function keys and modified characters, protects IME and AltGraph", () => {
  assert.equal(recordRenameShortcutFromKey(key("F2")), "F2");
  assert.equal(recordRenameShortcutFromKey(key("r", { ctrlKey: true, shiftKey: true })), "Control+Shift+R");
  assert.equal(recordRenameShortcutFromKey(key("r", { metaKey: true })), "Super+R");
  for (const event of [key("a"), key("Enter"), key("F25"), key("Control"), key("F2", { isComposing: true }),
    key("r", { ctrlKey: true, getModifierState: () => true })]) assert.equal(recordRenameShortcutFromKey(event), null);
  assert.equal(recordRenameShortcutError("Control+D", ""), "此快捷键已用于其他操作，请更换按键。");
  assert.equal(recordRenameShortcutError("Control+Shift+R", "Control+Shift+R"), "此快捷键已用于其他操作，请更换按键。");
  assert.equal(recordRenameShortcutError("F2", ""), null);
  assert.equal(recordRenameShortcutError("", ""), null);
});
