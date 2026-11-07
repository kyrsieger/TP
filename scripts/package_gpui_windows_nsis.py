"""Compile a GPUI NSIS package; never install it or run the packaged application.

Reuse the pinned legacy installer's UI/upgrade logic, not its WebView runtime.
The small renderer accepts only the constructs in that hash-checked template.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

from gpui_windows_identity import ROOT, native_identity, allow_downgrades
from package_gpui_windows_msi import validate_stage

UPSTREAM = ROOT / "packaging/windows/nsis/upstream"
# Official CLI revision above pins nsis-tauri-utils v0.5.3 with this SHA-1.
# SHA-256 was computed from the local distribution after matching that pin.
PLUGIN_SHA1 = '75197fee3c6a814fe035788d1c34ead39349b860'
PLUGIN_SHA256 = '5ba143b5db4a87d32d6e7802e033330aae56cbceabe0d1e3ba41948385ad4709'


def verify_compiler_files(compiler):
    manifest = json.loads((UPSTREAM.parent/'toolchain.json').read_text(encoding='utf-8'))
    if compiler.name.lower() != 'makensis.exe':
        raise ValueError('Expected the distribution root makensis.exe')
    for name, expected in manifest['files'].items():
        path = compiler.parent/name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f'NSIS distribution file missing or changed: {name}')


def verify_plugin(path):
    if not path.is_file():
        raise ValueError('Pinned NSIS utility plugin is missing')
    data = path.read_bytes()
    if (hashlib.sha1(data).hexdigest() != PLUGIN_SHA1
            or hashlib.sha256(data).hexdigest() != PLUGIN_SHA256):
        raise ValueError('NSIS utility plugin differs from pinned v0.5.3')


def verify_tools(compiler, plugins):
    # Check the plugin before running the explicitly supplied compiler.
    verify_plugin(plugins/'nsis_tauri_utils.dll')
    if not compiler.is_file():
        raise ValueError('NSIS compiler is missing')
    verify_compiler_files(compiler)
    version = subprocess.run([str(compiler), '/VERSION'], check=True,
                             capture_output=True, text=True).stdout.strip()
    if version != 'v3.11':
        raise ValueError(f'Expected NSIS v3.11, received {version!r}')


def verify_upstream(directory=UPSTREAM):
    manifest = json.loads((directory / "provenance.json").read_text(encoding="utf-8"))
    if manifest["commit"] != "9452ddee5ebefd9b678a94ff003521379df6c9ae":
        raise ValueError("Review installer compatibility before changing the upstream revision")
    for name, entry in manifest["files"].items():
        if Path(name).name != name:
            raise ValueError("Invalid upstream filename")
        if hashlib.sha256((directory / name).read_bytes()).hexdigest() != entry["sha256"]:
            raise ValueError(f"Upstream installer source changed: {name}")


def escape(value):
    # Match the pinned bundler's register_escape_fn; never interpret a path as NSIS code.
    return ''.join({'$': '$$', '"': '$\\"', '`': '$\\`', '\n': '$\\n',
                    '\r': '$\\r', '\t': '$\\t'}.get(c, c) for c in str(value))


def render(source, values):
    """Strict subset for the pinned template; unsupported live expressions fail closed."""
    tokens = re.split(r'(\{\{.*?\}\})', source, flags=re.S)

    def parse(start=0, end=None):
        nodes = []
        i = start
        while i < len(tokens):
            token = tokens[i]
            if not token.startswith('{{'):
                nodes.append(('text', token)); i += 1; continue
            expression = token[2:-2].strip().rstrip('~').strip()
            if expression.startswith('/'):
                if expression[1:] != end:
                    raise ValueError("Unbalanced installer template")
                return nodes, i + 1
            if expression.startswith('#'):
                operation, argument = expression[1:].split(' ', 1)
                if operation not in ('if', 'each'):
                    raise ValueError("Unsupported template block")
                children, i = parse(i + 1, operation)
                nodes.append((operation, argument, children))
            else:
                nodes.append(('value', expression)); i += 1
        if end is not None:
            raise ValueError("Unclosed installer template block")
        return nodes, i

    def emit(nodes, local):
        result = []
        for node in nodes:
            kind, expression = node[:2]
            if kind == 'text':
                result.append(expression)
            elif kind == 'value':
                if expression == 'this.[1]':
                    value = local['this'][1]
                elif expression == 'no-escape @key':
                    # Resource paths are validated/escaped here even though upstream
                    # uses no-escape for its own already-escaped map keys.
                    value = local['@key']
                else:
                    value = local[expression]
                result.append(escape(value))
            elif kind == 'if':
                if local[expression]:
                    result.append(emit(node[2], local))
            else:
                name = expression.split(' as ')[0]
                entries = local[name]
                if entries and ' as ' in expression:
                    raise ValueError("File associations/protocols require explicit implementation")
                pairs = entries.items() if isinstance(entries, dict) else enumerate(entries)
                for key, item in pairs:
                    result.append(emit(node[2], {**local, 'this': item, '@key': key}))
        return ''.join(result)

    return emit(parse()[0], values)


def author(stage, output, identity, files, plugins):
    verify_upstream()
    if identity['nsisInstallMode'] != 'currentUser' or identity['nsisLanguages'] != ['SimpChinese']:
        raise ValueError("Review NSIS scope/localization compatibility before changing it")
    template = (UPSTREAM / 'installer.nsi').read_text(encoding='utf-8')
    # Remove the entire runtime-specific section, including registry probes.
    template, count = re.subn(r'^Section WebView2\r?\n.*?^SectionEnd\r?\n', '', template,
                              flags=re.M | re.S)
    if count != 1:
        raise ValueError("Expected exactly one legacy WebView installation section")
    result = output / f"TP_{identity['version']}_x64-setup.exe"
    values = dict.fromkeys([
        'signed_plugins_path', 'homepage', 'license', 'sidebar_image', 'header_image',
        'uninstaller_header_image', 'uninstaller_sign_cmd', 'start_menu_folder',
        'webview2_installer_args', 'webview2_bootstrapper_path', 'webview2_installer_path',
        'minimum_webview2_version', 'copyright'], '')
    values.update(compression='lzma', manufacturer=identity['publisher'],
                  product_name=identity['productName'], version=identity['version'],
                  version_with_build=identity['version']+'.0', install_mode=identity['nsisInstallMode'],
                  installer_icon=ROOT/'packaging/icons/icon.ico',
                  uninstaller_icon=ROOT/'packaging/icons/icon.ico',
                  main_binary_name=Path(identity['executable']).stem,
                  main_binary_path=stage/identity['executable'], bundle_id=identity['bundleIdentifier'],
                  out_file=result, arch='x64', additional_plugins_path=plugins,
                  allow_downgrades=str(allow_downgrades()).lower(),
                  display_language_selector='false', install_webview2_mode='skip',
                  estimated_size=sum((stage/n).stat().st_size for n in files)//1024,
                  installer_hooks=ROOT/'packaging/windows/installer/hooks.nsh',
                  languages=identity['nsisLanguages'], language_files=[output/'SimpChinese.nsh'],
                  resources={str(stage/n): [None, n] for n in files if n != identity['executable']},
                  binaries={}, resources_dirs=[], resources_ancestors=[],
                  file_associations=[], deep_link_protocols=[])
    script = render(template, values)
    # NSIS otherwise defaults to the machine's ANSI codepage for input scripts.
    (output/'installer.nsi').write_text(script, encoding='utf-8-sig')
    locale = (ROOT/'packaging/windows/installer/SimpChinese.nsh').read_text(encoding='utf-8')
    (output/'SimpChinese.nsh').write_text(render(locale, values), encoding='utf-8-sig')
    for name in ('utils.nsh', 'FileAssociation.nsh'):
        shutil.copyfile(UPSTREAM/name, output/name)
    return result


def build(stage, output, compiler, plugins):
    identity = native_identity()
    stage, output, compiler, plugins = (p.resolve() for p in (stage, output, compiler, plugins))
    files = validate_stage(stage, identity)
    verify_tools(compiler, plugins)
    output.mkdir(parents=True, exist_ok=False)
    result = author(stage, output, identity, files, plugins)
    subprocess.run([str(compiler), '/V3', str(output/'installer.nsi')], cwd=output, check=True)
    if not result.is_file():
        raise ValueError("NSIS did not produce the requested installer")
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--plugins', type=Path, required=True)
    args = parser.parse_args()
    print(build(args.stage, args.output, args.compiler, args.plugins))
