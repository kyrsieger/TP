# Native GPUI packaging resources

`icons/` owns the GPUI executable and installer icons (`icon.ico`, `icon.icns`)
and Windows/macOS tray images. The title bar continues to embed
`icons/tp-mark.svg`. `windows/installer/` owns the Chinese NSIS/WiX wording
and finish-page hook.

Native Rust and packaging scripts must use the native paths above. Changes to
icons require checking the executable, tray, title bar, and installer separately.

`windows/identity.json` freezes the existing product identity and upgrade keys.
The macOS assembler reads its product name and bundle identifier too. Versions
come from the root Cargo workspace. `windows/policy.json` records the explicit
Windows downgrade policy. `scripts/gpui_windows_identity.py` checks compatibility
with the frozen historical contract in `windows/compatibility.json`; native package
assembly does not load legacy application code or resources. This baseline is
independent of the active identity and policy and must not be updated merely to
silence a compatibility failure.

The NSIS upstream template and plugin retain their original names and license
requirements; their use does not embed the Tauri application or WebView runtime.
Unsigned test packages and CI artifacts are not approved releases. Actual
installation, upgrade, uninstall, macOS runtime, signing, and distribution review
remain separate acceptance work.
