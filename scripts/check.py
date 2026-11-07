"""Explicit full local check entry point. Requires Python 3.11+ and Rust."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
STEPS = [
    ['cargo', 'fmt', '--all', '--', '--check'],
    ['rustfmt', '--edition', '2024', '--check', 'vendor/gpui-component/src/input/state.rs',
     'vendor/gpui-component/src/input/utf16_limit.rs', 'vendor/gpui-component/src/scroll/scrollbar.rs'],
    [sys.executable, 'scripts/check-gpui-translations.py'],
    [sys.executable, 'scripts/gpui_windows_identity.py'],
    [sys.executable, '-m', 'unittest', 'discover', '-s', 'tests/scripts', '-p', 'test_*.py'],
    ['cargo', 'check', '--workspace', '--locked'],
    ['cargo', 'test', '--workspace', '--locked'],
    ['cargo', 'build', '-p', 'tp-gpui', '--locked'],
]
if __name__ == '__main__':
    for step in STEPS:
        print('Running:', ' '.join(step), flush=True)
        subprocess.run(step, cwd=ROOT, check=True)
