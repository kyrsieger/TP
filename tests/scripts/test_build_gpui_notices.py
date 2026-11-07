import _support
import hashlib
import io
import json
import unittest
import zipfile
from pathlib import Path
import tempfile

from build_gpui_notices import render, installer_texts


class NoticeTests(unittest.TestCase):
    def test_installer_attribution_preserved_and_changed_text_rejected(self):
        texts = installer_texts()
        self.assertEqual(len(texts), 5)
        self.assertTrue(any(b"SPECIAL EXCEPTION FOR LZMA" in content for _, content in texts))
        self.assertTrue(any("nsis-tauri-utils 0.5.3" in label for label, _ in texts))
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "licenses/installers").mkdir(parents=True)
            content = b"Original attribution"
            (root / "notice").write_bytes(content)
            entry = {"name": "example", "source": "https://example.org/LICENSE", "file": "notice", "sha256": hashlib.sha256(content).hexdigest()}
            (root / "licenses/installers/manifest.json").write_text(json.dumps([entry]), encoding="utf-8")
            self.assertEqual(installer_texts(root)[0][1], content)
            (root / "notice").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                installer_texts(root)

    def setUp(self):
        self.lock = b"locked dependencies"
        self.terms = b"Copyright Original Author\nFull license terms\n"
        self.audit = {"cargo_lock_sha256": hashlib.sha256(self.lock).hexdigest(), "packages": [{
            "name": "example", "version": "1.0.0", "license": "MIT",
            "targets": ["windows", "macos"], "source": "registry+example",
            "repository": "https://example.org/source", "missing_declared_files": [],
            "text_files": [{"path": "LICENSE", "sha256": hashlib.sha256(self.terms).hexdigest()}],
        }]}

    def generate(self, content=None, lock=None):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("audit.json", json.dumps(self.audit))
            archive.writestr("licenses/example-1.0.0/LICENSE", self.terms if content is None else content)
        data.seek(0)
        with zipfile.ZipFile(data) as archive:
            return render(archive, self.lock if lock is None else lock, [("Additional attribution", self.terms)])

    def test_full_attribution_and_versioned_source_preserved(self):
        result = self.generate()
        self.assertIn("cargo: example 1.0.0", result)
        self.assertIn("https://crates.io/api/v1/crates/example/1.0.0/download", result)
        self.assertEqual(result.count("Copyright Original Author"), 1)
        self.assertEqual(result.count("see FULL TEXT"), 2)

    def test_stale_lock_rejected(self):
        with self.assertRaisesRegex(ValueError, "Cargo.lock"):
            self.generate(lock=b"new dependencies")

    def test_modified_text_rejected(self):
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            self.generate(content=b"omitted attribution")

    def test_missing_evidence_rejected(self):
        self.audit["packages"][0]["text_files"] = []
        with self.assertRaisesRegex(ValueError, "Missing license evidence"):
            self.generate()

    def test_missing_declared_file_rejected(self):
        self.audit["packages"][0]["missing_declared_files"] = ["COPYING"]
        with self.assertRaisesRegex(ValueError, "Missing license evidence"):
            self.generate()

    def test_duplicate_package_rejected(self):
        self.audit["packages"].append(self.audit["packages"][0].copy())
        with self.assertRaisesRegex(ValueError, "Ambiguous package"):
            self.generate()


if __name__ == "__main__":
    unittest.main()
