import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { getSettings } from "../lib/tauri";
import zhTW from "./zh-TW.json";
import en from "./en.json";

const LANGUAGE_STORAGE_KEY = "language";
const SUPPORTED_LANGUAGES = ["zh-TW", "en"] as const;
type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

function isSupportedLanguage(lang: string | null): lang is SupportedLanguage {
  return SUPPORTED_LANGUAGES.includes(lang as SupportedLanguage);
}

function normalizeLanguage(lang: string | null): SupportedLanguage | null {
  return lang === "zh" ? "zh-TW" : isSupportedLanguage(lang) ? lang : null;
}

function getStoredLanguage(): SupportedLanguage | null {
  const stored = localStorage.getItem(LANGUAGE_STORAGE_KEY);
  return normalizeLanguage(stored);
}

export const i18nReady = (async () => {
  const storedLanguage = getStoredLanguage();
  const savedLanguage = await getSettings("language").catch(() => null);
  const lng = normalizeLanguage(savedLanguage) || storedLanguage || "zh-TW";

  localStorage.setItem(LANGUAGE_STORAGE_KEY, lng);

  await i18n.use(initReactI18next).init({
    resources: {
      "zh-TW": { translation: zhTW },
      en: { translation: en },
    },
    lng,
    fallbackLng: "zh-TW",
    interpolation: { escapeValue: false },
  });
})();

export default i18n;
