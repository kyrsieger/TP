export type FontStyle = { name: string; family: string; weight: number; stretch: number; style: "normal" | "italic" | "oblique"; aliases?: string[] };
export type FontFamily = { name: string; aliases: string[]; styles: FontStyle[] };
export type FontSelection = { family: string; style: FontStyle; size?: number };
export const regularFontStyle: FontStyle = { name: "Regular", family: "", weight: 400, stretch: 100, style: "normal" };
export const fontStyleKey = (style: FontStyle) => JSON.stringify([style.name, style.family, style.weight, style.stretch, style.style]);
export function defaultFontStyle(family: FontFamily): FontStyle {
  return family.styles.find(style => style.name === "Regular") ?? [...family.styles].sort((a, b) => {
    const score = (style: FontStyle) => Math.abs(style.weight - 400) + Math.abs(style.stretch - 100) * 10 + (style.style === "normal" ? 0 : 1000);
    return score(a) - score(b);
  })[0] ?? regularFontStyle;
}
export function resolveFontSelection(fonts: FontFamily[], family: string, style: FontStyle): FontSelection {
  const group = fonts.find(font => font.name === family) ?? fonts.find(font => font.aliases.includes(family));
  if (!group) return { family, style };
  const exact = group.styles.find(candidate => fontStyleKey(candidate) === fontStyleKey(style));
  const legacy = group.name !== family ? group.styles.find(candidate => candidate.family === family || candidate.aliases?.includes(family)) : undefined;
  return { family: group.name, style: exact ?? legacy ?? defaultFontStyle(group) };
}
