import _support
import hashlib
import io
import tarfile
import unittest

from gpui_published_terms import TERMS_URLS, verify_package


class PublishedSourceTests(unittest.TestCase):
    def setUp(self):
        output = io.BytesIO()
        manifest = b'[package]\nname="example"\nversion="1.2.3"\nlicense="MIT"\nauthors=["Original Author"]\n'
        with tarfile.open(fileobj=output, mode="w:gz") as archive:
            entry = tarfile.TarInfo("example-1.2.3/Cargo.toml")
            entry.size = len(manifest)
            archive.addfile(entry, io.BytesIO(manifest))
        self.content = output.getvalue()
        digest = hashlib.sha256(self.content).hexdigest()
        self.package = {"name": "example", "version": "1.2.3", "license": "MIT", "source": "registry+https://github.com/rust-lang/crates.io-index"}
        self.lock = {**self.package, "checksum": digest}
        self.item = {"name": "example", "version": "1.2.3", "declared_license": "MIT", "license": "MIT",
                     "authors": ["Original Author"], "crate_sha256": digest, "terms_url": TERMS_URLS["MIT"],
                     "source_url": "https://crates.io/api/v1/crates/example/1.2.3/download"}

    def test_exact_published_declaration_is_preserved(self):
        self.assertIn(b'authors=["Original Author"]', verify_package(self.item, self.package, self.lock, self.content))

    def test_damaged_archive_is_rejected_before_reading(self):
        with self.assertRaisesRegex(ValueError, "checksum"):
            verify_package(self.item, self.package, self.lock, self.content + b"changed")

    def test_changed_lock_checksum_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "checksum"):
            verify_package(self.item, self.package, {**self.lock, "checksum": "wrong"}, self.content)

    def test_other_version_or_authorship_cannot_reuse_evidence(self):
        for field, value in (("version", "2.0.0"), ("authors", ["Different Author"])):
            with self.subTest(field=field), self.assertRaises(ValueError):
                verify_package({**self.item, field: value}, self.package, self.lock, self.content)

    def test_mit_only_cannot_be_assigned_apache(self):
        item = {**self.item, "license": "Apache-2.0", "terms_url": TERMS_URLS["Apache-2.0"]}
        with self.assertRaisesRegex(ValueError, "license choice"):
            verify_package(item, self.package, self.lock, self.content)

    def test_arbitrary_spdx_expression_requires_review(self):
        package = {**self.package, "license": "MIT AND Apache-2.0"}
        with self.assertRaisesRegex(ValueError, "license choice"):
            verify_package({**self.item, "declared_license": package["license"]}, package, self.lock, self.content)


if __name__ == "__main__":
    unittest.main()
