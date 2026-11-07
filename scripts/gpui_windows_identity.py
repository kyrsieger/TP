"""Check the GPUI Windows installer identity against the original TP installer.

Python 3.11+. Read-only; does not inspect or modify installed applications.
The historical contract was derived from the locked Tauri CLI 2.11.5 Windows
bundler and is preserved in packaging/windows/compatibility.json. It prevents
an accidental product rename from silently changing upgrades.
"""
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def native_identity(root=ROOT):
    """Read the native packaging contract without loading the legacy app."""
    frozen = json.loads((root / "packaging/windows/identity.json").read_text(encoding="utf-8"))
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    return {**frozen, "version": version}


def allow_downgrades(root=ROOT):
    policy = json.loads((root / "packaging/windows/policy.json").read_text(encoding="utf-8"))
    if type(policy.get("allowDowngrades")) is not bool:
        raise ValueError("allowDowngrades must be an explicit boolean")
    return policy["allowDowngrades"]


def check(root=ROOT):
    baseline = json.loads((root / "packaging/windows/compatibility.json").read_text(encoding="utf-8"))
    identity = native_identity(root)
    frozen = {key: value for key, value in identity.items() if key != "version"}
    actual = baseline["identity"]
    if frozen != actual:
        changed = sorted(key for key in frozen.keys() | actual.keys() if frozen.get(key) != actual.get(key))
        raise ValueError(f"Windows installer identity changed; review upgrade compatibility: {', '.join(changed)}")
    if allow_downgrades(root) != baseline["allowDowngrades"]:
        raise ValueError("Native and original downgrade policies differ")
    return identity


if __name__ == "__main__":
    print(json.dumps(check(), ensure_ascii=False, indent=2))
