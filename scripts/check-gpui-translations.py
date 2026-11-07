"""Check literal GPUI choose(Chinese, English) calls against the legacy catalog.

This is a bounded migration check, not a Rust parser: dynamic messages,
raw strings need manual review. Native-only literal messages are explicitly
reviewed in gpui-translations.json; new unknown literal messages fail the check.
Run from any directory; no dependencies or application launch are required.
"""

import json
from pathlib import Path
import re
import sys


def check(root: Path) -> int:
    literal = r'("(?:[^"\\]|\\.)*")'
    catalog = json.loads((root / "crates/tp-gpui/resources/translations.json").read_text(encoding="utf-8"))
    if not catalog:
        raise RuntimeError("No translation catalog entries found")
    native = json.loads((root / "scripts/gpui-translations.json").read_text(encoding="utf-8"))
    if any(zh in catalog for zh in native):
        raise ValueError("Native translations must not override the original catalog")
    catalog.update(native)
    calls = re.compile(r"choose\(\s*" + literal + r"\s*,\s*" + literal + r"\s*,?\s*\)")
    checked = unmatched = unsupported = failures = 0
    for path in sorted((root / "crates/tp-gpui/src").rglob("*.rs")):
        source = path.read_text(encoding="utf-8")
        for match in calls.finditer(source):
            try:
                zh, en = map(json.loads, match.groups())
            except ValueError:
                unsupported += 1
                continue
            if zh not in catalog:
                unmatched += 1
                failures += 1
                line = source.count("\n", 0, match.start()) + 1
                print(f"{path.relative_to(root)}:{line}: unreviewed native translation: {zh!r}")
                continue
            checked += 1
            if en != catalog[zh]:
                failures += 1
                line = source.count("\n", 0, match.start()) + 1
                print(f"{path.relative_to(root)}:{line}: {zh!r}: {en!r}; expected {catalog[zh]!r}")
    if not checked:
        raise RuntimeError("No comparable literal calls found")
    print(f"Compared {checked} calls; {failures} mismatches; "
          f"{unmatched} unreviewed; {unsupported} unsupported literal calls excluded.")
    return int(failures != 0)


if __name__ == "__main__":
    sys.exit(check(Path(__file__).resolve().parent.parent))
