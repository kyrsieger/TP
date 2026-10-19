import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { searchExcerpt, prepareSearchExcerpt } = loadTs("src/search-excerpt.ts");
const { recordListText } = loadTs("src/domain/records.ts");
const measure = text => Array.from(text).reduce((sum, char) => sum + (char === "宽" ? 2 : 1), 0);

test("the reported short path stays complete when it fits", () => {
  const text = '"D:\\Workspace\\project\\micai-ohoskills"';
  assert.equal(recordListText({ body: text, files: [] }, "micai-"), text);
  assert.equal(searchExcerpt(text, "micai-", 80, measure), text);
});

test("long content keeps the match and context within the measured width", () => {
  for (const [before, after] of [[0, 150], [150, 0], [100, 100]]) {
    const text = "宽".repeat(before) + "MiCaI-" + "z".repeat(after);
    const result = searchExcerpt(text, "micai-", 40, measure);
    assert.ok(result.includes("MiCaI-"));
    assert.ok(measure(result) <= 40);
    assert.equal(result.startsWith("…"), before > 0);
    assert.equal(result.endsWith("…"), after > 0);
    assert.ok(measure(result) >= 38);
  }
});

test("resizing can restore the beginning and then the full content", () => {
  const text = "a".repeat(30) + "match" + "b".repeat(30);
  assert.ok(searchExcerpt(text, "match", 20, measure).startsWith("…"));
  assert.ok(searchExcerpt(text, "match", 40, measure).startsWith("a"));
  assert.equal(searchExcerpt(text, "match", 65, measure), text);
});

test("a wide keyword takes priority over preceding context", () => {
  const result = searchExcerpt("before".repeat(20) + "longkeyword" + "after".repeat(20), "longkeyword", 15, measure);
  assert.ok(result.includes("longkeyword"));
  assert.ok(measure(result) <= 15);
});

test("Unicode and case expansion preserve complete characters and the match", () => {
  for (const [text, query, match] of [["😀".repeat(30) + "MATCH" + "😀".repeat(30), "match", "MATCH"],
    ["İ" + "a".repeat(40) + "MATCH" + "z".repeat(40), "match", "MATCH"]]) {
    const result = searchExcerpt(text, query, 20, measure);
    assert.ok(result.includes(match));
    assert.ok(result.isWellFormed());
    assert.ok(measure(result) <= 20);
  }
});

test("title-only and non-search rows retain their usual text; path matches use the full source", () => {
  const file = { body: "", files: [{ original: "D:\\Workspace\\micai\\notes.md" }] };
  assert.equal(recordListText(file, "title"), "notes.md");
  assert.equal(recordListText(file, "micai"), file.files[0].original);
  assert.equal(recordListText({ body: "first\nsecond", files: [] }, ""), "first ...");
});

test("million-character records only measure candidates near the visible width", () => {
  for (const [before, after] of [[0, 1_000_000], [1_000_000, 0], [500_000, 500_000]]) {
    const fit = prepareSearchExcerpt("宽".repeat(before) + "MATCH" + "😀".repeat(after), "match");
    for (const width of [40, 20, 80, 40]) {
      let total = 0, longest = 0;
      const result = fit(width, value => {
        total += value.length;
        longest = Math.max(longest, value.length);
        return measure(value);
      });
      assert.ok(result.includes("MATCH"));
      assert.ok(result.isWellFormed());
      assert.ok(measure(result) <= width);
      assert.ok(longest < 500, `measured a ${longest}-character candidate`);
      assert.ok(total < 10_000, `measured ${total} characters for a ${width}-unit row`);
    }
  }
});

test("a prepared match retains Unicode case mapping while resizing", () => {
  const text = "İ" + "😀".repeat(100) + "MATCH" + "宽".repeat(100);
  const fit = prepareSearchExcerpt(text, "match");
  for (const width of [20, 40, 1000, 20]) {
    const result = fit(width, measure);
    assert.ok(result.includes("MATCH"));
    assert.ok(result.isWellFormed());
    assert.ok(measure(result) <= width);
    if (width === 1000) assert.equal(result, text);
  }
});

test("a hidden row avoids measuring and rendering a full search result", () => {
  assert.equal(searchExcerpt("before MATCH after", "match", 0, () => { throw new Error("hidden row measured"); }), "");
});
