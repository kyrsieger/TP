import _support
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("audit", _support.SCRIPTS / "audit-gpui-licenses.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class StandardTermsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name) / "terms"
        self.source = Path(self.temp.name) / "crate"
        self.folder.mkdir()
        self.source.mkdir()
        self.item = copy.deepcopy(json.loads((audit.ROOT / "licenses/gpui/standard-terms.json").read_text())["packages"][0])
        self.declaration = b"/* https://mozilla.org/MPL/2.0/ */\n"
        (self.source / "lib.rs").write_bytes(self.declaration)
        self.item["declaration"]["sha256"] = hashlib.sha256(self.declaration).hexdigest()
        (self.source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": self.item["revision"]}}))
        (self.folder / self.item["file"]).write_bytes((audit.ROOT / "licenses/gpui" / self.item["file"]).read_bytes())
        self.entries = {"crate": {k: self.item[k] for k in ("name", "version", "repository", "license")}}

    def run_audit(self):
        (self.folder / "standard-terms.json").write_text(json.dumps({"packages": [self.item]}))
        return audit.apply_standard_terms(self.entries, {"crate": self.source}, self.folder)

    def test_preserves_terms_and_source_declaration(self):
        artifacts = self.run_audit()
        self.assertEqual(artifacts["selectors-0.38.0/declaration-source.txt"], self.declaration)
        self.assertEqual(len(artifacts), 2)
        self.assertEqual(self.entries["crate"]["standard_terms"][0], self.item)

    def test_rejects_changed_dependency(self):
        for field in ("version", "repository", "license"):
            with self.subTest(field=field):
                original = self.entries["crate"][field]
                self.entries["crate"][field] = "changed"
                with self.assertRaises(ValueError):
                    self.run_audit()
                self.entries["crate"][field] = original

    def test_rejects_changed_revision(self):
        self.item["revision"] = "wrong"
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_rejects_changed_declaration(self):
        (self.source / "lib.rs").write_bytes(b"different source")
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_rejects_changed_terms(self):
        (self.folder / self.item["file"]).write_bytes(b"different license")
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_rejects_path_escape(self):
        self.item["declaration"]["path"] = "../elsewhere"
        with self.assertRaises(ValueError):
            self.run_audit()

    def test_rejects_unreviewed_url(self):
        self.item["url"] = "https://example.com/license"
        with self.assertRaises(ValueError):
            self.run_audit()

    def prepare_apache(self):
        records = json.loads((audit.ROOT / "licenses/gpui/standard-terms.json").read_text())["packages"]
        self.item = copy.deepcopy(next(item for item in records if item["license"] == "Apache-2.0"))
        for name in (self.item["file"], self.item["upstream_notice"]["file"]):
            (self.folder / name).write_bytes((audit.ROOT / "licenses/gpui" / name).read_bytes())
        declaration = ('[package]\nname = "' + self.item["name"] + '"\nversion = "'
                       + self.item["version"] + '"\nlicense = "' + self.item["declared_license"] + '"\n').encode()
        (self.source / "Cargo.toml").write_bytes(declaration)
        self.item["declaration"]["sha256"] = hashlib.sha256(declaration).hexdigest()
        (self.source / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": self.item["revision"]}}))
        self.entries = {"crate": {k: self.item[k] for k in ("name", "version", "repository")}}
        self.entries["crate"]["license"] = self.item["declared_license"]

    def test_apache_choice_preserves_upstream_notice(self):
        self.prepare_apache()
        artifacts = self.run_audit()
        self.assertEqual(len(artifacts), 3)
        notice = next(value for name, value in artifacts.items() if name.endswith("upstream-LICENSE.md"))
        self.assertIn(b"Apple SDKs", notice)

    def test_apache_terms_cannot_replace_mit_only_package(self):
        self.prepare_apache()
        self.item["declared_license"] = "MIT"
        self.entries["crate"]["license"] = "MIT"
        with self.assertRaisesRegex(ValueError, "Unreviewed Apache license choice"):
            self.run_audit()

    def test_apache_notice_cannot_come_from_different_revision(self):
        self.prepare_apache()
        self.item["upstream_notice"]["url"] = "https://raw.githubusercontent.com/madsmtm/objc2/main/LICENSE.md"
        with self.assertRaisesRegex(ValueError, "release revision"):
            self.run_audit()

    def test_apache_manifest_must_identify_the_resolved_package(self):
        self.prepare_apache()
        declaration = b'[package]\nname="another-package"\nversion="1.0.0"\nlicense="MIT"\n'
        (self.source / "Cargo.toml").write_bytes(declaration)
        self.item["declaration"]["sha256"] = hashlib.sha256(declaration).hexdigest()
        with self.assertRaisesRegex(ValueError, "Manifest declaration"):
            self.run_audit()


class HomepageProvenanceTests(unittest.TestCase):
    def setUp(self):
        records = json.loads((audit.ROOT / "licenses/gpui/supplements.json").read_text())["packages"]
        self.item = copy.deepcopy(next(item for item in records if item["name"] == "zune-inflate"))
        self.published = {"name": "zune-inflate", "version": "0.2.54",
                          "homepage": self.item["declared_homepage"], "license": "MIT OR Apache-2.0 OR Zlib"}
        self.vcs = {"path_in_vcs": "zune-inflate"}

    def test_reviewed_homepage_mapping_accepted(self):
        audit.validate_homepage_provenance(self.item, self.published, self.vcs)

    def test_changed_source_location_rejected(self):
        for field in ("homepage", "name", "version", "license"):
            with self.subTest(field=field):
                published = {**self.published, field: "changed"}
                with self.assertRaises(ValueError):
                    audit.validate_homepage_provenance(self.item, published, self.vcs)

    def test_other_subdirectory_rejected(self):
        with self.assertRaises(ValueError):
            audit.validate_homepage_provenance(self.item, self.published, {"path_in_vcs": "other-crate"})


if __name__ == "__main__":
    unittest.main()
