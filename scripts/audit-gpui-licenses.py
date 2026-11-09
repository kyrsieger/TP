"""Inventory native distribution dependencies; this is not a license approval gate.

Run from any directory with Python 3: python scripts/audit-gpui-licenses.py
Uses locked, offline Cargo metadata and writes a review report under target/.
Includes normal/build dependencies reachable from tp-gpui, excluding dev edges.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib
import zipfile
from gpui_published_terms import collect as collect_published_terms


ROOT = Path(__file__).resolve().parent.parent
TARGETS = ("x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin")


def reachable(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pending = [p["id"] for p in metadata["packages"] if p["name"] == "tp-gpui" and p["source"] is None]
    if len(pending) != 1:
        raise ValueError("Expected one local tp-gpui package")
    visited = set()
    while pending:
        key = pending.pop()
        if key in visited:
            continue
        visited.add(key)
        pending.extend(d["pkg"] for d in nodes[key]["deps"]
                       if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    return [packages[key] for key in visited
            if packages[key]["source"] is not None
            or Path(packages[key]["manifest_path"]).is_relative_to(ROOT / "vendor")]


def license_files(package):
    folder = Path(package["manifest_path"]).parent
    candidates = set()
    if package.get("license_file"):
        candidates.add(folder / package["license_file"])
    # Include nested vendored notices too. Record original relative paths and
    # hashes so reviewers can distinguish files instead of guessing from SPDX.
    for path in folder.rglob("*"):
        if path.is_file() and re.match(r"^(license|licence|copying|notice)(?:$|[.\-_])", path.name, re.I):
            candidates.add(path)
    texts = []
    missing = []
    for path in sorted(candidates):
        if not path.is_file():
            missing.append(str(path.relative_to(folder)))
            continue
        content = path.read_bytes()
        texts.append({"path": str(path.relative_to(folder)).replace("\\", "/"),
                      "sha256": hashlib.sha256(content).hexdigest(), "bytes": len(content)})
    return texts, missing


def apply_supplements(entries, sources):
    folder = ROOT / "licenses/gpui"
    manifest = json.loads((folder / "supplements.json").read_text(encoding="utf-8"))
    artifacts = {}
    for item in manifest["packages"]:
        matches = [key for key, package in entries.items()
                   if (package["name"], package["version"]) == (item["name"], item["version"])]
        if len(matches) != 1:
            raise ValueError(f"Supplement does not match one resolved package: {item['name']} {item['version']}")
        key = matches[0]
        package = entries[key]
        vcs = json.loads((sources[key] / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
        if package["repository"] != item["repository"] or vcs["git"]["sha1"] != item["revision"]:
            raise ValueError(f"Supplement provenance differs from resolved package: {item['name']}")
        if package["repository"] is None:
            published = tomllib.loads((sources[key] / "Cargo.toml").read_text(encoding="utf-8"))["package"]
            validate_homepage_provenance(item, published, vcs)
        path = (folder / item["file"]).resolve()
        if not path.is_relative_to(folder.resolve()):
            raise ValueError("Supplement path escapes the license directory")
        content = path.read_bytes()
        if hashlib.sha256(content).hexdigest() != item["sha256"]:
            raise ValueError(f"Supplement hash mismatch: {item['file']}")
        package.setdefault("supplemental_files", []).append({**item, "bytes": len(content)})
        artifacts[item["file"]] = content
    return artifacts


def validate_homepage_provenance(item, published, vcs):
    # A missing repository field must not silently erase the source check.
    # This fallback is restricted to the inspected zune-inflate release layout.
    repo = "https://github.com/etemesi254/zune-image"
    if (item["name"] != "zune-inflate" or item["version"] != "0.2.54"
            or item.get("upstream_repository") != repo
            or item.get("declared_homepage") != repo + "/tree/main/zune-inflate"
            or published.get("homepage") != item["declared_homepage"]
            or published.get("name") != item["name"] or published.get("version") != item["version"]
            or published.get("license") != "MIT OR Apache-2.0 OR Zlib"
            or item.get("license") != "Zlib" or vcs.get("path_in_vcs") != "zune-inflate"
            or item["url"] != f"https://raw.githubusercontent.com/etemesi254/zune-image/{item['revision']}/LICENSE-ZLIB"):
        raise ValueError("Supplement homepage provenance requires review")


def apply_standard_terms(entries, sources, folder=None):
    """Keep publisher terms separate from the crate's own license declaration."""
    folder = Path(folder) if folder is not None else ROOT / "licenses/gpui"
    manifest = json.loads((folder / "standard-terms.json").read_text(encoding="utf-8"))
    artifacts = {}
    for item in manifest["packages"]:
        matches = [key for key, p in entries.items()
                   if (p["name"], p["version"]) == (item["name"], item["version"])]
        if len(matches) != 1:
            raise ValueError("Standard terms must match one resolved package")
        key = matches[0]
        package = entries[key]
        source = sources[key].resolve()
        vcs = json.loads((source / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
        if (package["repository"] != item["repository"]
                or vcs["git"]["sha1"] != item["revision"]
                or package["license"] != item.get("declared_license", item["license"])):
            raise ValueError("Standard terms differ from package provenance or license")
        reviewed_urls = {
            "MPL-2.0": "https://www.mozilla.org/media/MPL/2.0/index.f75d2927d3c1.txt",
            "Apache-2.0": "https://www.apache.org/licenses/LICENSE-2.0.txt",
        }
        if (item["license"] not in reviewed_urls or item["url"] != reviewed_urls[item["license"]]
                or item["source_url"] != f"https://crates.io/api/v1/crates/{item['name']}/{item['version']}/download"):
            raise ValueError("Unreviewed standard terms source")
        declaration = item["declaration"]
        paths = [(folder, item["file"], item["sha256"]),
                 (source, declaration["path"], declaration["sha256"])]
        if item["license"] == "Apache-2.0":
            # This explicitly reviewed family offers a simple OR choice. Do not
            # interpret arbitrary SPDX expressions or replace MIT-only notices.
            if (package["license"] != "Zlib OR Apache-2.0 OR MIT"
                    or package["repository"] != "https://github.com/madsmtm/objc2"
                    or declaration["path"] != "Cargo.toml"):
                raise ValueError("Unreviewed Apache license choice")
            notice = item["upstream_notice"]
            if notice["url"] != f"https://raw.githubusercontent.com/madsmtm/objc2/{item['revision']}/LICENSE.md":
                raise ValueError("Upstream notice does not match the release revision")
            paths.append((folder, notice["file"], notice["sha256"]))
        contents = []
        for base, name, digest in paths:
            path = (base / name).resolve()
            if not path.is_relative_to(base.resolve()):
                raise ValueError("Standard terms path escapes its source directory")
            content = path.read_bytes()
            if hashlib.sha256(content).hexdigest() != digest:
                raise ValueError("Standard terms or declaration hash mismatch")
            contents.append(content)
        if item["license"] == "MPL-2.0" and b"https://mozilla.org/MPL/2.0/" not in contents[1]:
            raise ValueError("Source declaration does not reference MPL 2.0")
        if item["license"] == "Apache-2.0":
            manifest_package = tomllib.loads(contents[1].decode("utf-8"))["package"]
            if any(manifest_package.get(field) != package[field] for field in ("name", "version", "license")):
                raise ValueError("Manifest declaration does not match the resolved package")
        prefix = f"{item['name']}-{item['version']}"
        artifacts[f"{prefix}/terms.txt"] = contents[0]
        artifacts[f"{prefix}/declaration-source.txt"] = contents[1]
        if len(contents) == 3:
            artifacts[f"{prefix}/upstream-LICENSE.md"] = contents[2]
        package.setdefault("standard_terms", []).append(item)
    return artifacts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/gpui-license-audit.json",
                        help="Path for the JSON report and its companion ZIP")
    output = parser.parse_args().output
    entries = {}
    sources = {}
    for target in TARGETS:
        result = subprocess.run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1",
                                 "--filter-platform", target], cwd=ROOT, check=True, capture_output=True, text=True, encoding="utf-8")
        for package in reachable(json.loads(result.stdout)):
            key = package["id"]
            if key not in entries:
                sources[key] = Path(package["manifest_path"]).parent
                texts, missing = license_files(package)
                entries[key] = {"name": package["name"], "version": package["version"],
                                "source": package["source"], "license": package["license"],
                                "repository": package.get("repository"),
                                "text_files": texts, "missing_declared_files": missing, "targets": []}
            entries[key]["targets"].append(target)
    supplements = apply_supplements(entries, sources)
    standard_terms = apply_standard_terms(entries, sources)
    published_terms = collect_published_terms(ROOT, entries, sources)
    packages = sorted(entries.values(), key=lambda p: (p["name"], p["version"]))
    report = {"scope": "tp-gpui normal/build dependency union; offline Cargo metadata; not legal approval",
              "targets": TARGETS, "cargo_lock_sha256": hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(),
              "packages": packages}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    # A review artifact, not the application's completed distribution notice.
    # Preserve exact bytes and stable archive metadata for repeatable review.
    bundle = output.with_suffix(".zip")
    with zipfile.ZipFile(bundle, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        def write(name, content):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, content)

        write("audit.json", output.read_bytes())
        for name, content in sorted(supplements.items()):
            write(f"supplements/{name}", content)
        for name, content in sorted(standard_terms.items()):
            write(f"standard-terms/{name}", content)
        for name, content in sorted(published_terms.items()):
            write(f"published-terms/{name}", content)
        for key, package in sorted(entries.items()):
            prefix = f"licenses/{package['name']}-{package['version']}"
            for item in package["text_files"]:
                content = (sources[key] / item["path"]).read_bytes()
                if hashlib.sha256(content).hexdigest() != item["sha256"]:
                    raise ValueError(f"License changed during collection: {prefix}/{item['path']}")
                write(f"{prefix}/{item['path']}", content)
    print(f"{len(packages)} native dependency packages")
    print(f"{sum((not p['text_files'] and not p.get('supplemental_files') and not p.get('standard_terms') and not p.get('published_terms')) or p['missing_declared_files'] != [] for p in packages)} packages missing license text evidence; attribution and distribution review still required")
    print(output)
    print(bundle)


if __name__ == "__main__":
    main()
