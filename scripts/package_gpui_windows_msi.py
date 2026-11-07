"""Build an unsigned x64 GPUI MSI using WiX 3; never installs or launches it.

Installer identity and UI structure follow the project's original Tauri 2.11.5
MSI configuration. No Tauri executable or WebView bootstrapper is packaged.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import uuid
import xml.etree.ElementTree as ET

from gpui_windows_identity import ROOT, native_identity, allow_downgrades
from prepare_gpui_wix import verify_tools

NS = "http://schemas.microsoft.com/wix/2006/wi"
TAURI_COMPONENT_NAMESPACE = uuid.UUID("fd8595a8-17a3-474e-a616-76148dfa0c7b")
ET.register_namespace("", NS)


def validate_stage(stage, identity):
    stage = stage.resolve()
    manifest = json.loads((stage / "package-manifest.json").read_text(encoding="utf-8-sig"))
    frozen = {k: v for k, v in identity.items() if k != "version"}
    if manifest.get("installerIdentity") != frozen or manifest.get("version") != identity["version"]:
        raise ValueError("Staged installer identity/version differs from the compatibility contract")
    if manifest.get("architecture") != "x64" or manifest.get("executable") != identity["executable"]:
        raise ValueError("Expected the x64 TP executable")
    names = set()
    for entry in manifest["files"]:
        name = entry["name"]
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", name) or name.lower() in names:
            raise ValueError("Invalid or duplicate staged filename")
        names.add(name.lower())
        path = stage / name
        if path.is_symlink() or path.resolve().parent != stage:
            raise ValueError("Staged files must be contained regular files")
        data = path.read_bytes()
        if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest().upper() != entry["sha256"].upper():
            raise ValueError(f"Staged file changed: {name}")
    if not {identity["executable"].lower(), "license", "third-party-notices.txt"} <= names:
        raise ValueError("Required executable or notices missing")
    actual = {p.name.lower() for p in stage.iterdir()}
    if actual != names | {"package-manifest.json"}:
        raise ValueError("Unlisted files in staging directory")
    return sorted(entry["name"] for entry in manifest["files"])


def node(parent, tag, text=None, **attrs):
    item = ET.SubElement(parent, f"{{{NS}}}{tag}", {k: str(v) for k, v in attrs.items()})
    item.text = text
    return item


def author(stage, output, identity, files):
    # WiX preprocessor syntax is not escaped by XML encoding.
    for path in (stage, output, ROOT):
        if "$(" in str(path) or "!(" in str(path):
            raise ValueError("WiX preprocessor markers are not supported in build paths")
    wix = ET.Element(f"{{{NS}}}Wix")
    product = node(wix, "Product", Id="*", Name=identity["productName"],
                   UpgradeCode=identity["msiUpgradeCode"], Language="2052",
                   Manufacturer=identity["publisher"], Version=identity["version"])
    node(product, "Package", Id="*", InstallerVersion="450", Compressed="yes",
         InstallScope=identity["msiInstallScope"], SummaryCodepage="936")
    node(product, "Media", Id="1", Cabinet="app.cab", EmbedCab="yes")
    if allow_downgrades():
        node(product, "MajorUpgrade", Schedule="afterInstallInitialize", AllowDowngrades="yes")
    else:
        node(product, "MajorUpgrade", Schedule="afterInstallInitialize", AllowSameVersionUpgrades="yes",
             DowngradeErrorMessage="!(loc.DowngradeErrorMessage)")
    node(product, "Property", Id="REINSTALLMODE", Value="amus")
    node(product, "Property", Id="ARPNOREPAIR", Value="yes", Secure="yes")
    node(product, "SetProperty", Id="ARPNOMODIFY", Value="1", After="InstallValidate", Sequence="execute")
    node(product, "Icon", Id="ProductIcon", SourceFile=str(ROOT / "packaging/icons/icon.ico"))
    node(product, "Property", Id="ARPPRODUCTICON", Value="ProductIcon")
    location = node(product, "Property", Id="INSTALLDIR")
    for name, registry_name in (("PrevInstallDirNoName", None), ("PrevInstallDirWithName", "InstallDir")):
        attrs = dict(Id=name, Root="HKCU", Key=identity["installLocationRegistryKey"], Type="raw")
        if registry_name:
            attrs["Name"] = registry_name
        node(location, "RegistrySearch", **attrs)
    for name in ("AUTOLAUNCHAPP", "LAUNCHAPPARGS"):
        node(product, "Property", Id=name, Secure="yes")
    node(product, "Property", Id="WIXUI_EXITDIALOGOPTIONALCHECKBOXTEXT", Value="!(loc.LaunchApp)")
    node(product, "Property", Id="WIXUI_EXITDIALOGOPTIONALCHECKBOX", Value="1")
    node(product, "CustomAction", Id="LaunchApplication", Impersonate="yes", FileKey="Path",
         ExeCommand="[LAUNCHAPPARGS]", Return="asyncNoWait")
    sequence = node(product, "InstallExecuteSequence")
    node(sequence, "RemoveShortcuts", "Installed AND NOT UPGRADINGPRODUCTCODE")
    node(sequence, "Custom", "AUTOLAUNCHAPP AND NOT Installed", Action="LaunchApplication", After="InstallFinalize")
    ui = node(product, "UI")
    node(ui, "Publish", "WIXUI_EXITDIALOGOPTIONALCHECKBOX = 1 and NOT Installed", Dialog="ExitDialog",
         Control="Finish", Event="DoAction", Value="LaunchApplication")
    node(ui, "Property", Id="WIXUI_INSTALLDIR", Value="INSTALLDIR")
    node(ui, "Publish", "1", Dialog="WelcomeDlg", Control="Next", Event="NewDialog", Value="InstallDirDlg", Order="2")
    node(ui, "Publish", "1", Dialog="InstallDirDlg", Control="Back", Event="NewDialog", Value="WelcomeDlg", Order="2")
    node(product, "UIRef", Id="WixUI_InstallDir")
    target = node(product, "Directory", Id="TARGETDIR", Name="SourceDir")
    desktop = node(target, "Directory", Id="DesktopFolder", Name="Desktop")
    program_files = node(target, "Directory", Id="ProgramFiles64Folder", Name="PFiles")
    install = node(program_files, "Directory", Id="INSTALLDIR", Name=identity["productName"])
    programs = node(target, "Directory", Id="ProgramMenuFolder")
    menu = node(programs, "Directory", Id="ApplicationProgramsFolder", Name=identity["productName"])
    components = []
    for index, name in enumerate(files):
        is_program = name == identity["executable"]
        component_id = "Path" if is_program else f"Payload{index}"
        key = identity["bundleIdentifier"] if is_program else f"TP.GPUI.payload/{name.lower()}"
        component = node(install, "Component", Id=component_id,
                         Guid=str(uuid.uuid5(TAURI_COMPONENT_NAMESPACE, key)), Win64="yes")
        node(component, "File", Id=component_id, Source=str(stage / name), KeyPath="yes", Checksum="yes")
        components.append(component_id)
    registry = node(install, "Component", Id="RegistryEntries", Guid="*")
    node(registry, "RegistryValue", Root="HKCU", Key=identity["installLocationRegistryKey"],
         Name="InstallDir", Type="string", Value="[INSTALLDIR]", KeyPath="yes")
    components.append("RegistryEntries")
    for directory, component_id, shortcut_id, registry_name in (
        (desktop, "ApplicationShortcutDesktop", "ApplicationDesktopShortcut", "Desktop Shortcut"),
        (menu, "ApplicationShortcut", "ApplicationStartMenuShortcut", "Start Menu Shortcut"),
    ):
        component = node(directory, "Component", Id=component_id, Guid="*")
        shortcut = node(component, "Shortcut", Id=shortcut_id, Name=identity["productName"],
                        Target="[!Path]", WorkingDirectory="INSTALLDIR", Icon="ProductIcon")
        if directory is menu:
            node(shortcut, "ShortcutProperty", Key="System.AppUserModel.ID", Value=identity["bundleIdentifier"])
        node(component, "RemoveFolder", Id=directory.attrib["Id"], On="uninstall")
        node(component, "RegistryValue", Root="HKCU", Key=identity["installLocationRegistryKey"],
             Name=registry_name, Type="integer", Value="1", KeyPath="yes")
        components.append(component_id)
    # This shortcut lives in machine-wide INSTALLDIR, unlike the user registry
    # entries used to remember the original install location. Its key path must
    # have the same scope (ICE57); the new HKLM key also gives WiX a distinct
    # auto-generated component GUID from the legacy HKCU component.
    uninstall = node(install, "Component", Id="CMP_UninstallShortcut", Guid="*", Win64="yes")
    node(uninstall, "Shortcut", Id="UninstallShortcut", Name="Uninstall TP",
         Target="[System64Folder]msiexec.exe", Arguments="/x [ProductCode]")
    node(uninstall, "RemoveFolder", Id="INSTALLDIR", On="uninstall")
    node(uninstall, "RegistryValue", Root="HKLM", Key=identity["installLocationRegistryKey"],
         Name="Uninstaller Shortcut", Type="integer", Value="1", KeyPath="yes")
    components.append("CMP_UninstallShortcut")
    feature = node(product, "Feature", Id="MainProgram", Title="Application", Level="1",
                   ConfigurableDirectory="INSTALLDIR", AllowAdvertise="no", Absent="disallow")
    for component_id in components:
        node(feature, "ComponentRef", Id=component_id)
    ET.indent(wix)
    ET.ElementTree(wix).write(output / "gpui.wxs", encoding="utf-8", xml_declaration=True)


def build(stage, output, tools):
    identity = native_identity()
    stage, output, tools = stage.resolve(), output.resolve(), tools.resolve()
    files = validate_stage(stage, identity)
    if identity["msiLanguages"] != ["zh-CN"]:
        raise ValueError("Review MSI localization before changing languages")
    verify_tools(tools)
    output.mkdir(parents=True, exist_ok=False)
    author(stage, output, identity, files)
    # Reuse the project's original Chinese wording.
    locale = output / "zh-CN.wxl"
    locale.write_bytes((ROOT / "packaging/windows/installer/zh-CN.wxl").read_bytes())
    obj = output / "gpui.wixobj"
    result = output / f"TP_{identity['version']}_x64_zh-CN.msi"
    subprocess.run([str(tools / "candle.exe"), "-nologo", "-arch", "x64", "-out", str(obj), str(output / "gpui.wxs")], check=True)
    subprocess.run([str(tools / "light.exe"), "-nologo", "-ext", str(tools / "WixUIExtension.dll"),
                    "-cultures:zh-cn", "-loc", str(locale), "-out", str(result), str(obj)], check=True)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--wix-tools", type=Path, required=True)
    args = parser.parse_args()
    print(f"Created unsigned MSI: {build(args.stage, args.output, args.wix_tools)}")
