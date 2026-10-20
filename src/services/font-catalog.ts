import type { FontFamily } from "../domain/fonts";
import { call } from "./desktop";

let fonts: FontFamily[] | null = null;
let pending: Promise<FontFamily[]> | null = null;

/** Keep the last successful catalog across settings unmounts, including idle release. */
export function getCachedSystemFonts() { return fonts; }

export function loadSystemFonts(refresh = false): Promise<FontFamily[]> {
  if (pending) return pending;
  if (!refresh && fonts !== null) return Promise.resolve(fonts);
  pending = call<FontFamily[]>("get_system_fonts").then(groups => {
    fonts = [...groups].sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
    return fonts;
  }).finally(() => { pending = null; });
  return pending;
}
