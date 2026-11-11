# TP layout dependency security patch

This directory contains the crates.io `taffy` 0.9.0 source distribution,
upstream commit `599b1b7d7410d97e13bafc4c01fbe6d7a9cf060a` (see
`.cargo_vcs_info.json`). The upstream MIT license is included as `LICENSE.md`,
copied from TP's existing, provenance-checked license supplement. Registry cache
markers and the crate's unused lockfile are omitted.

Changes made by TP on 2026-11-11:

- `Cargo.toml`: update `grid` from 0.18.0 to 1.0.1, the first release fixing
  GHSA-38c5-483c-4qqp (integer overflow in `Grid::expand_rows`). No layout source
  changes are required. `Cargo.toml.orig` is retained unchanged for comparison.

GPUI 0.2.2 requires exactly taffy 0.9.0, whose published dependency constraint
does not accept the fixed grid release. TP's root manifest patches taffy to
this directory to preserve the existing GPUI API while using the fixed grid.
Remove this patch when the selected GPUI release supports a taffy release
that depends on a fixed grid version.

Advisory: https://github.com/becheran/grid/security/advisories/GHSA-38c5-483c-4qqp
