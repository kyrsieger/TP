"""Assemble an unsigned TP.app from a native macOS GPUI executable.

Python 3.11+. Does not build, sign, notarize, install, or overwrite an app.
A complete, reviewed third-party notice file must be supplied explicitly.
"""
import argparse
from pathlib import Path
import plistlib
import shutil
import struct
from gpui_windows_identity import native_identity

ROOT = Path(__file__).resolve().parent.parent


def inspect_macho(data):
    """Read a thin 64-bit executable's target and deployment version."""
    if len(data) < 32 or data[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("Expected a thin little-endian 64-bit Mach-O executable")
    _, cpu, _, kind, count, size, _, _ = struct.unpack_from("<8I", data)
    if kind != 2 or cpu not in (0x01000007, 0x0100000C):
        raise ValueError("Expected an x86_64 or arm64 macOS executable")
    end = 32 + size
    if end > len(data) or count > size // 8:
        raise ValueError("Truncated Mach-O load commands")
    offset = 32
    minimum = None
    for _ in range(count):
        if offset + 8 > end:
            raise ValueError("Truncated Mach-O load command header")
        command, length = struct.unpack_from("<2I", data, offset)
        if length < 8 or length % 4 or offset + length > end:
            raise ValueError("Invalid Mach-O load command size")
        if command == 0x32:  # LC_BUILD_VERSION
            if length < 24:
                raise ValueError("Truncated build version")
            platform, version = struct.unpack_from("<2I", data, offset + 8)
            if platform != 1:
                raise ValueError("Executable targets an Apple platform other than macOS")
            minimum = max(minimum or 0, version)
        elif command == 0x24:  # LC_VERSION_MIN_MACOSX
            if length < 16:
                raise ValueError("Truncated deployment version")
            minimum = max(minimum or 0, struct.unpack_from("<I", data, offset + 8)[0])
        elif command in (0xC, 0x80000018, 0x8000001F, 0x20, 0x80000023):
            # This assembler does not copy/rewrite third-party dylibs.
            if length < 24:
                raise ValueError("Truncated dynamic library command")
            name_offset = struct.unpack_from("<I", data, offset + 8)[0]
            if not 24 <= name_offset < length:
                raise ValueError("Invalid dynamic library name offset")
            name = data[offset + name_offset:offset + length].split(b"\0", 1)[0].decode("utf-8")
            if not name.startswith(("/usr/lib/", "/System/Library/")):
                raise ValueError(f"Non-system dynamic library must be bundled first: {name}")
        offset += length
    if offset != end or not minimum:
        raise ValueError("Missing macOS deployment version or inconsistent load commands")
    return {
        "architecture": "arm64" if cpu == 0x0100000C else "x86_64",
        "minimum_system_version": f"{minimum >> 16}.{(minimum >> 8) & 255}.{minimum & 255}",
    }


def assemble(binary, output, notices, root=ROOT):
    binary, output, notices = Path(binary), Path(output), Path(notices)
    if output.suffix != ".app":
        raise ValueError("Output must end in .app")
    if output.exists() or output.is_symlink():
        raise FileExistsError(f"Output already exists: {output}")
    executable = binary.read_bytes()
    target = inspect_macho(executable)
    notice_text = notices.read_text(encoding="utf-8")
    if not notice_text.strip():
        raise ValueError("Third-party notices must not be empty")
    identity = native_identity(root)
    version = identity["version"]
    icon = (root / "packaging/icons/icon.icns").read_bytes()
    license_text = (root / "LICENSE").read_bytes()
    info = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": identity["productName"],
        "CFBundleName": identity["productName"],
        "CFBundleIdentifier": identity["bundleIdentifier"],
        "CFBundleExecutable": "TP",
        "CFBundleIconFile": "icon.icns",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "CFBundleLocalizations": ["en", "zh_CN"],
        "LSMinimumSystemVersion": target["minimum_system_version"],
        "NSHighResolutionCapable": True,
    }
    # Validate all inputs before creating the destination. Never delete a user's
    # existing bundle, even when a previous build is incomplete.
    output.mkdir(parents=True, exist_ok=False)
    contents = output / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    resources = contents / "Resources"
    resources.mkdir()
    program = contents / "MacOS/TP"
    program.write_bytes(executable)
    program.chmod(0o755)
    (contents / "Info.plist").write_bytes(plistlib.dumps(info, sort_keys=True))
    (contents / "PkgInfo").write_bytes(b"APPL????")
    (resources / "icon.icns").write_bytes(icon)
    (resources / "LICENSE").write_bytes(license_text)
    shutil.copyfile(notices, resources / "third-party-notices.txt")
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--notices", type=Path, required=True,
                        help="Complete reviewed third-party notices for the native application")
    args = parser.parse_args()
    target = assemble(args.binary, args.output, args.notices)
    print(f"Created unsigned {args.output}: {target}")
    print("Signing, notarization and macOS runtime validation remain separate steps.")


if __name__ == "__main__":
    main()
