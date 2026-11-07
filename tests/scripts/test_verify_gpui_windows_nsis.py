import _support
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from verify_gpui_windows_nsis import (
    INSTALLER_FILES, SUPPORT_SOURCES, archive_paths, validate_archive_paths, validate_extracted,
)


class NsisVerificationTests(unittest.TestCase):
    def listing(self, names, kind='Nsis'):
        return '\nType = '+kind+'\n\n----------\n'+''.join('Path = '+name+'\nSize = 3\n\n' for name in names)

    def test_listing_and_exact_file_set(self):
        names = INSTALLER_FILES | {'tp.exe', 'license', 'third-party-notices.txt'}
        paths = archive_paths(self.listing(names))
        validate_archive_paths(paths, ['TP.exe', 'LICENSE', 'third-party-notices.txt'])
        for wrong in (paths | {'extra.dll'}, paths - {'tp.exe'}):
            with self.assertRaisesRegex(ValueError, 'file set differs'):
                validate_archive_paths(wrong, ['TP.exe', 'LICENSE', 'third-party-notices.txt'])

    def test_unsafe_and_duplicate_archive_entries_are_rejected(self):
        for name in ('../TP.exe', 'C:\\TP.exe', '\\\\server\\share\\TP.exe',
                     'TP.exe:stream', '/TP.exe', '$PLUGINSDIR\\..\\TP.exe'):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Unsafe'):
                archive_paths(self.listing([name]))
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            archive_paths(self.listing(['TP.exe', 'tp.exe']))
        with self.assertRaisesRegex(ValueError, 'NSIS archive'):
            archive_paths(self.listing(['TP.exe'], kind='zip'))

    def test_extracted_payload_hash_and_uninstaller(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in INSTALLER_FILES:
                path = root/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'MZfixture')
            payload = b'expected executable fixture'
            (root/'TP.exe').write_bytes(payload)
            manifest = {'files': [{'name': 'TP.exe', 'bytes': len(payload),
                                   'sha256': hashlib.sha256(payload).hexdigest()}]}
            # Fixture hashes replace only the expected pin, not the verifier.
            with patch('package_gpui_windows_nsis.PLUGIN_SHA1', hashlib.sha1(b'MZfixture').hexdigest()), patch(
                    'package_gpui_windows_nsis.PLUGIN_SHA256', hashlib.sha256(b'MZfixture').hexdigest()), patch(
                    'verify_gpui_windows_nsis.support_hashes', return_value={
                        name: hashlib.sha256(b'MZfixture').hexdigest() for name in SUPPORT_SOURCES}):
                validate_extracted(root, manifest)
                for name in SUPPORT_SOURCES:
                    with self.subTest(support=name):
                        (root/name).write_bytes(b'replaced support file')
                        with self.assertRaisesRegex(ValueError, 'support file hash mismatch'):
                            validate_extracted(root, manifest)
                        (root/name).write_bytes(b'MZfixture')
                plugin = root/'$pluginsdir/nsis_tauri_utils.dll'
                plugin.write_bytes(b'MZreplacement')
                with self.assertRaisesRegex(ValueError, 'pinned v0.5.3'):
                    validate_extracted(root, manifest)
                plugin.write_bytes(b'MZfixture')
            # A plausible executable with the right name is not the pinned plugin.
            with self.assertRaisesRegex(ValueError, 'pinned v0.5.3'):
                validate_extracted(root, manifest)
            (root/'TP.exe').write_bytes(b'changed executable fixture')
            with self.assertRaisesRegex(ValueError, 'payload hash mismatch'):
                validate_extracted(root, manifest)
            (root/'TP.exe').write_bytes(payload)
            (root/'uninstall.exe').write_bytes(b'not a PE')
            with self.assertRaisesRegex(ValueError, 'Uninstaller'):
                validate_extracted(root, manifest)


if __name__ == '__main__':
    unittest.main()
