import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./desktop";
import { regularFontStyle, type FontSelection } from "../domain/fonts";

const fallbackFonts = '"Segoe UI","Microsoft YaHei UI","Microsoft YaHei","PingFang SC",system-ui,sans-serif';
const listeners = new Set<() => void>();

export function onFontChange(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function applyFont(value: unknown) {
  const style = document.documentElement.style;
  const selection = typeof value === "string" ? { family: value, style: regularFontStyle, size: 14 } : value as FontSelection | undefined;
  const size = selection?.size;
  style.setProperty("--app-font-scale", String(typeof size === "number" && Number.isInteger(size) && size >= 10 && size <= 20 ? size / 13 : 14 / 13));
  const validName = (name: unknown): name is string => typeof name === "string" && [...name].length <= 256 && !/[\u0000-\u001f\u007f-\u009f]/.test(name);
  if (!selection || !validName(selection.family) || !selection.family) {
    style.removeProperty("--app-font");
    for (const property of ["weight", "style", "stretch"]) style.removeProperty(`--app-font-${property}`);
    listeners.forEach(listener => listener());
    return;
  }
  // Quote the whole family name so punctuation cannot introduce extra CSS values.
  const face = selection.style ?? regularFontStyle;
  const quote = (name: string) => `"${name.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  const families = [selection.family];
  if (face.family && validName(face.family) && face.family !== selection.family) families.unshift(face.family);
  style.setProperty("--app-font", `${families.map(quote).join(",")},${fallbackFonts}`);
  style.setProperty("--app-font-weight", String(Number.isInteger(face.weight) && face.weight >= 1 && face.weight <= 1000 ? face.weight : 400));
  style.setProperty("--app-font-style", ["normal", "italic", "oblique"].includes(face.style) ? face.style : "normal");
  style.setProperty("--app-font-stretch", `${Number.isInteger(face.stretch) && face.stretch >= 50 && face.stretch <= 200 ? face.stretch : 100}%`);
  listeners.forEach(listener => listener());
}

/** Listen first so a late startup read cannot undo a newer live selection. */
export async function startFontSync(): Promise<UnlistenFn> {
  let revision = 0;
  const unlisten = await listen<FontSelection | string>("font-changed", event => { revision++; applyFont(event.payload); });
  try { const font = await call<FontSelection>("get_font"); if (revision === 0) applyFont(font); }
  catch { if (revision === 0) applyFont(undefined); }
  return unlisten;
}
