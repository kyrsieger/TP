import { useSyncExternalStore } from "react";
import { getLocale, subscribeLanguage } from "../i18n";
export function useLanguage() { return useSyncExternalStore(subscribeLanguage, getLocale, getLocale); }
