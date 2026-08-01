import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import pos from "./locales/en/pos.json";
import backOffice from "./locales/en/backOffice.json";
import detail from "./locales/en/detail.json";
import modal from "./locales/en/modal.json";
import officeAi from "./locales/en/officeAi.json";
import officeAiTool from "./locales/en/officeAiTool.json";
import operations from "./locales/en/operations.json";

import posAr from "./locales/ar/pos.json";
import backOfficeAr from "./locales/ar/backOffice.json";
import detailAr from "./locales/ar/detail.json";
import modalAr from "./locales/ar/modal.json";
import officeAiAr from "./locales/ar/officeAi.json";
import officeAiToolAr from "./locales/ar/officeAiTool.json";
import operationsAr from "./locales/ar/operations.json";

const NS = ["pos", "backOffice", "detail", "modal", "officeAi", "officeAiTool", "operations"];

i18n.use(initReactI18next).init({
  resources: {
    en: { pos, backOffice, detail, modal, officeAi, officeAiTool, operations },
    ar: {
      pos: posAr,
      backOffice: backOfficeAr,
      detail: detailAr,
      modal: modalAr,
      officeAi: officeAiAr,
      officeAiTool: officeAiToolAr,
      operations: operationsAr,
    },
  },
  lng: "en",
  fallbackLng: "en",
  ns: NS,
  defaultNS: "pos",
  interpolation: { escapeValue: false },
  returnObjects: false,
});

export default i18n;
