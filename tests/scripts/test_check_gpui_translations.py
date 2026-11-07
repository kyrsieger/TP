import _support
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("translations", _support.SCRIPTS / "check-gpui-translations.py")
translations = importlib.util.module_from_spec(spec)
spec.loader.exec_module(translations)


class TranslationChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for path in ("crates/tp-gpui/resources", "scripts", "crates/tp-gpui/src"):
            (self.root / path).mkdir(parents=True)
        (self.root / "crates/tp-gpui/resources/translations.json").write_text('{"取消":"Cancel"}', encoding="utf-8")
        self.native = self.root / "scripts/gpui-translations.json"
        self.native.write_text(json.dumps({"开启": "On"}), encoding="utf-8")
        self.source = self.root / "crates/tp-gpui/src/view.rs"

    def check(self, extra):
        self.source.write_text('choose("取消", "Cancel");\n' + extra, encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            return translations.check(self.root)

    def test_original_and_reviewed_native_messages_pass(self):
        self.assertEqual(self.check('choose("开启", "On");'), 0)

    def test_new_native_message_requires_review(self):
        self.assertEqual(self.check('choose("新增状态", "New status");'), 1)

    def test_changed_native_translation_is_rejected(self):
        self.assertEqual(self.check('choose("开启", "Enabled");'), 1)

    def test_native_catalog_cannot_override_original(self):
        self.native.write_text(json.dumps({"取消": "Dismiss"}), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "must not override"):
            self.check("")


if __name__ == "__main__":
    unittest.main()
