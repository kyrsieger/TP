"""Build and verify both TP Windows x64 installers with one command."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from build_native import ROOT, build_command
from gpui_windows_identity import native_identity
from package_gpui_windows_msi import build as build_msi
from package_gpui_windows_nsis import build as build_nsis
from prepare_gpui_nsis import prepare as prepare_nsis
from prepare_gpui_wix import prepare as prepare_wix
from verify_gpui_windows_nsis import verify as verify_nsis


WINDOWS_TARGET = "x86_64-pc-windows-msvc"


def run(*command, **kwargs):
    print("Running:", subprocess.list2cmdline(command), flush=True)
    return subprocess.run(command, cwd=ROOT, check=True, **kwargs)


def package_windows():
    if sys.platform != "win32":
        raise ValueError("Run this command on Windows to build the x64 installers.")
    seven_zip = shutil.which("7z.exe")
    if not seven_zip:
        raise ValueError("Install 7-Zip and add 7z.exe to PATH to verify the EXE installer.")
    powershell = shutil.which("pwsh.exe")
    if not powershell:
        raise ValueError("Install PowerShell 7 and add pwsh.exe to PATH to verify the installers.")

    version = native_identity(ROOT)["version"]
    metadata = run("cargo", "metadata", "--no-deps", "--format-version", "1", "--locked",
                   stdout=subprocess.PIPE, encoding="utf-8")
    target = Path(json.loads(metadata.stdout)["target_directory"])
    run("cargo", "fetch", "--locked")
    build_env = {**os.environ, "CARGO_BUILD_TARGET": WINDOWS_TARGET}
    run(*build_command(sys.platform), "-Offline", env=build_env)

    output = ROOT / "target/windows-installers"
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="work-", dir=output) as work_path:
        work = Path(work_path)
        audit = work / "license-audit.json"
        notices = work / "notices.txt"
        stage = work / "stage"
        run(sys.executable, "scripts/audit-gpui-licenses.py", "--output", audit)
        run(sys.executable, "scripts/build_gpui_notices.py", "--audit", audit.with_suffix(".zip"),
            "--output", notices)
        run(powershell, "-NoProfile", "-File", ROOT / "scripts/stage-gpui-windows.ps1",
            "-BinaryPath", target / WINDOWS_TARGET / "release/tp-gpui.exe", "-OutputDirectory", stage,
            "-NoticesPath", notices, "-ExpectedVersion", version)

        print("Building the NSIS and MSI installers...", flush=True)
        nsis = prepare_nsis(work / "nsis-tools")
        wix = prepare_wix(work / "wix-tools")
        exe = build_nsis(stage, work / "nsis", nsis / "makensis.exe",
                         nsis / "Plugins/x86-unicode/additional")
        msi = build_msi(stage, work / "msi", wix)
        verify_nsis(exe, stage, Path(seven_zip))
        run(powershell, "-NoProfile", "-File", ROOT / "scripts/verify-gpui-msi.ps1",
            "-MsiPath", msi, "-StageDirectory", stage, "-DarkPath", wix / "dark.exe")

        installers = work / "installers"
        installers.mkdir()
        exe.rename(installers / exe.name)
        msi.rename(installers / msi.name)
        destination = output / f"{version}-{work.name.removeprefix('work-')}"
        installers.rename(destination)

    print("Created unsigned Windows installers:")
    print(destination / exe.name)
    print(destination / msi.name)


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    try:
        package_windows()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Windows packaging failed: {error}")
