"""Inspect/extract an NSIS archive with 7-Zip, without executing the installer."""
import argparse
import hashlib
import json
from pathlib import Path, PureWindowsPath
import subprocess
import tempfile

from gpui_windows_identity import ROOT, check
from package_gpui_windows_msi import validate_stage
from package_gpui_windows_nsis import verify_plugin

INSTALLER_FILES = {
    '$pluginsdir/system.dll', '$pluginsdir/modern-wizard.bmp',
    '$pluginsdir/nsdialogs.dll', '$pluginsdir/nsis_tauri_utils.dll',
    '$pluginsdir/startmenu.dll', 'uninstall.exe',
}
SUPPORT_SOURCES = {
    '$pluginsdir/system.dll': 'Plugins/x86-unicode/System.dll',
    '$pluginsdir/nsdialogs.dll': 'Plugins/x86-unicode/nsDialogs.dll',
    '$pluginsdir/startmenu.dll': 'Plugins/x86-unicode/StartMenu.dll',
    '$pluginsdir/modern-wizard.bmp': 'Contrib/Graphics/Wizard/win.bmp',
}


def support_hashes():
    toolchain = json.loads((ROOT/'packaging/windows/nsis/toolchain.json').read_text(encoding='utf-8'))
    return {name: toolchain['files'][source] for name, source in SUPPORT_SOURCES.items()}


def archive_paths(listing):
    header, separator, entries = listing.partition('\n----------\n')
    if not separator or '\nType = Nsis\n' not in header:
        raise ValueError('Expected a readable NSIS archive')
    paths = []
    seen = set()
    for line in entries.splitlines():
        if not line.startswith('Path = '):
            continue
        name = line[7:]
        path = PureWindowsPath(name)
        if (path.is_absolute() or path.drive or not path.parts
                or any(part in ('.', '..') or ':' in part for part in name.replace('\\', '/').split('/'))
                or name.startswith(('/', '\\')) or name.endswith(('/', '\\'))):
            raise ValueError('Unsafe installer archive path')
        normalized = '/'.join(path.parts).lower()
        if normalized in seen:
            raise ValueError('Duplicate installer archive path')
        seen.add(normalized)
        paths.append(normalized)
    if not paths:
        raise ValueError('Empty installer archive')
    return set(paths)


def validate_archive_paths(paths, files):
    expected = {name.lower() for name in files} | INSTALLER_FILES
    if paths != expected:
        raise ValueError(f'Installer file set differs: {sorted(paths ^ expected)}')


def validate_extracted(directory, manifest):
    directory = directory.resolve()
    actual = {}
    for path in directory.rglob('*'):
        if path.is_symlink() or not path.resolve().is_relative_to(directory):
            raise ValueError('Extracted path escaped the verification directory')
        if path.is_file():
            name = path.relative_to(directory).as_posix().lower()
            if name in actual:
                raise ValueError('Duplicate extracted installer path')
            actual[name] = path
    validate_archive_paths(set(actual), [entry['name'] for entry in manifest['files']])
    for entry in manifest['files']:
        data = actual[entry['name'].lower()].read_bytes()
        if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest().lower() != entry['sha256'].lower():
            raise ValueError(f'Installer payload hash mismatch: {entry["name"]}')
    if actual['uninstall.exe'].read_bytes()[:2] != b'MZ':
        raise ValueError('Uninstaller executable is missing or invalid')
    verify_plugin(actual['$pluginsdir/nsis_tauri_utils.dll'])
    for name, expected in support_hashes().items():
        if hashlib.sha256(actual[name].read_bytes()).hexdigest() != expected:
            raise ValueError(f'Installer support file hash mismatch: {name}')


def verify(installer, stage, seven_zip):
    installer, stage = installer.resolve(), stage.resolve()
    identity = check()
    files = validate_stage(stage, identity)
    manifest = json.loads((stage/'package-manifest.json').read_text(encoding='utf-8-sig'))
    listing = subprocess.run([str(seven_zip), 'l', '-slt', '-sccUTF-8', str(installer)],
                             check=True, capture_output=True, encoding='utf-8').stdout
    validate_archive_paths(archive_paths(listing), files)
    target = (ROOT/'target').resolve()
    target.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='gpui-nsis-verify-', dir=target) as temporary:
        extraction = Path(temporary).resolve()
        # Validate the exact cleanup target before TemporaryDirectory can remove it.
        if extraction.parent != target:
            raise ValueError('Verification directory is outside the workspace target')
        subprocess.run([str(seven_zip), 'x', '-y', '-sccUTF-8', f'-o{extraction}', str(installer)],
                       check=True, capture_output=True, encoding='utf-8')
        validate_extracted(extraction, manifest)
    return {'installer': str(installer), 'version': identity['version'],
            'payloadFilesVerified': len(files),
            'utilityPluginVerified': 'nsis-tauri-utils v0.5.3',
            'supportFilesVerified': len(SUPPORT_SOURCES),
            'sha256': hashlib.sha256(installer.read_bytes()).hexdigest(),
            'installerExecuted': False,
            'limitations': 'Does not verify installer behavior, UI or generated uninstaller code; compiler integrity is checked separately at build time.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installer', type=Path, required=True)
    parser.add_argument('--stage', type=Path, required=True)
    parser.add_argument('--seven-zip', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.installer, args.stage, args.seven_zip), indent=2))
