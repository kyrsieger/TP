import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

const family = name => ({ name, aliases: [name], styles: [] });

test("font catalog shares pending reads and reuses successful data across consumers", async () => {
  let finish, calls = 0;
  const pending = new Promise(resolve => { finish = resolve; });
  const { getCachedSystemFonts, loadSystemFonts } = loadTs("src/services/font-catalog.ts", {}, {
    "./desktop": { call: () => { calls++; return pending; } },
  });
  assert.equal(getCachedSystemFonts(), null);
  const startup = loadSystemFonts();
  assert.equal(loadSystemFonts(), startup);
  assert.equal(loadSystemFonts(true), startup);
  const unsorted = [family("Z"), family("A")];
  finish(unsorted);
  const fonts = await startup;
  assert.equal(fonts.map(font => font.name).join(","), "A,Z");
  assert.equal(unsorted[0].name, "Z");
  assert.equal(await loadSystemFonts(), fonts);
  assert.equal(getCachedSystemFonts(), fonts);
  assert.equal(calls, 1);
});

test("catalog refresh retains known styles while pending or failed and can retry", async () => {
  let read = async () => [family("Installed")], calls = 0;
  const { getCachedSystemFonts, loadSystemFonts } = loadTs("src/services/font-catalog.ts", {}, {
    "./desktop": { call: () => { calls++; return read(); } },
  });
  const original = await loadSystemFonts();
  let fail;
  read = () => new Promise((_resolve, reject) => { fail = reject; });
  const refresh = loadSystemFonts(true);
  assert.equal(getCachedSystemFonts(), original);
  fail(Error("unavailable"));
  await assert.rejects(refresh, /unavailable/);
  assert.equal(await loadSystemFonts(), original);
  read = async () => [];
  await loadSystemFonts(true);
  assert.equal(getCachedSystemFonts().length, 0);
  await loadSystemFonts();
  assert.equal(calls, 3);
});

test("an initial font catalog failure does not prevent the next read", async () => {
  let calls = 0;
  const { getCachedSystemFonts, loadSystemFonts } = loadTs("src/services/font-catalog.ts", {}, {
    "./desktop": { call: async () => { if (++calls === 1) throw Error("unavailable"); return [family("Recovered")]; } },
  });
  await assert.rejects(loadSystemFonts(), /unavailable/);
  assert.equal(getCachedSystemFonts(), null);
  assert.equal((await loadSystemFonts())[0].name, "Recovered");
});
