"""Native release orchestration (Python 3.11+, git and gh; no third-party modules)."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


def command(program, args):
    return subprocess.run([program, *args], check=True, stdout=subprocess.PIPE,
                          encoding="utf-8").stdout.strip()


def version_from_tag(tag):
    if not re.fullmatch(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", tag or ""):
        raise ValueError(f"Expected a stable version tag; got {tag}")
    return tag[1:]


def extract_notes(changelog, version):
    lines = changelog.lstrip("\ufeff").splitlines()
    sections, fence = [], None
    for i, line in enumerate(lines):
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            mark = marker[1]
            if fence is None:
                fence = mark
            elif mark[0] == fence[0] and len(mark) >= len(fence):
                fence = None
            continue
        if fence is None and re.match(r"^##\s+", line):
            sections.append(i)
    matches = []
    for i in sections:
        match = re.match(r"^##\s+(?:\[v?(\d+\.\d+\.\d+)\]|v?(\d+\.\d+\.\d+))(?=\s|$)", lines[i])
        if match and (match[1] or match[2]) == version:
            matches.append(i)
    if len(matches) != 1:
        raise ValueError(f"CHANGELOG.md must contain exactly one section for {version}")
    start = matches[0]
    end = next((i for i in sections if i > start), len(lines))
    notes = "\n".join(lines[start + 1:end]).strip()
    if not any(line.strip() and not line.lstrip().startswith('#') for line in notes.splitlines()):
        raise ValueError(f"CHANGELOG.md section {version} has no release notes")
    return notes + "\n"


def release_state(tag, sha, runs, releases):
    if any(r['tag_name'] == tag and not r['draft'] for r in releases):
        return 'published'
    matching = [r for r in runs if r['head_sha'] == sha and r['head_branch'] == 'main' and r['event'] == 'push']
    run = max(matching, key=lambda r: r['id'], default={})
    return 'ready' if run.get('status') == 'completed' and run.get('conclusion') == 'success' else 'ci-not-successful'


def list_runs(repo, sha):
    pages = json.loads(command('gh', ['api', '--paginate', '--slurp',
        f'repos/{repo}/actions/workflows/ci.yml/runs?branch=main&event=push&head_sha={sha}&per_page=100']))
    return [run for page in pages for run in page['workflow_runs']]


def list_releases(repo):
    pages = json.loads(command('gh', ['api', '--paginate', '--slurp', f'repos/{repo}/releases?per_page=100']))
    return [release for page in pages for release in page]


def discover_candidates(event_name, event, repo, resolve_tag, tags_at, assert_main, runs_for, releases_for):
    if event_name == 'push':
        if event.get('deleted') or not event.get('ref', '').startswith('refs/tags/'):
            return []
        tag = event['ref'][len('refs/tags/'):]
        version_from_tag(tag)
        sha, tags = resolve_tag(tag), [tag]
    elif event_name == 'workflow_run':
        run = event.get('workflow_run', {})
        if (run.get('event') != 'push' or run.get('head_branch') != 'main' or
            run.get('status') != 'completed' or run.get('conclusion') != 'success' or
            run.get('head_repository', {}).get('full_name') != repo):
            return []
        sha = run['head_sha']
        tags = []
        if not re.fullmatch('[a-f0-9]{40}', sha):
            raise ValueError('Invalid CI commit')
        for tag in tags_at(sha):
            try:
                version_from_tag(tag)
                tags.append(tag)
            except ValueError:
                pass
    else:
        raise ValueError(f'Unsupported release event: {event_name}')
    if not re.fullmatch('[a-f0-9]{40}', sha):
        raise ValueError('Invalid release commit')
    if not tags:
        return []
    assert_main(sha)
    runs, releases = runs_for(sha), releases_for()
    return [dict(tag=tag, sha=sha) for tag in tags if release_state(tag, sha, runs, releases) == 'ready']


def validate_checkout(sha, expected_sha, tag_sha):
    if sha != expected_sha or tag_sha != sha:
        raise ValueError('Checked-out commit does not match selected commit and version tag')


def validate_native_versions(version, cargo, lock):
    versions = [tomllib.loads(cargo)['workspace']['package']['version']]
    packages = tomllib.loads(lock)['package']
    for name in ('tp-core', 'tp-desktop', 'tp-gpui'):
        entries = [p for p in packages if p['name'] == name]
        if len(entries) != 1 or 'source' in entries[0]:
            raise ValueError(f'Expected one local {name} package in Cargo.lock')
        versions.append(entries[0]['version'])
    if any(v != version for v in versions):
        raise ValueError(f'Tag version {version} must match native workspace and local packages: {versions}')


def installer_paths(version, root=Path('.')):
    paths = []
    for kind, extension, name in [('nsis', '.exe', f'TP_{version}_x64-setup.exe'),
                                  ('msi', '.msi', f'TP_{version}_x64_zh-CN.msi')]:
        directory = root / 'target/gpui-package' / kind
        files = sorted(p.name for p in directory.iterdir() if p.suffix == extension)
        if files != [name] or not (directory / name).is_file():
            raise ValueError(f'Expected exactly one installer {name} in {directory}')
        paths.append(str(directory / name))
    return paths


def publish_release(tag, repo, paths, notes_path, releases, execute=command):
    existing = next((r for r in releases if r['tag_name'] == tag), None)
    if existing and not existing['draft']:
        raise ValueError(f'Release {tag} is already published; refusing to modify it')
    metadata = ['--repo', repo, '--draft', '--title', f'TP {tag}', '--notes-file', str(notes_path)]
    if existing:
        execute('gh', ['release', 'edit', tag, *metadata])
        execute('gh', ['release', 'upload', tag, *paths, '--repo', repo, '--clobber'])
    else:
        execute('gh', ['release', 'create', tag, *paths, '--verify-tag', *metadata])
    execute('gh', ['release', 'edit', tag, '--repo', repo, '--draft=false'])


def output(name, value):
    with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as stream:
        stream.write(f'{name}={json.dumps(value, separators=(",", ":"))}\n')


def main(mode):
    repo = os.environ['GITHUB_REPOSITORY']
    if mode == 'discover':
        candidates = discover_candidates(os.environ['GITHUB_EVENT_NAME'],
            json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text(encoding='utf-8')), repo,
            lambda tag: command('git', ['rev-parse', f'refs/tags/{tag}^{{commit}}']),
            lambda sha: command('git', ['tag', '--points-at', sha]).splitlines(),
            lambda sha: command('git', ['merge-base', '--is-ancestor', sha, 'origin/main']),
            lambda sha: list_runs(repo, sha), lambda: list_releases(repo))
        output('candidates', candidates)
        output('ready', bool(candidates))
        return
    tag = os.environ['RELEASE_TAG']
    version = version_from_tag(tag)
    sha = command('git', ['rev-parse', 'HEAD'])
    validate_checkout(sha, os.environ['RELEASE_SHA'], command('git', ['rev-parse', f'refs/tags/{tag}^{{commit}}']))
    if mode == 'ready':
        state = release_state(tag, sha, list_runs(repo, sha), list_releases(repo))
        if state == 'ci-not-successful':
            raise ValueError('Main push CI has not succeeded; not waiting')
        output('ready', state == 'ready')
    elif mode == 'prepare':
        command('git', ['merge-base', '--is-ancestor', sha, 'origin/main'])
        if json.loads(Path('packaging/release.json').read_text(encoding='utf-8'))['backend'] != 'gpui':
            raise ValueError('Only native GPUI releases are supported')
        validate_native_versions(version, Path('Cargo.toml').read_text(encoding='utf-8'), Path('Cargo.lock').read_text(encoding='utf-8'))
        Path(os.environ['RUNNER_TEMP'], 'tp-release-notes.md').write_text(
            extract_notes(Path('CHANGELOG.md').read_text(encoding='utf-8'), version), encoding='utf-8')
    elif mode == 'publish':
        if release_state(tag, sha, list_runs(repo, sha), []) != 'ready':
            raise ValueError('Main push CI no longer successful; refusing to publish')
        publish_release(tag, repo, installer_paths(version), Path(os.environ['RUNNER_TEMP'], 'tp-release-notes.md'), list_releases(repo))
    else:
        raise ValueError(f'Unknown release operation: {mode}')


if __name__ == '__main__':
    main(sys.argv[1])
