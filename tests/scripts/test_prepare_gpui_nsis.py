import _support
import hashlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from prepare_gpui_nsis import archive_files, prepare, verified_bytes


class ToolPreparationTests(unittest.TestCase):
    def archive(self, names):
        result = io.BytesIO()
        with zipfile.ZipFile(result, "w") as archive:
            for name in names:
                archive.writestr(name, b"compiler")
        return result.getvalue()

    def manifest(self):
        return {"version": "3.11", "files": {"makensis.exe": hashlib.sha256(b"compiler").hexdigest()}}

    def test_archive_root_removed_and_content_verified(self):
        self.assertEqual(archive_files(self.archive(["nsis-3.11/makensis.exe"]), self.manifest()), {"makensis.exe": b"compiler"})

    def test_wrong_root_traversal_and_duplicate_case_rejected(self):
        for names in [["../escape"], ["nsis-3.11/../escape"], ["nsis-3.11/C:escape"],
                      ["nsis-3.11/makensis.exe", "nsis-3.11/MAKENSIS.exe"]]:
            with self.assertRaisesRegex(ValueError, "archive member"):
                archive_files(self.archive(names), self.manifest())

    def test_missing_or_modified_required_file_rejected(self):
        with self.assertRaisesRegex(ValueError, "missing or changed"):
            archive_files(self.archive(["nsis-3.11/another.exe"]), self.manifest())
        manifest = self.manifest()
        manifest["files"]["makensis.exe"] = "wrong"
        with self.assertRaisesRegex(ValueError, "missing or changed"):
            archive_files(self.archive(["nsis-3.11/makensis.exe"]), manifest)

    def test_both_pinned_hashes_required(self):
        data = b"tool"
        sha256, sha1 = hashlib.sha256(data).hexdigest(), hashlib.sha1(data).hexdigest()
        self.assertEqual(verified_bytes(data, sha256, sha1.upper()), data)
        for digests in [("wrong", sha1), (sha256, "wrong")]:
            with self.assertRaisesRegex(ValueError, "checksum"):
                verified_bytes(data, *digests)

    def test_existing_destination_never_downloads_or_overwrites(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch("prepare_gpui_nsis.obtain") as fetch:
                with self.assertRaises(FileExistsError):
                    prepare(Path(directory))
                fetch.assert_not_called()

    def test_bad_download_leaves_no_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "tools"
            with patch("prepare_gpui_nsis.obtain", return_value=b"damaged archive"):
                with self.assertRaisesRegex(ValueError, "checksum"):
                    prepare(output)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
