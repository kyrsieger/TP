import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
function setup() {
  class Input { constructor(id) {this.id=id;this.selected=false;} select(){this.selected=true;} }
  class Textarea extends Input {}
  const {selectAllKeydown}=loadTs("src/input/keyboard.ts",{HTMLInputElement:Input,HTMLTextAreaElement:Textarea});
  return { Input,Textarea,dispatch(_name,target,overrides={}) {
    const event={target,key:"",ctrlKey:false,altKey:false,metaKey:false,shiftKey:false,getModifierState:()=>false,defaultPrevented:false,stopped:false,
      preventDefault(){this.defaultPrevented=true;},stopImmediatePropagation(){this.stopped=true;},...overrides};
    selectAllKeydown(event);return event;
  } };
}
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
