import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as sleep } from 'node:timers/promises';

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

export async function waitForCi({ listRuns, sha, timeoutMs = 60 * 60 * 1000,
  intervalMs = 30 * 1000, now = Date.now, pause = sleep, log = console.log }) {
  const deadline = now() + timeoutMs;
  let previous;
  while (now() < deadline) {
    const run = selectCiRun(await listRuns(), sha);
    const state = run ? `${run.id}/${run.run_attempt}: ${run.status} (${run.conclusion ?? 'pending'})` : 'No matching main push CI yet';
    if (state !== previous) { log(state); previous = state; }
    if (run?.status === 'completed') {
      if (run.conclusion !== 'success') throw new Error(`Main CI did not succeed: ${run.html_url} (${run.conclusion})`);
      log(`Main CI passed: ${run.html_url}`);
      return run;
    }
    await pause(Math.min(intervalMs, Math.max(0, deadline - now())));
  }
  throw new Error(`Timed out waiting for main push CI for ${sha}. Check Actions and rerun this release workflow.`);
}

function command(program, args) {
  return execFileSync(program, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

function ghJson(args) { return JSON.parse(command('gh', args)); }

function context() {
  const tag = process.env.GITHUB_REF_NAME;
  const version = versionFromTag(tag);
  const repo = process.env.GITHUB_REPOSITORY;
  if (!repo) throw new Error('GITHUB_REPOSITORY is required');
  const sha = command('git', ['rev-parse', 'HEAD']);
  // Peel annotated tags as well as lightweight tags; never package another commit.
  if (command('git', ['rev-parse', `refs/tags/${tag}^{commit}`]) !== sha) {
    throw new Error('Checked-out commit does not match the version tag');
  }
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

export function publishDraft({ tag, repo, paths, notesPath, releases, execute = command }) {
  const existing = releases.find(release => release.tag_name === tag);
  if (existing && !existing.draft) throw new Error(`Release ${tag} is already published; refusing to modify it`);
  if (existing) {
    execute('gh', ['release', 'edit', tag, '--repo', repo, '--draft', '--title', `TP ${tag}`, '--notes-file', notesPath]);
    execute('gh', ['release', 'upload', tag, ...paths, '--repo', repo, '--clobber']);
  } else {
    execute('gh', ['release', 'create', tag, ...paths, '--repo', repo, '--verify-tag', '--draft',
      '--title', `TP ${tag}`, '--notes-file', notesPath]);
  }
}

async function main(mode) {
  const { tag, version, repo, sha } = context();
  if (mode === 'prepare') {
    command('git', ['merge-base', '--is-ancestor', sha, 'origin/main']);
    const json = path => JSON.parse(readFileSync(path, 'utf8'));
    validateVersions(version, json('package.json'), json('package-lock.json'),
      json('src-tauri/tauri.conf.json'), readFileSync('src-tauri/Cargo.toml', 'utf8'),
      readFileSync('src-tauri/Cargo.lock', 'utf8'));
    writeFileSync(releaseNotesPath(), extractNotes(readFileSync('CHANGELOG.md', 'utf8'), version));
    console.log(`Prepared ${tag} at ${sha}; release notes extracted from CHANGELOG.md`);
  } else if (mode === 'wait') {
    const query = new URLSearchParams({ branch: 'main', event: 'push', head_sha: sha, per_page: '100' });
    await waitForCi({ sha, listRuns: async () => ghJson(['api',
      `repos/${repo}/actions/workflows/ci.yml/runs?${query}`]).workflow_runs });
  } else if (mode === 'publish') {
    const paths = ['nsis', 'msi'].map(kind => {
      const dir = join('src-tauri', 'target', 'release', 'bundle', kind);
      const extension = kind === 'nsis' ? '.exe' : '.msi';
      const files = readdirSync(dir).filter(file => file.endsWith(extension));
      if (files.length !== 1) throw new Error(`Expected one ${extension} installer in ${dir}`);
      return join(dir, files[0]);
    });
    // Listing releases lets authentication/network errors fail instead of being
    // mistaken for a missing release. Pagination also finds older drafts.
    const releases = ghJson(['api', '--paginate', '--slurp', `repos/${repo}/releases?per_page=100`]).flat();
    publishDraft({ tag, repo, paths, notesPath: releaseNotesPath(), releases });
    console.log(`Release draft ready: ${tag}`);
  } else throw new Error(`Unknown release operation: ${mode}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv[2]).catch(error => {
    console.error(error.message);
    if (error.stderr) console.error(error.stderr.toString());
    process.exitCode = 1;
  });
}
