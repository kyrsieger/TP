import _support
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock
import release
from build_native import build_command

ROOT = _support.ROOT
SHA = 'a' * 40
RUN = dict(id=1, head_sha=SHA, head_branch='main', event='push', status='completed', conclusion='success')

class ReleaseTests(unittest.TestCase):
    def test_stable_tags(self):
        self.assertEqual(release.version_from_tag('v1.2.3'), '1.2.3')
        for tag in ('v01.2.3', '1.2.3', 'v1.2.3-beta', 'v1.2.3\n', None):
            with self.assertRaises(ValueError): release.version_from_tag(tag)

    def test_notes(self):
        text = '# Changes\n## [v1.2.3] date\nFixed\n```md\n## 1.2.3\n```\n## 1.2.2\nOld'
        self.assertEqual(release.extract_notes(text, '1.2.3'), 'Fixed\n```md\n## 1.2.3\n```\n')
        for text in ('## 1.2.3\n### Empty', '## 1.2.3\nA\n## 1.2.3\nB', '## 1.2.30\nWrong'):
            with self.assertRaises(ValueError): release.extract_notes(text, '1.2.3')

    def test_latest_main_ci(self):
        self.assertEqual(release.release_state('v1.2.3', SHA, [RUN], []), 'ready')
        for change in ({'id': 2, 'status': 'in_progress'}, {'id': 2, 'conclusion': 'failure'}):
            self.assertEqual(release.release_state('v1.2.3', SHA, [RUN, {**RUN, **change}], []), 'ci-not-successful')
        self.assertEqual(release.release_state('v1.2.3', SHA, [{**RUN, 'event': 'pull_request'}], []), 'ci-not-successful')
        self.assertEqual(release.release_state('v1.2.3', SHA, [RUN], [{'tag_name': 'v1.2.3', 'draft': False}]), 'published')

    def discover(self, name, event, runs=None):
        return release.discover_candidates(name, event, 'owner/TP', lambda tag: SHA,
            lambda sha: ['v1.2.3', 'invalid'], lambda sha: None,
            lambda sha: [RUN] if runs is None else runs, lambda: [])

    def test_discovery_events(self):
        push = {'ref': 'refs/tags/v1.2.3'}
        self.assertEqual(self.discover('push', push, []), [])
        self.assertEqual(self.discover('push', push), [{'tag': 'v1.2.3', 'sha': SHA}])
        event = {'workflow_run': {**RUN, 'head_repository': {'full_name': 'owner/TP'}}}
        self.assertEqual(self.discover('workflow_run', event), [{'tag': 'v1.2.3', 'sha': SHA}])
        event['workflow_run']['head_repository']['full_name'] = 'other/TP'
        self.assertEqual(self.discover('workflow_run', event), [])
        self.assertEqual(self.discover('push', {**push, 'deleted': True}), [])

    def test_api_failure_blocks_discovery(self):
        fail = Mock(side_effect=RuntimeError('API unavailable'))
        with self.assertRaises(RuntimeError):
            release.discover_candidates('push', {'ref': 'refs/tags/v1.2.3'}, 'owner/TP',
                lambda tag: SHA, lambda sha: [], lambda sha: None, fail, lambda: [])

    def test_commit_and_versions(self):
        release.validate_checkout(SHA, SHA, SHA)
        for expected, tag in [('b'*40, SHA), (SHA, 'b'*40)]:
            with self.assertRaises(ValueError): release.validate_checkout(SHA, expected, tag)
        cargo = (ROOT / 'Cargo.toml').read_text(encoding='utf-8')
        lock = (ROOT / 'Cargo.lock').read_text(encoding='utf-8')
        import tomllib
        version = tomllib.loads(cargo)['workspace']['package']['version']
        release.validate_native_versions(version, cargo, lock)
        with self.assertRaises(ValueError): release.validate_native_versions('99.0.0', cargo, lock)
        with self.assertRaises(ValueError):
            release.validate_native_versions(version, cargo, lock + '\n[[package]]\nname="tp-core"\nversion="1.1.0"\n')

    def test_exact_installers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for kind, name in [('nsis', 'TP_1.2.3_x64-setup.exe'), ('msi', 'TP_1.2.3_x64_zh-CN.msi')]:
                path = root / 'target/gpui-package' / kind / name
                path.parent.mkdir(parents=True)
                path.write_bytes(b'test')
            self.assertEqual(len(release.installer_paths('1.2.3', root)), 2)
            with self.assertRaises(ValueError): release.installer_paths('1.2.4', root)
            (root / 'target/gpui-package/nsis/extra.exe').write_bytes(b'test')
            with self.assertRaises(ValueError): release.installer_paths('1.2.3', root)

    def test_publish_order_and_existing_release(self):
        for existing in ([], [{'tag_name': 'v1.2.3', 'draft': True}]):
            execute = Mock()
            release.publish_release('v1.2.3', 'owner/TP', ['a.exe', 'b.msi'], 'notes.md', existing, execute)
            self.assertIn('--draft=false', execute.call_args.args[1])
            self.assertIn('--draft', execute.call_args_list[0].args[1])
        execute = Mock()
        with self.assertRaises(ValueError):
            release.publish_release('v1.2.3', 'owner/TP', [], 'notes.md', [{'tag_name': 'v1.2.3', 'draft': False}], execute)
        execute.assert_not_called()

    def test_upload_failure_never_publishes(self):
        execute = Mock(side_effect=[None, RuntimeError('upload failed')])
        with self.assertRaises(RuntimeError):
            release.publish_release('v1.2.3', 'owner/TP', ['a', 'b'], 'notes.md', [{'tag_name': 'v1.2.3', 'draft': True}], execute)
        self.assertFalse(any('--draft=false' in call.args[1] for call in execute.call_args_list))

    def test_build_entry_points(self):
        self.assertEqual(build_command('win32')[0], 'powershell.exe')
        self.assertEqual(build_command('darwin'), ['cargo', 'build', '-p', 'tp-gpui', '--release', '--locked'])
        with self.assertRaises(ValueError): build_command('linux')
