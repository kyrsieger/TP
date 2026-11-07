import _support
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from gpui_windows_identity import check
from package_gpui_windows_nsis import author, escape, render, verify_upstream, verify_compiler_files, verify_tools


class NsisAuthoringTests(unittest.TestCase):
    def test_incomplete_or_replaced_compiler_distribution_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            compiler = Path(temp)/'makensis.exe'
            with self.assertRaisesRegex(ValueError, 'distribution file missing or changed'):
                verify_compiler_files(compiler)
            compiler.write_bytes(b'MZreplacement')
            with self.assertRaisesRegex(ValueError, 'distribution file missing or changed'):
                verify_compiler_files(compiler)

    def test_missing_or_replaced_plugin_rejected_before_compiler_runs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with patch('package_gpui_windows_nsis.subprocess.run') as run:
                with self.assertRaisesRegex(ValueError, 'missing'):
                    verify_tools(root/'makensis.exe', root)
                (root/'nsis_tauri_utils.dll').write_bytes(b'replacement')
                with self.assertRaisesRegex(ValueError, 'pinned v0.5.3'):
                    verify_tools(root/'makensis.exe', root)
                run.assert_not_called()

    def test_wrong_compiler_version_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            compiler = root/'makensis.exe'; compiler.touch()
            with patch('package_gpui_windows_nsis.verify_plugin'), patch(
                    'package_gpui_windows_nsis.verify_compiler_files'), patch(
                    'package_gpui_windows_nsis.subprocess.run') as run:
                run.return_value.stdout = 'v3.10'
                with self.assertRaisesRegex(ValueError, 'Expected NSIS v3.11'):
                    verify_tools(compiler, root)
                run.return_value.stdout = 'v3.11'
                verify_tools(compiler, root)

    def test_pinned_template_integrity(self):
        verify_upstream()

    def test_values_are_nsis_escaped(self):
        self.assertEqual(escape('a$"\n'), 'a$$$\\"$\\n')
        self.assertEqual(render('{{#each resources}}{{this.[1]}}={{no-escape @key}}\n{{/each}}',
                                {'resources': {'C:\\$test.txt': [None, 'test.txt']}}),
                         'test.txt=C:\\$$test.txt\n')
        with self.assertRaises(KeyError):
            render('{{unknown}}', {})
        with self.assertRaises(ValueError):
            render('{{#if enabled}}oops', {'enabled': True})

    def test_legacy_upgrade_and_user_choices_survive_without_webview_section(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            stage = root/'stage'; stage.mkdir()
            output = root/'output'; output.mkdir()
            files = ['TP.exe', 'LICENSE', 'third-party-notices.txt', 'vcruntime140.dll']
            for name in files:
                (stage/name).write_bytes(b'authoring-only fixture')
            author(stage, output, check(), files, root/'plugins')
            self.assertTrue((output/'SimpChinese.nsh').read_bytes().startswith(b'\xef\xbb\xbf'))
            script = (output/'installer.nsi').read_text(encoding='utf-8')
            self.assertNotIn('{{', script)
            self.assertNotIn('Section WebView2', script)
            self.assertIn('Function PageReinstall', script)
            self.assertIn('RequestExecutionLevel user', script)
            self.assertIn('MUI_PAGE_STARTMENU', script)
            self.assertIn('DeleteAppDataCheckbox', script)
            self.assertIn('!define INSTALLMODE "currentUser"', script)
            self.assertIn('!insertmacro MUI_LANGUAGE "SimpChinese"', script)
            self.assertIn('Delete "$INSTDIR\\\\vcruntime140.dll"', script)
            self.assertNotIn('RMDir /r "$INSTDIR"', script)
            self.assertIn('WriteUninstaller "$INSTDIR\\uninstall.exe"', script)


if __name__ == '__main__':
    unittest.main()
