import _support
import hashlib
import io
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

from prepare_gpui_wix import archive_files, prepare, verify_tools


class WixPreparationTests(unittest.TestCase):
    def fixture(self, names=("candle.exe",)):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            for name in names:
                archive.writestr(name, b"tool")
        data = buffer.getvalue()
        return data, {"sha256": hashlib.sha256(data).hexdigest(), "files": {
            name: hashlib.sha256(b"tool").hexdigest() for name in names}}

    def test_exact_archive_preserves_contents(self):
        data, contract = self.fixture()
        self.assertEqual(archive_files(data, contract), {"candle.exe": b"tool"})

    def test_damaged_archive_and_member_are_rejected(self):
        data, contract = self.fixture()
        with self.assertRaisesRegex(ValueError, "checksum"):
            archive_files(data + b"changed", contract)
        contract["files"]["candle.exe"] = "changed"
        with self.assertRaisesRegex(ValueError, "file changed"):
            archive_files(data, contract)

    def test_unsafe_paths_and_case_duplicates_rejected(self):
        for names in [("../escape",), ("/absolute",), ("C:relative",), ("candle.exe", "CANDLE.exe")]:
            data, contract = self.fixture(names)
            with self.assertRaisesRegex(ValueError, "archive member"):
                archive_files(data, contract)

    def test_existing_output_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch("prepare_gpui_wix.urllib.request.urlopen") as fetch:
                with self.assertRaises(FileExistsError):
                    prepare(Path(directory))
                fetch.assert_not_called()

    def test_cached_tool_modified_or_missing_is_rejected(self):
        _, contract = self.fixture()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "missing or changed"):
                verify_tools(root, contract)
            (root / "candle.exe").write_bytes(b"tool")
            verify_tools(root, contract)
            (root / "candle.exe").write_bytes(b"modified")
            with self.assertRaisesRegex(ValueError, "missing or changed"):
                verify_tools(root, contract)


if __name__ == "__main__":
    unittest.main()
