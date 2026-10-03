import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("main.ts", readFileSync(new URL("../src/main.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
const statements = source.statements.filter(statement => {
  if (ts.isFunctionDeclaration(statement)) return statement.name?.text === "saveNumberSetting";
  if (!ts.isExpressionStatement(statement) || !ts.isCallExpression(statement.expression)) return false;
  const call = statement.expression;
  return call.expression.getText(source) === "root.addEventListener" && ["input", "change", "focusout", "keydown"].includes(call.arguments[0]?.text)
    || call.expression.getText(source) === "document.addEventListener" && call.arguments[2]?.kind === ts.SyntaxKind.TrueKeyword;
});
const { outputText } = ts.transpileModule(statements.map(statement => statement.getText(source)).join("\n"), { compilerOptions: { target: ts.ScriptTarget.ES2020 } });

function setup() {
  const saved = [];
  const messages = [];
  const listeners = new Map();
  const timers = new Map();
  let timerId = 0;
  class TextField {
    constructor(id, value = "99") { this.id = id; this.value = value; this.validity = { valid: true }; this.selected = false; }
    closest() { return null; }
    matches() { return false; }
    select() { this.selected = true; }
  }
  class Input extends TextField {}
  class Textarea extends TextField {}
  const context = {
    HTMLInputElement: Input, HTMLTextAreaElement: Textarea,
    root: { addEventListener: (name, callback) => listeners.set(name, [...(listeners.get(name) ?? []), callback]) },
    document: { addEventListener: (_name, callback) => listeners.set("capture-keydown", [callback]) },
    window: {
      setTimeout: callback => { timers.set(++timerId, callback); return timerId; },
      clearTimeout: id => timers.delete(id),
    },
    numberSaveTimers: new Map(), state: { settings: { maxOrdinary: 100, sensitiveSeconds: 90 } },
    saveSetting: patch => saved.push(JSON.parse(JSON.stringify(patch))), toast: message => messages.push(message),
  };
  runInNewContext(outputText, context);
  return {
    Input, Textarea, saved, messages,
    dispatch(name, target, overrides = {}) {
      const event = {
        target, key: "", ctrlKey: false, altKey: false, metaKey: false, shiftKey: false,
        repeat: false, isComposing: false, getModifierState: () => false,
        defaultPrevented: false, stopped: false,
        preventDefault() { this.defaultPrevented = true; },
        stopPropagation() { this.stopped = true; },
        stopImmediatePropagation() { this.stopped = true; },
        ...overrides,
      };
      if (name === "keydown") for (const callback of listeners.get("capture-keydown") ?? []) callback(event);
      if (!event.stopped) for (const callback of listeners.get(name) ?? []) callback(event);
      return event;
    },
    runTimers() { for (const [id, callback] of timers) { timers.delete(id); callback(); } },
  };
}

test("typing a lower record limit and pausing does not save or request deletion", () => {
  const { Input, saved, dispatch, runTimers } = setup();
  const input = new Input("max-ordinary");
  dispatch("input", input);
  runTimers();
  assert.deepEqual(saved, []);
});

test("number spinner change does not submit the record limit before confirmation", () => {
  const { Input, saved, dispatch } = setup();
  dispatch("change", new Input("max-ordinary"));
  assert.deepEqual(saved, []);
});

test("Enter submits the finished record limit", () => {
  const { Input, saved, dispatch } = setup();
  const event = dispatch("keydown", new Input("max-ordinary"), { key: "Enter" });
  assert.deepEqual(saved, [{ maxOrdinary: 99 }]);
  assert.equal(event.defaultPrevented, true);
});

test("leaving the record limit input submits the finished value", () => {
  const { Input, saved, dispatch } = setup();
  dispatch("focusout", new Input("max-ordinary"));
  assert.deepEqual(saved, [{ maxOrdinary: 99 }]);
});

test("confirming an invalid record limit restores the saved value", () => {
  const { Input, saved, messages, dispatch } = setup();
  const input = new Input("max-ordinary", "");
  dispatch("focusout", input);
  assert.deepEqual(saved, []);
  assert.equal(input.value, "100");
  assert.deepEqual(messages, ["请输入有效的设置值"]);
});

test("sensitive record seconds retain delayed saving", () => {
  const { Input, saved, dispatch, runTimers } = setup();
  dispatch("input", new Input("sensitive", "60"));
  assert.deepEqual(saved, []);
  runTimers();
  assert.deepEqual(saved, [{ sensitiveSeconds: 60 }]);
});

for (const id of ["search", "max-ordinary", "sensitive", "group-name"]) {
  test(`Ctrl+A selects the text in ${id}`, () => {
    const { Input, dispatch } = setup();
    const input = new Input(id);
    dispatch("keydown", input, { key: "a", ctrlKey: true });
    assert.equal(input.selected, true);
  });
}

test("Ctrl+A selects textarea text", () => {
  const { Textarea, dispatch } = setup();
  const textarea = new Textarea("text");
  dispatch("keydown", textarea, { key: "a", ctrlKey: true });
  assert.equal(textarea.selected, true);
});

test("Ctrl+A remains blocked on the record list", () => {
  const { dispatch } = setup();
  const event = dispatch("keydown", { matches: () => false }, { key: "a", ctrlKey: true });
  assert.equal(event.defaultPrevented, true);
  assert.equal(event.stopped, true);
});
