"""Prepare the pinned NSIS distribution and plugin without executing either."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import stat
import urllib.request
import zipfile

from package_gpui_windows_nsis import ROOT, PLUGIN_SHA1, PLUGIN_SHA256, verify_compiler_files, verify_plugin

PLUGIN_URL = "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll"


def verified_bytes(data, sha256, sha1):
    if hashlib.sha256(data).hexdigest() != sha256 or hashlib.sha1(data).hexdigest() != sha1.lower():
        raise ValueError("Downloaded tool checksum mismatch")
    return data


def archive_files(data, manifest):
    files = {}
    seen = set()
    prefix = f"nsis-{manifest['version']}"
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for entry in archive.infolist():
            path = PurePosixPath(entry.filename)
            if (not path.parts or path.parts[0] != prefix or path.is_absolute()
                    or any(part in (".", "..") for part in path.parts)
                    or "\\" in entry.filename or ":" in entry.filename
                    or stat.S_ISLNK(entry.external_attr >> 16)):
                raise ValueError("Invalid tool archive member")
            if entry.is_dir():
                continue
            name = path.relative_to(prefix).as_posix()
            if not name or name.casefold() in seen:
                raise ValueError("Duplicate tool archive member")
            seen.add(name.casefold())
            files[name] = archive.read(entry)
    for name, expected in manifest["files"].items():
        if name not in files or hashlib.sha256(files[name]).hexdigest() != expected:
            raise ValueError(f"Tool archive file missing or changed: {name}")
    return files


def obtain(url, local):
    if local is not None:
        return Path(local).read_bytes()
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def prepare(output, archive_path=None, plugin_path=None):
    output = Path(output)
    if output.exists() or output.is_symlink():
        raise FileExistsError(f"Tool output already exists: {output}")
    manifest = json.loads((ROOT / "packaging/windows/nsis/toolchain.json").read_text(encoding="utf-8"))
    data = verified_bytes(obtain(manifest["url"], archive_path),
                          manifest["measuredArchiveSha256"], manifest["upstreamSha1"])
    files = archive_files(data, manifest)
    plugin = verified_bytes(obtain(PLUGIN_URL, plugin_path), PLUGIN_SHA256, PLUGIN_SHA1)
    files["Plugins/x86-unicode/additional/nsis_tauri_utils.dll"] = plugin
    # Validate all downloads before creating anything. Preserve a partial output
    # on filesystem failure for inspection; never overwrite or delete caller data.
    output.mkdir(parents=True, exist_ok=False)
    for name, content in files.items():
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    verify_compiler_files(output / "makensis.exe")
    verify_plugin(output / "Plugins/x86-unicode/additional/nsis_tauri_utils.dll")
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--archive", type=Path, help="Use a locally cached archive with the same pinned hashes")
    parser.add_argument("--plugin", type=Path, help="Use a locally cached plugin with the same pinned hashes")
    args = parser.parse_args()
    print(prepare(args.output, args.archive, args.plugin))
