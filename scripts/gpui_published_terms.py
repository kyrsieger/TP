"""Collect standard terms with checksum-verified, unmodified published sources.

This supplies review inputs, not a conclusion about redistribution obligations.
No archive is extracted or package code executed.
"""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tomllib

TERMS_URLS = {
    "MIT": "https://raw.githubusercontent.com/spdx/license-list-data/31846481de7ea17d92f2f46fad76abe58b2e9bb0/text/MIT.txt",
    "CC0-1.0": "https://raw.githubusercontent.com/spdx/license-list-data/31846481de7ea17d92f2f46fad76abe58b2e9bb0/text/CC0-1.0.txt",
    "Apache-2.0": "https://www.apache.org/licenses/LICENSE-2.0.txt",
}
REVIEWED_CHOICES = {
    "MIT": "MIT", "CC0-1.0": "CC0-1.0", "MIT OR Apache-2.0": "Apache-2.0",
    "Apache-2.0 OR MIT": "Apache-2.0", "MIT / Apache-2.0": "Apache-2.0",
    "MIT/Apache-2.0": "Apache-2.0",
}


def verify_package(item, package, locked, content):
    identity = (item["name"], item["version"])
    if identity != (package["name"], package["version"]) or identity != (locked["name"], locked["version"]):
        raise ValueError("Published source version mismatch")
    if package["source"] != locked["source"] or not package["source"].startswith("registry+"):
        raise ValueError("Published source registry mismatch")
    digest = hashlib.sha256(content).hexdigest()
    if digest != item["crate_sha256"] or digest != locked["checksum"]:
        raise ValueError("Published source checksum mismatch")
    if (item["declared_license"] != package["license"]
            or REVIEWED_CHOICES.get(package["license"]) != item["license"]
            or TERMS_URLS.get(item["license"]) != item["terms_url"]):
        raise ValueError("Unreviewed published license choice")
    expected_url = f"https://crates.io/api/v1/crates/{identity[0]}/{identity[1]}/download"
    if item["source_url"] != expected_url:
        raise ValueError("Published source URL mismatch")
    with tarfile.open(fileobj=io.BytesIO(content), mode="r:gz") as archive:
        name = f"{identity[0]}-{identity[1]}/Cargo.toml"
        matches = [member for member in archive.getmembers() if member.name == name]
        if len(matches) != 1 or not matches[0].isfile() or matches[0].size > 1_000_000:
            raise ValueError("Invalid published manifest")
        manifest = archive.extractfile(matches[0]).read()
    declaration = tomllib.loads(manifest.decode("utf-8"))["package"]
    if any(declaration.get(key) != package[key] for key in ("name", "version", "license")):
        raise ValueError("Published manifest identity or license mismatch")
    if declaration.get("authors", []) != item["authors"]:
        raise ValueError("Published authorship changed")
    return manifest


def collect(root, entries, sources):
    root = Path(root)
    folder = root / "licenses/gpui"
    records = json.loads((folder / "published-terms.json").read_text(encoding="utf-8"))["packages"]
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    artifacts = {}
    seen = set()
    for item in records:
        identity = (item["name"], item["version"])
        if identity in seen:
            raise ValueError("Duplicate published license record")
        seen.add(identity)
        keys = [key for key, p in entries.items() if (p["name"], p["version"]) == identity]
        lock_matches = [p for p in locked if (p["name"], p["version"]) == identity and p.get("source") == entries[keys[0]]["source"]] if len(keys) == 1 else []
        if len(keys) != 1 or len(lock_matches) != 1:
            raise ValueError("Published terms must match one locked package")
        key = keys[0]
        source = sources[key]
        crate_name = f"{identity[0]}-{identity[1]}.crate"
        crate_path = source.parents[2] / "cache" / source.parent.name / crate_name
        content = crate_path.read_bytes()
        declaration = verify_package(item, entries[key], lock_matches[0], content)
        terms_path = (folder / item["terms_file"]).resolve()
        if not terms_path.is_relative_to(folder.resolve()):
            raise ValueError("Published terms path escapes license directory")
        terms = terms_path.read_bytes()
        if hashlib.sha256(terms).hexdigest() != item["terms_sha256"]:
            raise ValueError("Published standard terms hash mismatch")
        prefix = f"{identity[0]}-{identity[1]}"
        artifacts[f"{prefix}/source.crate"] = content
        artifacts[f"{prefix}/Cargo.toml"] = declaration
        artifacts[f"{prefix}/terms.txt"] = terms
        notice = (
            f"{item['name']} {item['version']}\n"
            f"Declared authors (not an invented copyright notice): {json.dumps(item['authors'], ensure_ascii=False)}\n"
            f"Published license declaration: {item['declared_license']}\n"
            f"Standard terms supplied: {item['license']}\n"
            f"Published source: {item['source_url']}\n"
            f"Cargo.lock archive SHA-256: {item['crate_sha256']}\n"
            "The original published source archive is preserved unmodified as source.crate.\n"
            "Standard terms do not replace existing source notices or establish missing copyright ownership.\n"
            "Attribution and final distribution review remain required.\n"
        )
        artifacts[f"{prefix}/NOTICE.txt"] = notice.encode("utf-8")
        entries[key]["published_terms"] = item
    return artifacts
