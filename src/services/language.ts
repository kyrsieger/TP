import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./desktop";
import { applyLanguage, refreshSystemLanguage, type LanguageState } from "../i18n";
export async function startLanguageSync(): Promise<UnlistenFn> {
  let revision = 0;
  const unlisten = await listen<LanguageState>("language-changed", event => { revision++; applyLanguage(event.payload); });
  try { const state = await call<LanguageState>("get_language"); if (revision === 0 && state) applyLanguage(state); } catch { if (revision === 0) refreshSystemLanguage(); }
  const refresh = () => { const token = ++revision; void call<LanguageState>("get_language").then(state => { if (token === revision) applyLanguage(state); }).catch(() => { if (token === revision) refreshSystemLanguage(); }); };
  window.addEventListener("languagechange", refresh);
  return () => { unlisten(); window.removeEventListener("languagechange", refresh); };
}
