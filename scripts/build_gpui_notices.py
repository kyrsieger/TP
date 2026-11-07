"""Render native third-party notices from the verified audit ZIP, without network access."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def installer_texts(root=ROOT):
    entries = json.loads((root / "licenses/installers/manifest.json").read_text(encoding="utf-8"))
    result = []
    for entry in entries:
        path = (root / entry["file"]).resolve()
        if not path.is_relative_to(root.resolve()):
            raise ValueError("Installer notice path escapes repository")
        content = path.read_bytes()
        if hashlib.sha256(content).hexdigest() != entry["sha256"]:
            raise ValueError("Installer license text hash mismatch")
        result.append((entry["name"] + "\nSource: " + entry["source"], content))
    return result


def render(archive, lock_bytes, extra_texts):
    names = archive.namelist()
    if len(set(names)) != len(names):
        raise ValueError("Duplicate audit archive members")
    audit = json.loads(archive.read("audit.json"))
    if hashlib.sha256(lock_bytes).hexdigest() != audit["cargo_lock_sha256"]:
        raise ValueError("Audit does not match Cargo.lock; regenerate it")
    texts = {}
    sections = [
        "TP GPUI third-party notices\n\n"
        "This file covers the native Windows/macOS normal/build dependency union.\n"
        "Inclusion does not mean every dependency is linked on every platform.\n"
        "Third-party materials retain their own licenses. TP's MIT license does not replace them.\n"
        "Unmodified registry sources are available at each versioned download URL below.\n"
        "For standard-text supplements, declared authors are preserved separately;\n"
        "template placeholders do not invent or replace an upstream copyright notice.\n"
        "FFmpeg is optional, external, and not bundled. System font files are not bundled.\n"
    ]

    def add(label, content, expected=None):
        digest = hashlib.sha256(content).hexdigest()
        if expected is not None and digest != expected:
            raise ValueError(f"Audit text hash mismatch: {label}")
        # Fail on undecodable inputs rather than silently dropping notices.
        texts[digest] = content.decode("utf-8-sig")
        return f"{label}: see FULL TEXT {digest}"

    for label, content in extra_texts:
        sections.append(add(label, content))
    seen = set()
    for package in audit["packages"]:
        identity = (package["name"], package["version"])
        if identity in seen:
            raise ValueError("Ambiguous package identity in audit")
        seen.add(identity)
        prefix = "-".join(identity)
        lines = [f"cargo: {identity[0]} {identity[1]}", f"Declared license: {package['license']}",
                 "Targets: " + ", ".join(package["targets"])]
        if package["source"] is None:
            lines.append("Locally patched source: vendor/" + identity[0])
        else:
            lines.append(f"Source: https://crates.io/api/v1/crates/{identity[0]}/{identity[1]}/download")
        if package["repository"]:
            lines.append("Upstream: " + package["repository"])
        count = 0
        for item in package["text_files"]:
            lines.append(add(item["path"], archive.read(f"licenses/{prefix}/{item['path']}"), item["sha256"]))
            count += 1
        for item in package.get("supplemental_files", []):
            lines.append("Supplement source: " + item["url"])
            lines.append(add(item["file"], archive.read("supplements/" + item["file"]), item["sha256"]))
            count += 1
        for item in package.get("standard_terms", []):
            lines.append("Terms supplied: " + item["license"] + " — " + item["url"])
            base = f"standard-terms/{prefix}/"
            lines.append(add("Standard terms", archive.read(base + "terms.txt"), item["sha256"]))
            lines.append(add("Original declaration", archive.read(base + "declaration-source.txt"), item["declaration"]["sha256"]))
            if "upstream_notice" in item:
                notice = item["upstream_notice"]
                lines.append(add("Upstream license explanation", archive.read(base + "upstream-LICENSE.md"), notice["sha256"]))
            count += 1
        if item := package.get("published_terms"):
            base = f"published-terms/{prefix}/"
            source = archive.read(base + "source.crate")
            if hashlib.sha256(source).hexdigest() != item["crate_sha256"]:
                raise ValueError("Published source archive hash mismatch")
            lines.append("Declared authors: " + json.dumps(item["authors"], ensure_ascii=False))
            lines.append("Terms supplied: " + item["license"] + " — " + item["terms_url"])
            lines.append(add("Standard terms", archive.read(base + "terms.txt"), item["terms_sha256"]))
            count += 1
        if not count or package["missing_declared_files"]:
            raise ValueError(f"Missing license evidence for {prefix}")
        sections.append("\n".join(lines))
    for digest, text in sorted(texts.items()):
        sections.append(f"FULL TEXT {digest}\n\n{text}")
    return ("\n\n" + "=" * 72 + "\n\n").join(sections) + "\n"


def build(audit_path, output):
    extra = [("Lucide 1.57.0 icons, including Feather-derived MIT notice", (ROOT / "crates/tp-gpui/assets/icons/LICENSE").read_bytes()),
             ("TP modifications to gpui-component 0.5.1", (ROOT / "vendor/gpui-component/TP-PATCHES.md").read_bytes())]
    extra.extend(installer_texts())
    with zipfile.ZipFile(audit_path) as archive:
        result = render(archive, (ROOT / "Cargo.lock").read_bytes(), extra)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(result, encoding="utf-8")
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit", type=Path, default=ROOT / "target/gpui-license-audit.zip")
    parser.add_argument("--output", type=Path, default=ROOT / "target/gpui-third-party-notices.txt")
    args = parser.parse_args()
    print(build(args.audit, args.output))
