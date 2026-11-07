# GPUI dependency license evidence

These files supply license texts and source attribution for the native application's
third-party notices. Keep them aligned with the resolved Cargo dependencies.

- `supplements.json` maps exact package versions and published repository revisions
  to upstream license files, source URLs and SHA-256 hashes.
- `standard-terms.json` connects official license terms to verified declarations
  in the published package source and manifest.
- `published-terms.json` identifies evidence from published crate archives. The
  audit verifies their Cargo.lock checksums and package identities, and retains
  source and attribution alongside the applicable terms.

`scripts/audit-gpui-licenses.py` validates this evidence against locked dependencies
and produces an audit JSON and ZIP under `target/`. It rejects mismatched versions,
revisions and file hashes. `scripts/build_gpui_notices.py` uses the audit to generate
recipient-facing notices, including the current icon and installer licenses.

When changing dependencies, review the mappings, license alternatives, attribution
and source-availability requirements. Generated reports and temporary research
files are build artifacts; they do not belong in the source tree.
