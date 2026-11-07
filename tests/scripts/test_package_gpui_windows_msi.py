import _support
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

from gpui_windows_identity import check
from package_gpui_windows_msi import NS, author, validate_stage


class WindowsMsiTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.stage = Path(self.temp.name) / "stage"
        self.stage.mkdir()
        self.identity = check()
        self.manifest = {
            "version": self.identity["version"], "architecture": "x64", "executable": "TP.exe",
            "installerIdentity": {k: v for k, v in self.identity.items() if k != "version"},
            "files": [],
        }
        for name in ("TP.exe", "LICENSE", "third-party-notices.txt", "vcruntime140.dll"):
            data = f"Test-only fixture: {name}".encode()
            (self.stage / name).write_bytes(data)
            self.manifest["files"].append({"name": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        self.save_manifest()

    def save_manifest(self):
        (self.stage / "package-manifest.json").write_text(json.dumps(self.manifest), encoding="utf-8")

    def test_identity_ui_shortcuts_and_payload_authoring(self):
        files = validate_stage(self.stage, self.identity)
        output = Path(self.temp.name) / "output"
        output.mkdir()
        author(self.stage, output, self.identity, files)
        tree = ET.parse(output / "gpui.wxs")
        ns = {"w": NS}
        product = tree.find("w:Product", ns)
        self.assertEqual(product.attrib["UpgradeCode"], self.identity["msiUpgradeCode"])
        self.assertEqual(product.attrib["Manufacturer"], "TP")
        self.assertEqual(product.attrib["Language"], "2052")
        self.assertEqual(tree.find(".//w:UIRef", ns).attrib["Id"], "WixUI_InstallDir")
        upgrade = tree.find(".//w:MajorUpgrade", ns)
        self.assertEqual(upgrade.attrib["Schedule"], "afterInstallInitialize")
        self.assertEqual(upgrade.attrib["AllowDowngrades"], "yes")
        self.assertEqual(tree.find(".//w:Property[@Id='REINSTALLMODE']", ns).attrib["Value"], "amus")
        payload = tree.findall(".//w:File", ns)
        self.assertEqual({Path(item.attrib["Source"]).name for item in payload}, set(files))
        self.assertEqual(len(tree.findall(".//w:Shortcut", ns)), 3)
        self.assertNotIn("WebView", (output / "gpui.wxs").read_text(encoding="utf-8"))

    def test_machine_uninstall_shortcut_does_not_use_user_key_path(self):
        output = Path(self.temp.name) / "output"
        output.mkdir()
        author(self.stage, output, self.identity, validate_stage(self.stage, self.identity))
        tree = ET.parse(output / "gpui.wxs")
        ns = {"w": NS}
        component = tree.find(".//w:Component[@Id='CMP_UninstallShortcut']", ns)
        self.assertEqual(component.attrib["Win64"], "yes")
        key = component.find("w:RegistryValue", ns)
        self.assertEqual(key.attrib["Root"], "HKLM")
        self.assertEqual(key.attrib["KeyPath"], "yes")
        self.assertEqual(component.find("w:Shortcut", ns).attrib["Arguments"], "/x [ProductCode]")
        # Preserve old install-location discovery; it is not this component's key.
        searches = tree.findall(".//w:RegistrySearch", ns)
        self.assertEqual(len(searches), 2)
        self.assertTrue(all(item.attrib["Root"] == "HKCU" for item in searches))

    def test_modified_payload_rejected(self):
        (self.stage / "TP.exe").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "Staged file changed"):
            validate_stage(self.stage, self.identity)

    def test_unlisted_payload_rejected(self):
        (self.stage / "unexpected.dll").write_bytes(b"unexpected")
        with self.assertRaisesRegex(ValueError, "Unlisted files"):
            validate_stage(self.stage, self.identity)

    def test_filename_traversal_rejected(self):
        self.manifest["files"][0]["name"] = "../TP.exe"
        self.save_manifest()
        with self.assertRaisesRegex(ValueError, "Invalid or duplicate"):
            validate_stage(self.stage, self.identity)

    def test_identity_change_rejected(self):
        self.manifest["installerIdentity"]["msiUpgradeCode"] = "11111111-2222-3333-4444-555555555555"
        self.save_manifest()
        with self.assertRaisesRegex(ValueError, "compatibility contract"):
            validate_stage(self.stage, self.identity)


if __name__ == "__main__":
    unittest.main()
