import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./desktop";
import { getTheme } from "../domain/themes";

export function applyTheme(id: unknown) {
  const t = getTheme(id), legacy = t.id === "indigo";
  const tokens: Record<string, string> = {
    "panel-bg": t.bg, surface: t.surface, "control-bg": t.control, "control-border": legacy ? "#e5e7ee" : t.line,
    accent: t.accent, "accent-soft": t.soft, "accent-hover": legacy ? "#4338ca" : `color-mix(in srgb, ${t.accent} 88%, ${t.text})`,
    "accent-soft-hover": legacy ? "#e1e4ff" : `color-mix(in srgb, ${t.soft} 85%, ${t.accent})`,
    "text-1": t.text, "text-2": t.muted, "text-3": legacy ? "#9aa2b1" : t.muted, "text-selected": t.selected,
    line: t.line, hover: legacy ? "#f6f7fb" : t.control, "group-soft": legacy ? "#e8edfa" : t.soft,
    "group-text": legacy ? "#5168ba" : t.selected, "group-idle": legacy ? "#6b7385" : t.muted,
    separator: legacy ? "rgba(23,28,40,.05)" : t.line, scrollbar: legacy ? "#d4d9e4" : t.line,
    disabled: legacy ? "#c9cedb" : t.muted, star: legacy ? "#f59e0b" : t.dark ? "#E5BC71" : "#986513",
    danger: legacy ? "#ef4444" : t.dark ? "#F09595" : "#AD3D3D",
    "danger-muted": legacy ? "#b35b5b" : t.dark ? "#F09595" : "#AD3D3D",
    "danger-bg": legacy ? "#c73737" : "#B83E3E", "danger-bg-hover": legacy ? "#ad2727" : "#9B3030",
    "on-accent": t.buttonText, "on-danger": "#FFFFFF", "toast-bg": legacy ? "#273149" : t.dark ? t.soft : t.selected,
    "toast-text": "#FFFFFF", link: legacy ? "#059669" : t.accent,
    "file-tag-bg": legacy ? "#f5f3ff" : t.soft, "file-tag-text": legacy ? "#7c3aed" : t.selected,
    "media-overlay": legacy ? "rgba(255,255,255,.9)" : `color-mix(in srgb, ${t.surface} 90%, transparent)`,
    "indicator-bg": legacy ? "rgba(255,255,255,.78)" : `color-mix(in srgb, ${t.surface} 85%, transparent)`,
    "indicator-dot": legacy ? "#b0bac7" : t.muted, "indicator-active": legacy ? "#8f9bab" : t.accent,
    warning: legacy ? "#a16207" : t.dark ? "#E5BC71" : "#986513",
  };
  for (const [name, value] of Object.entries(tokens)) document.documentElement.style.setProperty(`--${name}`, value);
  document.documentElement.dataset.theme = t.id;
  document.documentElement.style.colorScheme = t.dark ? "dark" : "light";
}

/** Register before fetching so a late startup response cannot undo a live theme change. */
export async function startThemeSync(): Promise<UnlistenFn> {
  let revision = 0;
  const unlisten = await listen<string>("theme-changed", event => { revision++; applyTheme(event.payload); });
  try { const theme = await call<string>("get_theme"); if (revision === 0) applyTheme(theme); }
  catch { if (revision === 0) applyTheme(undefined); }
  return unlisten;
}
