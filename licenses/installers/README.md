# Installer attribution inputs

The recipient notice generator includes these five full license texts in addition
to the native Cargo inventory. `manifest.json` records the source and SHA-256;
changed or missing text fails generation.

- NSIS 3.11 COPYING is copied byte-for-byte from the checksum-verified
  nsis-3.11.zip distribution, including the compression-module terms and LZMA
  exception. Its archive hash and internal member name are recorded.
- nsis-tauri-utils 0.5.3 license texts come from the tag's fixed commit
  13d9edd27b69310e108d6fbd49f90992f8a05390. This preserves the plugin's own
  attribution; it does not inventory all dependencies in the prebuilt DLL.
- The reused NSIS template's MIT/Apache texts refer to the already recorded
  Tauri CLI 2.11.5 source commit. Using this template does not bundle the
  Tauri application runtime.

Installer-plugin transitive attribution, runtime redistribution, and any source
availability requirements remain separate review items. This inventory is not
an assertion that distribution review is complete. The generated notice is a
cross-platform union; installer-only entries do not imply those components are
linked into the macOS executable.
