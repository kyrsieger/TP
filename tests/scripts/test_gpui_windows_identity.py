import _support
import json
from pathlib import Path
import tempfile
import unittest
from gpui_windows_identity import ROOT, check, native_identity, allow_downgrades

class InstallerIdentityTests(unittest.TestCase):
    def fixture(self, root):
        for relative in ('Cargo.toml', 'packaging/windows/identity.json', 'packaging/windows/policy.json', 'packaging/windows/compatibility.json'):
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / relative).read_bytes())

    def test_historical_upgrade_identity(self):
        self.assertEqual(check()['msiUpgradeCode'], '005599cc-eef4-57b7-9ab6-5f56e0b6e57d')
        self.assertEqual(check()['publisher'], 'TP')

    def test_self_contained(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            self.assertEqual(check(root), native_identity(root))

    def test_identity_changes_rejected(self):
        for key in ('productName', 'executable', 'publisher', 'msiUpgradeCode', 'msiInstallScope'):
            with self.subTest(key=key), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root)
                path = root / 'packaging/windows/identity.json'
                value = json.loads(path.read_text(encoding='utf-8'))
                value[key] = 'changed'
                path.write_text(json.dumps(value), encoding='utf-8')
                with self.assertRaisesRegex(ValueError, 'identity changed'):
                    check(root)

    def test_downgrade_changes_rejected(self):
        for value, message in [(False, 'policies differ'), ('false', 'explicit boolean')]:
            with self.subTest(value=value), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root)
                (root / 'packaging/windows/policy.json').write_text(json.dumps({'allowDowngrades': value}), encoding='utf-8')
                with self.assertRaisesRegex(ValueError, message):
                    check(root)
