import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import pos from "./locales/en/pos.json";
import posAr from "./locales/ar/pos.json";

i18n.use(initReactI18next).init({
  resources: {
    en: { pos },
    ar: { pos: posAr },
  },
  lng: "en",
  fallbackLng: "en",
  ns: ["pos"],
  defaultNS: "pos",
  interpolation: { escapeValue: false },
  returnObjects: false,
});

export default i18n;
