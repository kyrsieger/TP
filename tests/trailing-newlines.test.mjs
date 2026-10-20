import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

const { textForUse, hasTrailingNewline, recordListText } = loadTs("src/domain/records.ts");

test("display removes trailing whitespace and preserves indentation and inner lines", () => {
  for (const [original, expected] of [
    ["DSH 0.2.1-rc.2\r\n", "DSH 0.2.1-rc.2"],
    ["\n    first\r\n\tsecond\r\n\r\n", "\n    first\r\n\tsecond"],
    ["first\rsecond\r", "first\rsecond"],
    ["text  \n", "text"], ["text\n  ", "text"], ["\r\n\n\r", ""], ["", ""],
    ["text ", "text"], ["text  ", "text"], ["text\t\r\n \t", "text"],
    ["text\u00a0\u0085\u3000", "text"], [" \t\r\n\u00a0", ""],
    ["  first  \n\tsecond \t\n", "  first  \n\tsecond"], ["text\u200b", "text\u200b"],
  ]) {
    assert.equal(textForUse(original), expected);
    assert.equal(textForUse(original, false), original);
    assert.equal(hasTrailingNewline(original), /[\r\n]$/.test(original));
  }
});

test("list search excerpts use processed text without mutating the original", () => {
  const record = { kind: "text", body: "    first\r\n\tsecond\r\n", files: [] };
  assert.equal(recordListText(record, "second"), "    first\r\n\tsecond");
  assert.equal(recordListText(record, "second", false), record.body);
  assert.equal(recordListText(record, ""), "    first ...");
  assert.equal(record.body, "    first\r\n\tsecond\r\n");
});
