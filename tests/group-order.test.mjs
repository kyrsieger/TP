import test from 'node:test';
import assert from 'node:assert/strict';
import { loadTs } from './load-ts.mjs';
const { GroupController } = loadTs('src/state/groups.ts');

function setup(call) {
  const messages = [];
  const panel = { value: { view: 'group:1', selectedId: 'record', snapshot: { groups: [{ id: 1, name: '工作' }, { id: 2, name: '资料' }] } },
    update(patch) { Object.assign(this.value, patch); }, async refresh() {} };
  return { panel, messages, controller: new GroupController(panel, call, message => messages.push(message)) };
}
test('failed group order save restores order and preserves browsing state', async () => {
  let reject;
  const { panel, controller, messages } = setup(() => new Promise((_, fail) => { reject = fail; }));
  const saving = controller.reorder([2, 1]);
  assert.deepEqual(panel.value.snapshot.groups.map(group => group.id), [2, 1]);
  reject(new Error('保存失败')); await saving;
  assert.deepEqual(Array.from(panel.value.snapshot.groups, group => group.id), [1, 2]);
  assert.equal(panel.value.view, 'group:1'); assert.equal(panel.value.selectedId, 'record');
  assert.equal(controller.value.saving, false); assert.match(messages[0], /保存失败/);
});
test('unchanged or invalid group order does not save', async () => {
  let calls = 0;
  const { controller } = setup(async () => { calls++; });
  for (const ids of [[1, 2], [1, 1], [1], [1, 3]]) await controller.reorder(ids);
  assert.equal(calls, 0);
});
