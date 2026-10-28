import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";
const { TransferController } = loadTs("src/state/transfer.ts");
const preview = { token: "prepared", records: 200, groups: 2, cleanupCount: 30, currentMaxOrdinary: 100, nextMaxOrdinary: 300, warnings: ["文件已不可用"] };
function setup(invoke) {
  const calls = [], toasts = []; let refreshed = 0;
  const controller = new TransferController({ refresh: async () => { refreshed++; } }, async (name, args) => {
    calls.push({ name, args }); return invoke(name, args);
  }, message => toasts.push(message), task => task());
  return { controller, calls, toasts, get refreshed() { return refreshed; } };
}
test("the unified export sends no format selector, and cancelling reports no success", async () => {
  const run = setup(() => null);
  run.controller.requestExport(); await run.controller.confirm();
  assert.equal(run.calls[0].name, "export_data"); assert.equal(run.calls[0].args, undefined);
  assert.equal(run.controller.value.mode, null);
  run.controller.requestExport(); await run.controller.confirm();
  assert.equal(run.calls[1].args, undefined); assert.equal(run.toasts.length, 0);
});
test("in-flight exports reject duplicate submission and failed exports can retry", async () => {
  let finish; const pending = new Promise(resolve => { finish = resolve; });
  const run = setup(async () => { await pending; throw new Error("disk full"); });
  run.controller.requestExport(); const saving = run.controller.confirm();
  await run.controller.confirm(); run.controller.requestExport(); run.controller.cancel();
  assert.equal(run.calls.length, 1); assert.equal(run.controller.value.mode, "export");
  finish(); await saving;
  assert.equal(run.controller.value.busy, false); assert.match(run.controller.value.error, /disk full/);
});
test("restore is never invoked before confirmation and sends the exact reviewed count and capacity", async () => {
  const run = setup(name => name === "inspect_backup" ? preview : { status: "completed", records: 200, cleaned: 0 });
  await run.controller.inspect(); assert.equal(run.calls.length, 1);
  assert.equal(run.controller.value.keepExpired, false);
  run.controller.update({ keepExpired: true }); await run.controller.confirm();
  assert.deepEqual(JSON.parse(JSON.stringify(run.calls[1])), { name: "restore_backup", args: { token: "prepared", keepExpired: true, expectedCleanupCount: 30, expectedMaxOrdinary: 300 } });
  assert.equal(run.refreshed, 1); assert.equal(run.controller.value.mode, null);
});
test("a changed restore plan requires another explicit confirmation", async () => {
  const changed = { ...preview, cleanupCount: 31, nextMaxOrdinary: 400 };
  const run = setup(name => name === "inspect_backup" ? preview : { status: "confirmation_required", preview: changed });
  await run.controller.inspect(); await run.controller.confirm();
  assert.equal(run.calls.length, 2); assert.equal(run.controller.value.preview.cleanupCount, 31);
  assert.equal(run.controller.value.mode, "import"); assert.equal(run.refreshed, 0);
  assert.match(run.controller.value.error, /再次确认/);
});
test("cancelling restore discards extracted temporary files", async () => {
  const run = setup(name => name === "inspect_backup" ? preview : undefined);
  await run.controller.inspect(); run.controller.cancel();
  assert.equal(run.calls[1].name, "discard_backup"); assert.equal(run.calls[1].args.token, "prepared");
  assert.equal(run.controller.value.preview, null); assert.equal(run.controller.value.mode, null);
});
test("hiding during inspection discards the late preview without reopening a confirmation", async () => {
  let finish; const pending = new Promise(resolve => { finish = resolve; });
  const run = setup(name => name === "inspect_backup" ? pending : undefined);
  const inspecting = run.controller.inspect(); run.controller.reset(); finish(preview); await inspecting;
  assert.equal(run.controller.value.mode, null); assert.equal(run.controller.value.preview, null);
  assert.equal(run.calls[1].name, "discard_backup"); assert.equal(run.controller.value.busy, false);
});
test("a failed restore requires a new inspection and reports the failure", async () => {
  const run = setup(name => { if (name === "inspect_backup") return preview; throw new Error("backup failed"); });
  await run.controller.inspect(); await run.controller.confirm();
  assert.equal(run.controller.value.mode, null); assert.equal(run.refreshed, 0);
  assert.match(run.toasts[0], /backup failed/);
});
