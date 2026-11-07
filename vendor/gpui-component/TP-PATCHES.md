# TP input extension

This directory is the crates.io `gpui-component` 0.5.1 source distribution,
upstream commit `0f0ab35233212f8f3277028995caf0c41e13ee6c` (see
`.cargo_vcs_info.json`). Its Apache-2.0 license is retained in LICENSE-APACHE.
TP's root Cargo manifest patches this exact dependency to the local directory.

Changes made by TP on 2026-11-06:

- `src/input/state.rs`: opt-in `InputState::max_length_utf16`, applied to
  committed edits before updating text, cursor and undo history. Replacement
  capacity excludes the replaced range; the suffix is preserved. Programmatic
  set_value and undo/redo replay bypass the limit via the existing history flag.
  IME marked text is unrestricted; commit/unmark applies the limit.
- `src/input/utf16_limit.rs` and its module declaration: UTF-16 prefix helper
  with tests, without splitting surrogate pairs.
- IME preedit selections are converted relative to the new composition text
  and both endpoints are offset from the insertion start. The upstream code
  converted against the entire document and used the old replacement end for
  one endpoint, producing incorrect positions during composition updates.
  This correction applies to all inputs, independently of the length option.

Only TP's group editor enables this option (40 UTF-16 code units). Other
inputs retain their default unlimited behavior. The helper tests are included
in tp-gpui's test build so the ordinary workspace test command executes them.
IME behavior still requires real Windows and macOS acceptance testing.

The vendored crate is excluded from workspace membership. Its changed Rust
implementation files are checked separately by `python scripts/check.py` and the GPUI
CI job using `rustfmt --edition 2024 --check` on `src/input/state.rs`,
`src/input/utf16_limit.rs` and `src/scroll/scrollbar.rs`; workspace
`cargo fmt --all` alone is insufficient.

When updating this dependency, port or remove the extension explicitly,
compare upstream edits, retain license files, and rerun the input tests and
interactive paste/selection/undo/IME checks. Do not edit the Cargo registry cache.

## Thin scrollbar option

`src/scroll/scrollbar.rs` adds opt-in `Scrollbar::thin(width)` for TP's CSS-style narrow scrollbars. It keeps the upstream scroll/drag logic, but uses the requested width for track and thumb hitboxes, zero inset, half-width corner radius, and transparent track/border in all states. Existing callers keep their original appearance. TP uses this on the records list; other scrollable surfaces still require integration and visual verification.
