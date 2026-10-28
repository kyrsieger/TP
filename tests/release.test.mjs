import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { extractNotes, publishDraft, selectCiRun, validateVersions, versionFromTag, waitForCi } from '../scripts/release.mjs';

test('release accepts stable version tags and rejects malformed or prerelease tags', () => {
  assert.equal(versionFromTag('v1.0.22'), '1.0.22');
  for (const tag of ['1.0.22', 'v01.0.22', 'v1.0', 'v1.0.22-beta.1', 'v1.0.22/other']) {
    assert.throws(() => versionFromTag(tag));
  }
});

test('release extracts only the requested version, preserves subsections and handles CRLF', () => {
  const text = '\uFEFF# 更新日志\r\n\r\n## [1.0.23] - 2026-10-28\r\nnew\r\n## [1.0.22] - 2026-10-27\r\n\r\n### 修复\r\n- 修复预览。\r\n\r\n## [1.0.21]\r\nold';
  assert.equal(extractNotes(text, '1.0.22'), '### 修复\n- 修复预览。\n');
  assert.equal(extractNotes('## v1.0.22\n- Change\n', '1.0.22'), '- Change\n');
  assert.equal(extractNotes('## 1.0.22 - 2026-10-28\n- Change', '1.0.22'), '- Change\n');
});

test('release refuses missing, duplicate, empty and heading-only CHANGELOG sections', () => {
  for (const text of ['## [1.0.220]\nwrong version', '## [1.0.22]\n## [1.0.21]\nold',
    '## [1.0.22]\n### 修复\n', '## [1.0.22]\na\n## v1.0.22\nb']) {
    assert.throws(() => extractNotes(text, '1.0.22'));
  }
});

test('release does not interpret headings inside fenced code as release boundaries', () => {
  const text = '## [1.0.22]\nExample:\n```md\n## [1.0.21]\n```\n- Fix\n## [1.0.21]\nold';
  assert.equal(extractNotes(text, '1.0.22'), 'Example:\n```md\n## [1.0.21]\n```\n- Fix\n');
});

test('release verifies all version files, including the real Cargo package section', () => {
  const json = path => JSON.parse(readFileSync(path, 'utf8'));
  const pkg = json('package.json');
  const args = [pkg, json('package-lock.json'), json('src-tauri/tauri.conf.json'),
    readFileSync('src-tauri/Cargo.toml', 'utf8'), readFileSync('src-tauri/Cargo.lock', 'utf8')];
  validateVersions(pkg.version, ...args);
  assert.throws(() => validateVersions('99.0.0', ...args));
  assert.throws(() => validateVersions(pkg.version, ...args.slice(0, 3), '[package]\nname = "tp"\n[dependencies]\nversion = "' + pkg.version + '"\n', args[4]));
  assert.throws(() => validateVersions(pkg.version, ...args.slice(0, 4), '[[package]]\nname = "tp"\nversion = "0.0.0"\n'));
});

const run = overrides => ({ id: 10, run_attempt: 1, head_sha: 'abc', head_branch: 'main', event: 'push',
  status: 'completed', conclusion: 'success', html_url: 'https://example.test/run/10', ...overrides });

test('release ignores PR, manual, other branch and other commit results; chooses newest main push run', () => {
  assert.equal(selectCiRun([run({ id: 12, status: 'in_progress', conclusion: null }), run(),
    run({ id: 15, event: 'pull_request' }), run({ id: 16, head_sha: 'def' }),
    run({ id: 17, head_branch: 'feature' }), run({ id: 18, event: 'workflow_dispatch' })], 'abc').id, 12);
  assert.equal(selectCiRun([run({ head_sha: 'def' })], 'abc'), undefined);
});

function clock() {
  let time = 0;
  return { now: () => time, pause: async ms => { time += ms; }, timeoutMs: 100, intervalMs: 10, log: () => {} };
}

test('release waits for a run to appear and succeed without rerunning tests', async () => {
  const responses = [[], [run({ status: 'queued', conclusion: null })],
    [run({ status: 'in_progress', conclusion: null })], [run()]];
  let queries = 0;
  const result = await waitForCi({ ...clock(), sha: 'abc', listRuns: async () => { queries++; return responses.shift(); } });
  assert.equal(result.conclusion, 'success');
  assert.equal(queries, 4);
});

test('release proceeds immediately when the matching CI already passed', async () => {
  await waitForCi({ ...clock(), sha: 'abc', listRuns: async () => [run()],
    pause: async () => assert.fail('should not wait') });
});

test('release stops on failed, cancelled, skipped or timed out CI', async () => {
  for (const conclusion of ['failure', 'cancelled', 'skipped', 'timed_out', 'neutral']) {
    await assert.rejects(waitForCi({ ...clock(), sha: 'abc', listRuns: async () => [run({ conclusion })] }), /did not succeed/);
  }
});

test('release times out on missing or unfinished CI, and propagates API errors', async () => {
  for (const runs of [[], [run({ status: 'in_progress', conclusion: null })]]) {
    await assert.rejects(waitForCi({ ...clock(), sha: 'abc', listRuns: async () => runs }), /Timed out/);
  }
  await assert.rejects(waitForCi({ ...clock(), sha: 'abc', listRuns: async () => { throw new Error('API denied'); } }), /API denied/);
});

const draftOptions = { tag: 'v1.0.22', repo: 'owner/tp', paths: ['setup.exe', 'setup.msi'], notesPath: 'notes with spaces.md' };

test('published releases cannot be edited or have installers overwritten', () => {
  assert.throws(() => publishDraft({ ...draftOptions, releases: [{ tag_name: 'v1.0.22', draft: false }],
    execute: () => assert.fail('must not mutate published release') }), /already published/);
});

test('first release creation stays draft and uses prepared notes and both installers', () => {
  const calls = [];
  publishDraft({ ...draftOptions, releases: [], execute: (program, args) => calls.push({ program, args }) });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].program, 'gh');
  const args = calls[0].args;
  assert.deepEqual(args.slice(0, 5), ['release', 'create', 'v1.0.22', 'setup.exe', 'setup.msi']);
  assert.ok(args.includes('--draft'));
  assert.ok(args.includes('--verify-tag'));
  assert.equal(args[args.indexOf('--notes-file') + 1], draftOptions.notesPath);
  assert.ok(!args.includes('--generate-notes'));
});

test('retry updates the existing draft and uploads installers without publishing', () => {
  const calls = [];
  publishDraft({ ...draftOptions, releases: [{ tag_name: 'v1.0.22', draft: true }],
    execute: (_program, args) => calls.push(args) });
  assert.equal(calls.length, 2);
  assert.equal(calls[0][1], 'edit');
  assert.ok(calls[0].includes('--draft'));
  assert.equal(calls[1][1], 'upload');
  assert.ok(calls[1].includes('--clobber'));
});

test('draft update errors stop the installer upload', () => {
  let count = 0;
  assert.throws(() => publishDraft({ ...draftOptions, releases: [{ tag_name: 'v1.0.22', draft: true }],
    execute: () => { count++; throw new Error('Permission denied'); } }), /Permission denied/);
  assert.equal(count, 1);
});
