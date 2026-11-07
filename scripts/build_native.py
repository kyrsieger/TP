"""Build the native release on Windows or macOS, without Node."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent

def build_command(platform):
    if platform == 'win32':
        return ['powershell.exe', '-NoProfile', '-File', str(ROOT / 'scripts/build-gpui-windows.ps1')]
    if platform == 'darwin':
        return ['cargo', 'build', '-p', 'tp-gpui', '--release', '--locked']
    raise ValueError('TP supports Windows and macOS')

if __name__ == '__main__':
    sys.exit(subprocess.run(build_command(sys.platform), cwd=ROOT).returncode)
