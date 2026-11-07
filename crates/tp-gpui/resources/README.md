# UI reference data

`translations.json` and `themes.json` define the reviewed translation and theme
values used by the native UI checks.

- Translations map Chinese literals to their reviewed English counterparts. The
  checker also reads `scripts/gpui-translations.json` for native-only additions.
- Theme entries contain ten hex colors in this order: accent, background,
  surface, control, selected background, selected text, text, muted text, border,
  button text. The native palette test compares every role for all eleven themes.

Change these baselines only when intentionally changing the corresponding UI,
not merely to make a failed comparison pass.
