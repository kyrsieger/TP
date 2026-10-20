import { pinyin } from "pinyin-pro";
import type { FontFamily } from "./fonts";

type FontSearchEntry = { family: string; terms: readonly string[] };
export type FontSearchIndex = readonly FontSearchEntry[];

const normalize = (value: string) => value.normalize("NFKC").toLowerCase().replace(/\s+/g, "");

/** Build once per font list; keystrokes only compare the prepared strings. */
export function buildFontSearchIndex(fonts: readonly FontFamily[], selectedFamily = ""): FontSearchIndex {
  const prepared = new Map<string, string[]>();
  const termsFor = (name: string) => {
    let terms = prepared.get(name);
    if (terms) return terms;
    terms = [normalize(name)];
    if (/\p{Script=Han}/u.test(name)) {
      terms.push(
        normalize(pinyin(name, { pattern: "first", toneType: "none", type: "array", nonZh: "consecutive" }).join("")),
        normalize(pinyin(name, { toneType: "none", type: "array", nonZh: "consecutive" }).join("")),
      );
    }
    prepared.set(name, terms);
    return terms;
  };
  const entries = new Map<string, FontSearchEntry>();
  entries.set("", { family: "", terms: termsFor("系统默认") });
  for (const font of fonts) {
    if (!font.name || entries.has(font.name)) continue;
    const names = [font.name, ...font.aliases];
    entries.set(font.name, { family: font.name, terms: [...new Set(names.flatMap(termsFor))] });
  }
  if (selectedFamily && !entries.has(selectedFamily)) entries.set(selectedFamily, { family: selectedFamily, terms: termsFor(selectedFamily) });
  return [...entries.values()];
}

export function searchFontFamilies(index: FontSearchIndex, query: string): string[] {
  const needle = normalize(query);
  return index.filter(entry => !needle || entry.terms.some(term => term.includes(needle))).map(entry => entry.family);
}
