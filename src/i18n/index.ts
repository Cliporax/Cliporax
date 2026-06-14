import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from '../locales/en.json';
import zh from '../locales/zh.json';

const resources = {
  en: { translation: en },
  zh: { translation: zh },
};

// Detect system language
const getSystemLanguage = (): string => {
  if (typeof navigator === 'undefined') return 'en';
  const browserLang = navigator.language;
  if (browserLang.startsWith('zh')) return 'zh';
  return 'en';
};

// Get saved language or fallback to system language
const getInitialLanguage = (): string => {
  try {
    if (typeof window !== 'undefined' && window.localStorage && typeof window.localStorage.getItem === 'function') {
      const saved = window.localStorage.getItem('cliporax-language');
      if (saved && (saved === 'en' || saved === 'zh')) return saved;
    }
  } catch {}
  return getSystemLanguage();
};

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: getInitialLanguage(),
    fallbackLng: 'en',
    interpolation: {
      escapeValue: false, // React already safes from xss
    },
  });

// Save language preference when changed
i18n.on('languageChanged', (lng) => {
  try {
    if (typeof window !== 'undefined' && window.localStorage && typeof window.localStorage.setItem === 'function') {
      window.localStorage.setItem('cliporax-language', lng);
    }
  } catch {}
});

export default i18n;
