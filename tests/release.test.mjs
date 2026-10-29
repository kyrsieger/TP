import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { extractNotes, publishRelease, selectCiRun, validateVersions, versionFromTag, discoverCandidates, releaseState, validateCheckout } from '../scripts/release.mjs';

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

const sha = 'a'.repeat(40);
const tag = 'v1.0.22';
const repo = 'owner/tp';
test('checkout must match both the selected CI commit and the peeled version tag', () => {
  validateCheckout(sha, sha, sha);
  assert.throws(() => validateCheckout('b'.repeat(40), sha, sha), /selected release commit/);
  assert.throws(() => validateCheckout(sha, sha, 'b'.repeat(40)), /version tag/);
  assert.throws(() => validateCheckout(sha, undefined, sha), /selected release commit/);
});
const completedEvent = overrides => ({ workflow_run: { ...run({ head_sha: sha }),
  head_repository: { full_name: repo }, ...overrides } });
function discovery(overrides = {}) {
  return { eventName: 'push', event: { ref: `refs/tags/${tag}` }, repo,
    resolveTag: value => { assert.equal(value, tag); return sha; },
    tagsAt: value => { assert.equal(value, sha); return [tag]; },
    assertMainAncestor: value => assert.equal(value, sha),
    listRuns: value => { assert.equal(value, sha); return [run({ head_sha: sha })]; },
    listReleases: () => [], ...overrides };
}

test('tag before CI returns immediately; completion event discovers the same version', () => {
  let queries = 0;
  assert.deepEqual(discoverCandidates(discovery({ listRuns: () => { queries++; return []; } })), []);
  assert.equal(queries, 1);
  assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent() })), [{ tag, sha }]);
});

test('CI before tag does nothing; later tag push discovers the ready version', () => {
  assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent(),
    tagsAt: () => [], listRuns: () => assert.fail('no need to query without a tag') })), []);
  assert.deepEqual(discoverCandidates(discovery()), [{ tag, sha }]);
});

test('no release for missing, running, failed, cancelled or unrelated CI; retry can succeed', () => {
  for (const runs of [[], [run()], [run({ head_sha: sha, event: 'pull_request' })],
    [run({ head_sha: sha, event: 'workflow_dispatch' })],
    ...['failure', 'cancelled', 'timed_out', 'skipped', 'neutral'].map(conclusion => [run({ head_sha: sha, conclusion })]),
    [run({ head_sha: sha, status: 'in_progress', conclusion: null })]]) {
    assert.deepEqual(discoverCandidates(discovery({ listRuns: () => runs })), []);
  }
  // A stale success event cannot override a newer run, or a rerun in progress.
  assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent(),
    listRuns: () => [run({ head_sha: sha }), run({ id: 11, head_sha: sha, conclusion: 'failure' })] })), []);
  assert.deepEqual(discoverCandidates(discovery({ listRuns: () => [run({ head_sha: sha, run_attempt: 2,
    status: 'in_progress', conclusion: null })] })), []);
  assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent({ run_attempt: 2 }) })), [{ tag, sha }]);
});

test('completion events must come from successful main push CI in this repository', () => {
  for (const overrides of [{ event: 'pull_request' }, { event: 'workflow_dispatch' },
    { head_branch: 'feature' }, { conclusion: 'failure' }, { status: 'in_progress' },
    { head_repository: { full_name: 'someone/fork' } }]) {
    assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent(overrides),
      tagsAt: () => assert.fail('must not inspect tags for unrelated events') })), []);
  }
});

test('published versions are skipped after duplicate events, drafts remain retryable', () => {
  for (const eventName of ['push', 'workflow_run']) {
    const options = discovery({ eventName, event: eventName === 'push' ? { ref: `refs/tags/${tag}` } : completedEvent() });
    assert.deepEqual(discoverCandidates({ ...options, listReleases: () => [{ tag_name: tag, draft: false }] }), []);
    assert.deepEqual(discoverCandidates({ ...options, listReleases: () => [{ tag_name: tag, draft: true }] }), [{ tag, sha }]);
  }
  // Second queued job checks this again after the first job publishes.
  assert.equal(releaseState({ tag, sha, runs: [run({ head_sha: sha })], releases: [{ tag_name: tag, draft: false }] }), 'published');
});

