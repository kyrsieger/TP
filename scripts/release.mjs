import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function versionFromTag(tag) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag)) {
    throw new Error(`Expected a stable version tag such as v1.0.22; got ${tag}`);
  }
  return tag.slice(1);
}

export function extractNotes(changelog, version) {
  // Only level-two headings outside fenced code blocks delimit releases.
  const sections = [];
  const lines = changelog.replace(/^\uFEFF/, '').split(/\r?\n/);
  let fence;
  for (let i = 0; i < lines.length; i++) {
    const marker = lines[i].match(/^\s{0,3}(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = undefined;
      continue;
    }
    if (!fence && /^##\s+/.test(lines[i])) sections.push(i);
  }
  const matches = sections.filter(i => {
    const heading = lines[i].replace(/^##\s+/, '');
    const found = heading.match(/^(?:\[v?(\d+\.\d+\.\d+)\]|v?(\d+\.\d+\.\d+))(?=\s|$)/);
    return (found?.[1] ?? found?.[2]) === version;
  });
  if (matches.length !== 1) throw new Error(`CHANGELOG.md must contain exactly one section for ${version}`);
  const start = matches[0];
  const end = sections.find(i => i > start) ?? lines.length;
  const notes = lines.slice(start + 1, end).join('\n').trim();
  if (!notes || !notes.split('\n').some(line => line.trim() && !/^\s*#/.test(line))) {
    throw new Error(`CHANGELOG.md section ${version} has no release notes`);
  }
  return `${notes}\n`;
}

export function selectCiRun(runs, sha) {
  return runs.filter(run => run.head_sha === sha && run.head_branch === 'main' && run.event === 'push')
    .sort((a, b) => b.id - a.id)[0];
}

export function releaseState({ tag, sha, runs, releases }) {
  if (releases.some(release => release.tag_name === tag && !release.draft)) return 'published';
  const run = selectCiRun(runs, sha);
  return run?.status === 'completed' && run.conclusion === 'success' ? 'ready' : 'ci-not-successful';
}

// Both events converge here. A tag arriving before CI exits immediately; the
// later CI completion event will discover it, without keeping a runner alive.
export function discoverCandidates({ eventName, event, repo, resolveTag, tagsAt,
  assertMainAncestor, listRuns, listReleases }) {
  let sha, tags;
  if (eventName === 'push') {
    if (event.deleted || !event.ref?.startsWith('refs/tags/')) return [];
    const tag = event.ref.slice('refs/tags/'.length);
    versionFromTag(tag);
    sha = resolveTag(tag);
    tags = [tag];
  } else if (eventName === 'workflow_run') {
    const run = event.workflow_run;
    if (run?.event !== 'push' || run.head_branch !== 'main' || run.status !== 'completed' ||
      run.conclusion !== 'success' || run.head_repository?.full_name !== repo) return [];
    sha = run.head_sha;
    if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error('Invalid CI commit');
    tags = tagsAt(sha).filter(tag => {
      try { versionFromTag(tag); return true; } catch { return false; }
    });
  } else throw new Error(`Unsupported release event: ${eventName}`);
  if (!tags.length) return [];
  assertMainAncestor(sha);
  const runs = listRuns(sha);
  const releases = listReleases();
  return tags.filter(tag => releaseState({ tag, sha, runs, releases }) === 'ready')
    .map(tag => ({ tag, sha }));
}

function command(program, args) {
  return execFileSync(program, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

function ghJson(args) { return JSON.parse(command('gh', args)); }

function listCiRuns(repo, sha) {
  const query = new URLSearchParams({ branch: 'main', event: 'push', head_sha: sha, per_page: '100' });
  return ghJson(['api', '--paginate', '--slurp',
    `repos/${repo}/actions/workflows/ci.yml/runs?${query}`]).flatMap(page => page.workflow_runs);
}

function listReleases(repo) {
  // API failures must fail closed, never masquerade as an unpublished version.
  return ghJson(['api', '--paginate', '--slurp', `repos/${repo}/releases?per_page=100`]).flat();
}

function output(name, value) {
  if (!process.env.GITHUB_OUTPUT) throw new Error('GITHUB_OUTPUT is required');
  appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

function discover() {
  const repo = process.env.GITHUB_REPOSITORY;
  if (!repo) throw new Error('GITHUB_REPOSITORY is required');
  const candidates = discoverCandidates({
    eventName: process.env.GITHUB_EVENT_NAME,
    event: JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')), repo,
    resolveTag: tag => command('git', ['rev-parse', `refs/tags/${tag}^{commit}`]),
    tagsAt: sha => command('git', ['tag', '--points-at', sha]).split(/\r?\n/).filter(Boolean),
    assertMainAncestor: sha => command('git', ['merge-base', '--is-ancestor', sha, 'origin/main']),
    listRuns: sha => listCiRuns(repo, sha), listReleases: () => listReleases(repo),
  });
  output('candidates', JSON.stringify(candidates));
  output('ready', candidates.length > 0);
  console.log(candidates.length ? `Ready: ${candidates.map(item => item.tag).join(', ')}` :
    'No unpublished version with successful main CI; exiting without waiting.');
}

export function validateCheckout(sha, expectedSha, tagSha) {
  if (sha !== expectedSha) throw new Error('Checked-out commit does not match the selected release commit');
  if (tagSha !== sha) throw new Error('Checked-out commit does not match the version tag');
}

function context() {
  const tag = process.env.RELEASE_TAG;
  const version = versionFromTag(tag);
  const repo = process.env.GITHUB_REPOSITORY;
  if (!repo) throw new Error('GITHUB_REPOSITORY is required');
  const sha = command('git', ['rev-parse', 'HEAD']);
  // Peel annotated tags as well as lightweight tags; never package another commit.
  validateCheckout(sha, process.env.RELEASE_SHA, command('git', ['rev-parse', `refs/tags/${tag}^{commit}`]));
  return { tag, version, repo, sha };
}

function releaseNotesPath() {
  if (!process.env.RUNNER_TEMP) throw new Error('RUNNER_TEMP is required');
  return join(process.env.RUNNER_TEMP, 'tp-release-notes.md');
}

export function validateVersions(version, packageJson, lock, tauri, cargo, cargoLock) {
  const cargoPackage = cargo.match(/^\[package\]\s*\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
  const versions = [packageJson.version, lock.version, lock.packages?.['']?.version,
    tauri.version, cargoPackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    cargoLock.split(/^\[\[package\]\]\s*$/m).find(block => /^name\s*=\s*"tp"\s*$/m.test(block))
      ?.match(/^version\s*=\s*"([^"]+)"/m)?.[1]];
  if (versions.some(value => value !== version)) {
    throw new Error(`Tag version ${version} must match package.json, package-lock.json, tauri.conf.json, Cargo.toml and Cargo.lock (${versions.join(', ')})`);
  }
}

export function publishRelease({ tag, repo, paths, notesPath, releases, execute = command }) {
  const existing = releases.find(release => release.tag_name === tag);
  if (existing && !existing.draft) throw new Error(`Release ${tag} is already published; refusing to modify it`);
  if (existing) {
    execute('gh', ['release', 'edit', tag, '--repo', repo, '--draft', '--title', `TP ${tag}`, '--notes-file', notesPath]);
    execute('gh', ['release', 'upload', tag, ...paths, '--repo', repo, '--clobber']);
  } else {
    execute('gh', ['release', 'create', tag, ...paths, '--repo', repo, '--verify-tag', '--draft',
      '--title', `TP ${tag}`, '--notes-file', notesPath]);
  }
  // Keep the release in draft until both installers have uploaded successfully.
  execute('gh', ['release', 'edit', tag, '--repo', repo, '--draft=false']);
}

async function main(mode) {
  if (mode === 'discover') return discover();
  const { tag, version, repo, sha } = context();
  if (mode === 'ready') {
    const state = releaseState({ tag, sha, runs: listCiRuns(repo, sha), releases: listReleases(repo) });
    if (state === 'ci-not-successful') throw new Error(`Main push CI has not succeeded for ${sha}; not waiting`);
    output('ready', state === 'ready');
    console.log(state === 'published' ? `${tag} is already published; skipping build` : `Ready to build ${tag}`);
  } else if (mode === 'prepare') {
    command('git', ['merge-base', '--is-ancestor', sha, 'origin/main']);
    const json = path => JSON.parse(readFileSync(path, 'utf8'));
    validateVersions(version, json('package.json'), json('package-lock.json'),
      json('src-tauri/tauri.conf.json'), readFileSync('src-tauri/Cargo.toml', 'utf8'),
      readFileSync('src-tauri/Cargo.lock', 'utf8'));
    writeFileSync(releaseNotesPath(), extractNotes(readFileSync('CHANGELOG.md', 'utf8'), version));
    console.log(`Prepared ${tag} at ${sha}; release notes extracted from CHANGELOG.md`);
  } else if (mode === 'publish') {
    if (releaseState({ tag, sha, runs: listCiRuns(repo, sha), releases: [] }) !== 'ready') {
      throw new Error(`Main push CI no longer successful for ${sha}; refusing to publish`);
    }
    const paths = ['nsis', 'msi'].map(kind => {
      const dir = join('src-tauri', 'target', 'release', 'bundle', kind);
      const extension = kind === 'nsis' ? '.exe' : '.msi';
      const files = readdirSync(dir).filter(file => file.endsWith(extension));
      if (files.length !== 1) throw new Error(`Expected one ${extension} installer in ${dir}`);
      return join(dir, files[0]);
    });
    // Listing releases lets authentication/network errors fail instead of being
    // mistaken for a missing release. Pagination also finds older drafts.
    const releases = listReleases(repo);
    publishRelease({ tag, repo, paths, notesPath: releaseNotesPath(), releases });
    console.log(`Release published: ${tag}`);
  } else throw new Error(`Unknown release operation: ${mode}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv[2]).catch(error => {
    console.error(error.message);
    if (error.stderr) console.error(error.stderr.toString());
    process.exitCode = 1;
  });
}
