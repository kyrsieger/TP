"""Bundle structure tests use synthetic headers, not runnable macOS binaries."""
import _support

import importlib.util
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("packager", _support.SCRIPTS / "package-gpui-macos.py")
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


def executable(cpu=0x0100000C, platform=1, extra=b""):
    version = struct.pack("<6I", 0x32, 24, platform, 0x000D0200, 0x000E0000, 0)
    commands = version + extra
    return struct.pack("<8I", 0xFEEDFACF, cpu, 0, 2, 1 + bool(extra), len(commands), 0, 0) + commands


class BundleTests(unittest.TestCase):
    def test_both_architectures_and_deployment_version(self):
        for cpu, arch in [(0x0100000C, "arm64"), (0x01000007, "x86_64")]:
            self.assertEqual(packager.inspect_macho(executable(cpu)), {
                "architecture": arch, "minimum_system_version": "13.2.0"})

    def test_rejects_windows_and_wrong_apple_platform(self):
        for data in [b"MZ" + bytes(64), executable(platform=2), executable(cpu=7), executable()[:-1]]:
            with self.assertRaises(ValueError):
                packager.inspect_macho(data)

    def test_rejects_missing_dynamic_library(self):
        name = b"@rpath/libmissing.dylib\0"
        name += bytes((-len(name)) % 8)
        command = struct.pack("<6I", 0xC, 24 + len(name), 24, 0, 0, 0) + name
        with self.assertRaisesRegex(ValueError, "must be bundled"):
            packager.inspect_macho(executable(extra=command))

    def test_rejects_inconsistent_commands(self):
        malformed = bytearray(executable())
        struct.pack_into("<I", malformed, 16, 3)
        with self.assertRaises(ValueError):
            packager.inspect_macho(malformed)

    def test_bundle_preserves_identity_assets_notices_and_executable(self):
        with tempfile.TemporaryDirectory(prefix="tp-bundle-test-") as folder:
            root = Path(folder)
            binary, notices, bundle = root / "input", root / "notices.txt", root / "TP.app"
            binary.write_bytes(executable())
            notices.write_text("Synthetic test notices only\n", encoding="utf-8")
            packager.assemble(binary, bundle, notices)
            contents = bundle / "Contents"
            info = plistlib.loads((contents / "Info.plist").read_bytes())
            self.assertEqual(info["CFBundleIdentifier"], "TP")
            self.assertEqual(info["CFBundleExecutable"], "TP")
            self.assertEqual(info["CFBundlePackageType"], "APPL")
            self.assertEqual(info["LSMinimumSystemVersion"], "13.2.0")
            self.assertEqual((contents / "MacOS/TP").read_bytes(), binary.read_bytes())
            self.assertEqual((contents / "Resources/icon.icns").read_bytes(),
                             (packager.ROOT / "packaging/icons/icon.icns").read_bytes())
            self.assertEqual((contents / "Resources/third-party-notices.txt").read_bytes(), notices.read_bytes())
            with self.assertRaises(FileExistsError):
                packager.assemble(binary, bundle, notices)
            self.assertEqual((contents / "MacOS/TP").read_bytes(), binary.read_bytes())

    def test_invalid_input_does_not_create_destination(self):
        with tempfile.TemporaryDirectory(prefix="tp-bundle-test-") as folder:
            root = Path(folder)
            binary, notices, bundle = root / "input", root / "notices.txt", root / "TP.app"
            binary.write_bytes(executable())
            notices.write_text("", encoding="utf-8")
            with self.assertRaises(ValueError):
                packager.assemble(binary, bundle, notices)
            self.assertFalse(bundle.exists())

    def test_bundle_assembly_without_legacy_configuration(self):
        with tempfile.TemporaryDirectory(prefix="tp-native-bundle-") as folder:
            root = Path(folder)
            for relative in ("Cargo.toml", "LICENSE", "packaging/windows/identity.json", "packaging/icons/icon.icns"):
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((packager.ROOT / relative).read_bytes())
            binary, notices = root / "input", root / "notices.txt"
            binary.write_bytes(executable())
            notices.write_text("Synthetic notices\n", encoding="utf-8")
            bundle = root / "TP.app"
            self.assertFalse((root / "src-tauri").exists())
            packager.assemble(binary, bundle, notices, root=root)
            info = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
            self.assertEqual(info["CFBundleIdentifier"], "TP")
            self.assertEqual(info["CFBundleName"], "TP")


if __name__ == "__main__":
    unittest.main()
