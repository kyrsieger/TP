import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { loadTs } from "./load-ts.mjs";
const { hexToLab, deltaE2000 } = loadTs("src/domain/color-distance.ts");
const { themes, sortedThemes, themeDistance, themeSwatches, getTheme, sortThemes } = loadTs("src/domain/themes.ts");

test("CIEDE2000 matches all 34 Sharma/Wu/Dalal reference pairs", () => {
  // https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/dataNprograms/ciede2000testdata.txt
  const rows = readFileSync("tests/fixtures/ciede2000-reference.txt", "utf8").trim().split(/\r?\n/);
  assert.equal(rows.length, 34);
  for (const row of rows) {
    const values = row.trim().split(/\s+/).map(Number);
    const actual = deltaE2000(values.slice(0,3), values.slice(3,6));
    assert.ok(Math.abs(actual-values[6]) < .00005, `${row}: ${actual}`);
    assert.ok(Math.abs(deltaE2000(values.slice(3,6), values.slice(0,3))-actual)<1e-9);
  }
  assert.ok(Math.abs(hexToLab("#FFFFFF")[0]-100) < .0001);
  assert.equal(hexToLab("#000000")[0], 0);
});

test("approved themes retain their seven review swatches and unknown themes fall back safely", () => {
  const selected = JSON.parse(readFileSync("tests/fixtures/approved-theme-palettes.json", "utf8"));
  assert.equal(themes.length, 11);
  selected.forEach((palette,i)=>assert.deepEqual([...themeSwatches(themes[i+1])], [palette.accent,palette.bg,palette.surface,palette.control,palette.soft,palette.text,palette.muted]));
  assert.equal(getTheme("missing").id, "indigo");
});

test("ordering starts at green, keeps dark themes last, and always follows the nearest remaining palette", () => {
  assert.equal(sortedThemes[0].id,"forest-mist");
  assert.equal(new Set(sortedThemes.map(t=>t.id)).size,11);
  let dark=false;
  for(let i=1;i<sortedThemes.length;i++) {
    const current=sortedThemes[i]; dark ||= current.dark;
    assert.ok(!dark || current.dark);
    const rest=sortedThemes.slice(i).filter(t=>t.dark===current.dark);
    assert.ok(rest.every(t=>themeDistance(sortedThemes[i-1],current)<=themeDistance(sortedThemes[i-1],t)+1e-9));
  }
  assert.deepEqual([...sortThemes([...themes].reverse()).map(t=>t.id)], [...sortedThemes.map(t=>t.id)]);
});

function setup(save=async()=>({status:"saved"})) {
  const timers=new Map();let next=0;const writes=[],errors=[];
  const {SettingsController}=loadTs("src/state/settings.ts",{setTimeout: fn=>{timers.set(++next,fn);return next},clearTimeout: id=>timers.delete(id)});
  const history={value:{active:true,settings:{theme:"indigo",maxOrdinary:300,retentionDays:7}},commitSettings(settings){this.value.settings=settings}};
  const controller=new SettingsController(history,async(name,args)=>{writes.push(args.value);return save(args.value)}, {toast:error=>errors.push(error)});
  return {controller,history,writes,errors};
}

test("hover changes immediately and fast hovers coalesce into one saved choice", async()=>{
  const {controller,history,writes}=setup();
  controller.selectTheme("forest-mist");assert.equal(controller.value.themeDraft,"forest-mist");assert.equal(writes.length,0);
  controller.selectTheme("olive");controller.selectTheme("oat");controller.flushTheme();
  await controller.save({});
  assert.equal(writes.length,1);assert.equal(history.value.settings.theme,"oat");assert.equal(controller.value.themeDraft,null);
});

test("a slow earlier save never replaces the latest hovered theme",async()=>{
  let finish;const first=new Promise(resolve=>{finish=resolve});
  const {controller,history,writes}=setup(async()=>writes.length===1?first:{status:"saved"});
  controller.selectTheme("forest-mist");controller.flushTheme();await Promise.resolve();
  controller.selectTheme("violet");controller.flushTheme();
  finish({status:"saved"});await Promise.resolve();await Promise.resolve();
  assert.equal(controller.value.themeDraft,"violet");
  await controller.save({});assert.equal(history.value.settings.theme,"violet");assert.equal(controller.value.themeDraft,null);
});

test("failed theme saves restore the saved theme; closing flushes the last hover",async()=>{
  const bad=setup(async()=>{throw Error("disk full")});
  bad.controller.selectTheme("clay");bad.controller.reset();await bad.controller.save({});
  assert.equal(bad.history.value.settings.theme,"indigo");assert.equal(bad.controller.value.themeDraft,null);assert.match(bad.errors[0],/disk full/);
  const good=setup();good.controller.selectTheme("charcoal");good.controller.reset();await good.controller.save({});
  assert.equal(good.history.value.settings.theme,"charcoal");
});

test("preview startup restores its saved theme and ignores a stale reply after a live change",async()=>{
  for(const live of [false,true]) {
    let listener,finish;const pending=new Promise(resolve=>{finish=resolve});
    const root={dataset:{},style:{colorScheme:'',setProperty(){}}};
    const {startThemeSync}=loadTs("src/services/theme.ts",{document:{documentElement:root}}, {
      "@tauri-apps/api/event":{listen:async(_event,callback)=>{listener=callback;return ()=>{}}},
      "./desktop":{call:()=>pending},
    });
    const startup=startThemeSync();await Promise.resolve();
    if(live)listener({payload:'forest-mist'});
    finish('violet');await startup;
    assert.equal(root.dataset.theme,live?'forest-mist':'violet');
    assert.equal(root.style.colorScheme,live?'light':'dark');
  }
});
