/** Canonical names shared by shortcut capture and local keyboard matching. */
export function recordRenameShortcutFromKey(event: KeyboardEvent): string | null {
  if (event.isComposing || event.keyCode === 229 || event.getModifierState("AltGraph")) return null;
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  const modifiers = [event.ctrlKey && "Control", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter(Boolean);
  if (!/^F(?:[1-9]|1\d|2[0-4])$/.test(key) && !(modifiers.length && /^[A-Z0-9]$/.test(key))) return null;
  return [...modifiers, key].join("+");
}

export function recordRenameShortcutError(shortcut: string, panelShortcut: string): string | null {
  if (shortcut && (shortcut === panelShortcut || /^Control\+[1-9ADFGP]$/.test(shortcut))) return "此快捷键已用于其他操作，请更换按键。";
  return null;
}
