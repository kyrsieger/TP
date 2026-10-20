import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { buildFontSearchIndex, searchFontFamilies } = loadTs("src/domain/font-search.ts");
const family = (name, aliases = []) => ({ name, aliases, styles: [] });
const fonts = [family("霞鹜文楷", ["LXGW WenKai", "霞鹜文楷"]), family("微软雅黑", ["Microsoft YaHei"]), family("Arial"), family("华文行楷")];

test("Chinese font search supports initial prefixes, longer initials, and full pinyin", () => {
  const index = buildFontSearchIndex(fonts);
  for (const query of ["XW", "xw", "XWWK", "wk", "xiawu", "xiawuwenkai", "霞鹜", "文楷"]) {
    assert.deepEqual([...searchFontFamilies(index, query)], ["霞鹜文楷"], query);
  }
  for (const query of ["WRYH", "weiruan", "微软雅黑"]) assert.deepEqual([...searchFontFamilies(index, query)], ["微软雅黑"]);
  assert.deepEqual([...searchFontFamilies(buildFontSearchIndex([family("微信")]), "WX")], ["微信"]);
});

test("font aliases and English matching keep one canonical family and ignore case, width, and spaces", () => {
  const index = buildFontSearchIndex(fonts);
  for (const query of ["LXGW", "lxgw wenkai", "LXGW   WENKAI", "ＸＷ", " x w "]) assert.deepEqual([...searchFontFamilies(index, query)], ["霞鹜文楷"], query);
  assert.deepEqual([...searchFontFamilies(index, "MICROSOFT YAHEI")], ["微软雅黑"]);
  assert.deepEqual([...searchFontFamilies(index, "ARIAL")], ["Arial"]);
  assert.deepEqual([...searchFontFamilies(index, "does-not-exist")], []);
});

test("contextual pronunciations and mixed Chinese/English font names remain searchable", () => {
  const index = buildFontSearchIndex([family("重庆字体"), family("华文行楷"), family("LXGW 霞鹜文楷 GB"), family("霞雾文楷")]);
  assert.deepEqual([...searchFontFamilies(index, "CQZT")], ["重庆字体"]);
  assert.deepEqual([...searchFontFamilies(index, "HWXK")], ["华文行楷"]);
  assert.deepEqual([...searchFontFamilies(index, "LXGWXWWKGB")], ["LXGW 霞鹜文楷 GB"]);
  assert.deepEqual([...searchFontFamilies(index, "xiawu")], ["LXGW 霞鹜文楷 GB", "霞雾文楷"]);
});

test("empty searches preserve family order, default remains searchable, and missing saved fonts are retained", () => {
  const index = buildFontSearchIndex([...fonts, fonts[0]], "未安装字体");
  assert.deepEqual([...searchFontFamilies(index, "  ")], ["", ...fonts.map(font => font.name), "未安装字体"]);
  assert.deepEqual([...searchFontFamilies(index, "XTMR")], [""]);
  assert.deepEqual([...searchFontFamilies(index, "WAZZT")], ["未安装字体"]);
  assert.deepEqual([...searchFontFamilies(index, "xw")], ["霞鹜文楷"]);
  assert.deepEqual([...searchFontFamilies(index, "wryh")], ["微软雅黑"]);
  assert.deepEqual([...searchFontFamilies(index, "xw")], ["霞鹜文楷"]);
});
