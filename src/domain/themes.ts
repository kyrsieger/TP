import { deltaE2000, hexToLab } from "./color-distance";

export type ThemeId = "indigo" | "forest-mist" | "forest-night" | "oat" | "graphite" | "clay" | "olive" | "ice" | "berry" | "violet" | "charcoal";
export type Theme = {
  id: ThemeId; name: string; dark: boolean;
  accent: string; bg: string; surface: string; control: string; soft: string;
  selected: string; text: string; muted: string; line: string; buttonText: string;
};
const theme = (id: ThemeId, name: string, dark: boolean, colors: string[]): Theme => {
  const [accent, bg, surface, control, soft, selected, text, muted, line, buttonText] = colors;
  return { id, name, dark, accent, bg, surface, control, soft, selected, text, muted, line, buttonText };
};
export const themes: readonly Theme[] = [
  theme("indigo", "靛蓝", false, ["#4F46E5", "#FFFFFF", "#FFFFFF", "#F3F4F8", "#EEF0FF", "#312E81", "#171C28", "#5B6472", "#EEF0F5", "#FFFFFF"]),
  theme("forest-mist", "墨绿", false, ["#3D755D", "#F4F7F4", "#FBFCFA", "#E9EFEA", "#DFEBE2", "#28513D", "#22332A", "#5A6C61", "#D8E2DA", "#FFFFFF"]),
  theme("forest-night", "森林", true, ["#93C6AB", "#202C26", "#29382F", "#324338", "#365B45", "#E1F1E6", "#E8EFE9", "#BECCBF", "#445B4B", "#183426"]),
  theme("oat", "燕麦", false, ["#8F622D", "#F8F5EF", "#FDFBF7", "#EFE9DF", "#EFE2CE", "#70491C", "#382F23", "#6F6352", "#E5DDCF", "#FFFFFF"]),
  theme("graphite", "石墨", true, ["#A6BFF0", "#22262D", "#2A3039", "#323A45", "#354764", "#DCE8FF", "#E7EBF2", "#ABB6C6", "#424C5B", "#1C2A40"]),
  theme("clay", "陶土", false, ["#A35D43", "#F8F0E8", "#FCF7F1", "#F0E4D8", "#F0DCCF", "#7C412E", "#3D2C25", "#745E53", "#E7D7CC", "#FFFFFF"]),
  theme("olive", "橄榄", false, ["#70733F", "#F4F3E8", "#FAFAF2", "#E9E9D9", "#E3E5CD", "#50552B", "#323629", "#626753", "#DADCC8", "#FFFFFF"]),
  theme("ice", "靛青", false, ["#3C6881", "#EEF4F7", "#F8FCFD", "#E0EAF0", "#D3E6F0", "#274C63", "#233A46", "#4D6874", "#CDDDE5", "#FFFFFF"]),
  theme("berry", "莓灰", false, ["#8A5670", "#F7F0F4", "#FCF8FA", "#EEE2E9", "#EAD6E1", "#6C3B53", "#3F2C37", "#725868", "#E2CFDA", "#FFFFFF"]),
  theme("violet", "暮紫", true, ["#C5B2E3", "#292532", "#342F40", "#413A50", "#514366", "#EFE5FF", "#ECE6F4", "#C0B4CF", "#574B64", "#332840"]),
  theme("charcoal", "炭黑", true, ["#E2B77A", "#282522", "#322E28", "#3D372E", "#584632", "#F7DFC0", "#EFE9DF", "#C2B8A9", "#544B3E", "#342719"]),
];
export const defaultTheme: ThemeId = "indigo";
export function getTheme(id: unknown): Theme { return themes.find(theme => theme.id === id) ?? themes[0]; }
/** Same order as the seven swatches in the approved review. */
export function themeSwatches(theme: Theme): string[] {
  return [theme.accent, theme.bg, theme.surface, theme.control, theme.soft, theme.text, theme.muted];
}
export function themeDistance(a: Theme, b: Theme): number {
  const colors = themeSwatches(b);
  // Accent carries 1/3 of the distance; the six supporting colors share the remainder.
  return themeSwatches(a).reduce((sum, color, i) => sum + (i === 0 ? 3 : 1) * deltaE2000(hexToLab(color), hexToLab(colors[i])), 0) / 9;
}
export function sortThemes(input: readonly Theme[] = themes): Theme[] {
  const remaining = [...input], sorted: Theme[] = [];
  const first = remaining.findIndex(theme => theme.id === "forest-mist");
  if (first >= 0) sorted.push(...remaining.splice(first, 1));
  for (const dark of [false, true]) {
    while (remaining.some(theme => theme.dark === dark)) {
      const previous = sorted[sorted.length - 1] ?? getTheme("forest-mist");
      const next = remaining.filter(theme => theme.dark === dark).sort((a, b) => themeDistance(previous, a) - themeDistance(previous, b) || a.id.localeCompare(b.id))[0];
      sorted.push(next); remaining.splice(remaining.indexOf(next), 1);
    }
  }
  return sorted;
}
export const sortedThemes = sortThemes();
