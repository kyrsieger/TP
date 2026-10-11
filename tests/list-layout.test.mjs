import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import ts from "typescript";

const { outputText } = ts.transpileModule(readFileSync(new URL("../src/list-layout.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS },
});
const context = { exports: {} };
runInNewContext(outputText, context);
const { listOffsets, visibleRange, rowScrollTop, backToTopOffset } = context.exports;

test("mixed media and text rows retain their full scroll height", () => {
  const offsets = listOffsets(["text", "image", "file", "video", "link"].map(kind => ({ kind })));
  assert.deepEqual([...offsets], [0, 48, 228, 276, 456, 504]);
  assert.deepEqual({ ...visibleRange(offsets, 48, 180, 0) }, { start: 1, end: 2 });
  assert.deepEqual({ ...visibleRange(offsets, 227, 50, 0) }, { start: 1, end: 4 });
});

test("named media adds title space without changing ordinary row heights", () => {
  const offsets = listOffsets([{ kind: "text", title: "标题" }, { kind: "image", title: "图片标题" }, { kind: "video" }, { kind: "video", title: "视频标题" }]);
  assert.deepEqual([...offsets], [0, 48, 260, 440, 652]);
  assert.equal(rowScrollTop(offsets, 3, 0, 400), 252);
});

test("a long history creates only viewport rows and the small overscan", () => {
  const offsets = listOffsets(Array.from({ length: 5000 }, () => ({ kind: "text" })));
  for (const scrollTop of [0, 240, 120000, 239520]) {
    const { start, end } = visibleRange(offsets, scrollTop, 480);
    assert.ok(end - start <= 19);
    assert.ok(offsets[start] <= scrollTop);
    assert.ok(offsets[end] >= Math.min(offsets[5000], scrollTop + 480));
  }
  assert.deepEqual({ ...visibleRange([0], 0, 480) }, { start: 0, end: 0 });
});

test("keyboard scrolling mounts a distant row while preserving nearby positions", () => {
  const offsets = listOffsets(Array.from({ length: 100 }, (_, index) => ({ kind: index % 2 ? "image" : "text" })));
  const height = 400;
  let scrollTop = rowScrollTop(offsets, 99, 0, height);
  assert.equal(scrollTop, offsets[100] - height);
  let range = visibleRange(offsets, scrollTop, height);
  assert.ok(range.start <= 99 && range.end > 99);
  assert.equal(rowScrollTop(offsets, 98, scrollTop, height), scrollTop);
  scrollTop = rowScrollTop(offsets, 0, scrollTop, height);
  assert.equal(scrollTop, 0);
  range = visibleRange(offsets, scrollTop, height);
  assert.ok(range.start === 0 && range.end < 100);
});

test("back-to-top threshold counts all rows even when its target is unmounted", () => {
  const offsets = listOffsets(Array.from({ length: 100 }, (_, index) => ({ kind: index === 1 ? "image" : "text" })));
  assert.equal(backToTopOffset(offsets, 276), offsets[12]);
  assert.equal(backToTopOffset(listOffsets([{ kind: "text" }]), 480), null);
});
