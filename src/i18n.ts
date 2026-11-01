import { english } from "./translations";
export type LanguagePreference = "system" | "zh-CN" | "en";
export type Locale = "zh-CN" | "en";
export type LanguageState = { preference: LanguagePreference; resolved: Locale; system?: Locale };
export function resolveLanguage(preference: LanguagePreference, systemLanguage = typeof navigator === "undefined" ? "zh-CN" : navigator.language): Locale {
  return preference === "system" ? /^zh(?:-|$)/i.test(systemLanguage) ? "zh-CN" : "en" : preference;
}
let preference: LanguagePreference = "system";
let locale: Locale = resolveLanguage(preference);
let systemLocale: Locale = locale;
const listeners = new Set<() => void>();
export const getLocale = () => locale;
export const getSystemLocale = () => systemLocale;
export const subscribeLanguage = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export function applyLanguage(state: LanguageState) {
  preference = state.preference;
  const next = state.resolved;
  const nextSystem = state.system ?? (state.preference === "system" ? next : systemLocale);
  if (typeof document !== "undefined") document.documentElement.lang = next;
  if (locale === next && systemLocale === nextSystem) return;
  locale = next; systemLocale = nextSystem; listeners.forEach(listener => listener());
}
export function refreshSystemLanguage() { if (preference === "system") applyLanguage({ preference, resolved: resolveLanguage(preference) }); }
export function t(key: keyof typeof english, ...values: (string | number)[]): string {
  const message = locale === "en" ? english[key] : key;
  return message.replace(/\{(\d+)\}/g, (_, index: string) => String(values[Number(index)] ?? ""));
}
