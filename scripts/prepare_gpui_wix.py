"""Download and verify the pinned WiX 3.14.1 tools without executing them."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import stat
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def manifest():
    return json.loads((ROOT / "packaging/windows/wix/toolchain.json").read_text(encoding="utf-8"))


def archive_files(data, contract):
    if hashlib.sha256(data).hexdigest() != contract["sha256"]:
        raise ValueError("WiX archive checksum mismatch")
    files = {}
    seen = set()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for entry in archive.infolist():
            path = PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or ":" in entry.filename or stat.S_ISLNK(entry.external_attr >> 16)):
                raise ValueError("Invalid WiX archive member")
            if entry.is_dir():
                continue
            name = path.as_posix()
            if name.casefold() in seen or name not in contract["files"]:
                raise ValueError("Duplicate or unexpected WiX archive member")
            seen.add(name.casefold())
            content = archive.read(entry)
            if hashlib.sha256(content).hexdigest() != contract["files"][name]:
                raise ValueError(f"WiX archive file changed: {name}")
            files[name] = content
    if files.keys() != contract["files"].keys():
        raise ValueError("WiX archive files missing")
    return files


def verify_tools(directory, contract=None):
    contract = manifest() if contract is None else contract
    directory = Path(directory)
    for name, expected in contract["files"].items():
        path = directory / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f"WiX distribution file missing or changed: {name}")


def prepare(output, archive_path=None):
    output = Path(output)
    if output.exists() or output.is_symlink():
        raise FileExistsError(f"Tool output already exists: {output}")
    contract = manifest()
    if archive_path is None:
        with urllib.request.urlopen(contract["url"], timeout=60) as response:
            data = response.read()
    else:
        data = Path(archive_path).read_bytes()
    files = archive_files(data, contract)
    output.mkdir(parents=True, exist_ok=False)
    for name, content in files.items():
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    verify_tools(output, contract)
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    args = parser.parse_args()
    print(prepare(args.output, args.archive))
