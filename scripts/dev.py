"""Start TP with UI hot reload on Windows or macOS."""
from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
DX_VERSION = "0.7.10"
WINDOWS_DX = Path(f"target/hot-reload-tools/dioxus-{DX_VERSION}/target/debug/dx.exe")


def main():
    if sys.platform == "win32":
        dx = ROOT / WINDOWS_DX
        if not dx.is_file():
            print(f"Missing patched dx: {WINDOWS_DX}. See CONTRIBUTING.md for setup.", file=sys.stderr)
            return 1
    elif sys.platform == "darwin":
        dx = shutil.which("dx")
        if dx is None:
            print(f"Install Dioxus CLI {DX_VERSION} and add dx to PATH. See CONTRIBUTING.md.", file=sys.stderr)
            return 1
    else:
        print("TP supports Windows and macOS.", file=sys.stderr)
        return 1

    command = [
        dx, "serve", "--desktop", "--hot-patch", "--features", "hot-reload",
        "-p", "tp-gpui", "--locked", "--addr", "127.0.0.1", "--open", "false",
    ]
    try:
        return subprocess.run(command, cwd=ROOT).returncode
    except KeyboardInterrupt:
        return 130
    except OSError as error:
        print(f"Could not start dx: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
