# GPUI NSIS packaging

`scripts/package_gpui_windows_nsis.py` compiles an installer from a validated
Windows staging directory. It does not install or launch anything. The output
directory must not already exist.

```powershell
python scripts/package_gpui_windows_nsis.py --stage <stage> --output <new-output-directory> --compiler <makensis.exe> --plugins <additional-plugin-directory>
```

The stage uses the same identity/version/hash manifest as the MSI packager.
Supply NSIS 3.11 and the original installer's `nsis_tauri_utils.dll` v0.5.3 plugin.
The plugin is an installer build dependency, not a TP application runtime.
The builder checks the plugin bytes before executing the compiler, then checks
the compiler's distribution files and `/VERSION` output. `toolchain.json`
records SHA-256 hashes for 321 build files: the root compiler/configuration,
Bin, Contrib, Plugins, Include and Stubs. Missing or changed files stop the
build before the compiler executes. Documentation/examples and GUI-only tools
are excluded. Release tool provisioning remains to be completed.

These file hashes were measured from the upstream-pinned NSIS 3.11 ZIP after
matching its published SHA-1 `ef7ff767e5cbd9edd22add3a32c9b8f4500bb10d`.
The measured archive SHA-256 is
`c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1`;
the exact download URL is recorded in `toolchain.json`.

The [pinned upstream bundler](https://github.com/tauri-apps/tauri/blob/9452ddee5ebefd9b678a94ff003521379df6c9ae/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs)
specifies the plugin's release URL
`https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll`
and SHA-1 `75197fee3c6a814fe035788d1c34ead39349b860`.
After matching that upstream pin, the local plugin's SHA-256 was recorded as
`5ba143b5db4a87d32d6e7802e033330aae56cbceabe0d1e3ba41948385ad4709`.
Both hashes are required. The SHA-256 is locally measured, not an upstream
published checksum.

The upstream installer sources are copied byte-for-byte from Tauri CLI 2.11.5,
commit `9452ddee5ebefd9b678a94ff003521379df6c9ae`. See
[the original template](https://github.com/tauri-apps/tauri/blob/9452ddee5ebefd9b678a94ff003521379df6c9ae/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi).
`upstream/provenance.json` records paths and SHA-256 hashes; the author checks
these before rendering. The upstream MIT and Apache-2.0 license texts are
included alongside the source.

The adapter retains the original maintenance/upgrade pages, install scope,
shortcuts, uninstall choices, and the project's Chinese strings/hooks. It
removes the complete WebView installation section and supplies the GPUI
executable and staged resources. Template changes or new associations/protocols
require explicit adapter review; unknown live expressions fail rather than
silently disappearing. Generated scripts include a UTF-8 BOM for Chinese text.

Compilation and authoring tests do not prove install/upgrade/uninstall behavior.
Use the read-only archive verifier to check the compiled package's payload:

```powershell
python scripts/verify_gpui_windows_nsis.py --installer <setup.exe> --stage <stage> --seven-zip <7z.exe>
```

It checks the NSIS archive type and exact file set before extracting into a fresh
workspace `target` temporary directory. Application payload sizes/hashes must
match the validated stage. It rejects extra, missing, duplicate and unsafe paths.
The extracted utility plugin must also match the pinned v0.5.3 hashes.
The other four support files (System, nsDialogs, StartMenu and the wizard bitmap)
must match their original NSIS distribution hashes from `toolchain.json`.
The generated uninstaller has only an executable-header sanity check; compiled
uninstaller logic and installer registry/UI behavior require runtime tests. Neither
the setup executable nor the extracted uninstaller is executed.

The first local test package contains an old GPUI binary and test-only license
notices; do not distribute it. A release requires a fresh verified executable,
complete third-party notices, payload verification, and real installer tests.