test('only stable tags are selected, and deleted tag pushes are ignored', () => {
  assert.deepEqual(discoverCandidates(discovery({ event: { ref: `refs/tags/${tag}`, deleted: true } })), []);
  assert.throws(() => discoverCandidates(discovery({ event: { ref: 'refs/tags/v1.0.22-beta.1' } })), /stable version tag/);
  assert.deepEqual(discoverCandidates(discovery({ eventName: 'workflow_run', event: completedEvent(),
    tagsAt: () => ['nightly', 'v1.0.22-beta.1', tag] })), [{ tag, sha }]);
});

test('ancestry and API failures stop discovery instead of releasing without evidence', () => {
  for (const dependency of ['assertMainAncestor', 'listRuns', 'listReleases']) {
    assert.throws(() => discoverCandidates(discovery({ [dependency]: () => { throw new Error('blocked'); } })), /blocked/);
  }
});

const releaseOptions = { tag: 'v1.0.22', repo: 'owner/tp', paths: ['setup.exe', 'setup.msi'], notesPath: 'notes with spaces.md' };

test('published releases cannot be edited or have installers overwritten', () => {
  assert.throws(() => publishRelease({ ...releaseOptions, releases: [{ tag_name: 'v1.0.22', draft: false }],
    execute: () => assert.fail('must not mutate published release') }), /already published/);
});

test('first release uploads prepared notes and both installers before publishing the draft', () => {
  const calls = [];
  publishRelease({ ...releaseOptions, releases: [], execute: (program, args) => calls.push({ program, args }) });
  assert.equal(calls.length, 2);
  assert.equal(calls[0].program, 'gh');
  const args = calls[0].args;
  assert.deepEqual(args.slice(0, 5), ['release', 'create', 'v1.0.22', 'setup.exe', 'setup.msi']);
  assert.ok(args.includes('--draft'));
  assert.ok(args.includes('--verify-tag'));
  assert.equal(args[args.indexOf('--notes-file') + 1], releaseOptions.notesPath);
  assert.ok(!args.includes('--generate-notes'));
  assert.deepEqual(calls[1], { program: 'gh', args: ['release', 'edit', 'v1.0.22', '--repo', 'owner/tp', '--draft=false'] });
});

test('retry updates the existing draft and uploads installers before publishing', () => {
  const calls = [];
  publishRelease({ ...releaseOptions, releases: [{ tag_name: 'v1.0.22', draft: true }],
    execute: (_program, args) => calls.push(args) });
  assert.equal(calls.length, 3);
  assert.equal(calls[0][1], 'edit');
  assert.ok(calls[0].includes('--draft'));
  assert.equal(calls[1][1], 'upload');
  assert.ok(calls[1].includes('--clobber'));
  assert.deepEqual(calls[2], ['release', 'edit', 'v1.0.22', '--repo', 'owner/tp', '--draft=false']);
});

test('draft update errors stop the installer upload and publishing', () => {
  let count = 0;
  assert.throws(() => publishRelease({ ...releaseOptions, releases: [{ tag_name: 'v1.0.22', draft: true }],
    execute: () => { count++; throw new Error('Permission denied'); } }), /Permission denied/);
  assert.equal(count, 1);
});

test('failed draft creation does not publish the release', () => {
  const calls = [];
  assert.throws(() => publishRelease({ ...releaseOptions, releases: [],
    execute: (_program, args) => { calls.push(args); throw new Error('Creation failed'); } }), /Creation failed/);
  assert.equal(calls.length, 1);
  assert.equal(calls[0][1], 'create');
  assert.ok(!calls.some(args => args.includes('--draft=false')));
});

test('failed installer upload leaves the existing draft unpublished', () => {
  const calls = [];
  assert.throws(() => publishRelease({ ...releaseOptions, releases: [{ tag_name: 'v1.0.22', draft: true }],
    execute: (_program, args) => {
      calls.push(args);
      if (args[1] === 'upload') throw new Error('Upload failed');
    } }), /Upload failed/);
  assert.equal(calls.length, 2);
  assert.ok(!calls.some(args => args.includes('--draft=false')));
});

test('final publishing errors propagate after installers are ready for new and existing drafts', () => {
  for (const releases of [[], [{ tag_name: 'v1.0.22', draft: true }]]) {
    const calls = [];
    assert.throws(() => publishRelease({ ...releaseOptions, releases,
      execute: (_program, args) => {
        calls.push(args);
        if (args.includes('--draft=false')) throw new Error('Publishing denied');
      } }), /Publishing denied/);
    assert.equal(calls.length, releases.length ? 3 : 2);
    assert.deepEqual(calls.at(-1), ['release', 'edit', 'v1.0.22', '--repo', 'owner/tp', '--draft=false']);
  }
});
