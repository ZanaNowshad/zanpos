import type { OfficeAiStringKey, OfficeAiTranslator } from "../i18n/officeAiStrings";
import type { OfficeTab } from "./officeAiTypes";
import {
  CONTROL_GROUPS,
  CONTROL_TABS,
  OPERATION_GROUPS,
  OPERATION_TABS,
  type OfficeNavSection,
  type OfficeSubNavGroup,
  type OfficeSubTab,
} from "./nav";

const LABEL_KEYS: Partial<Record<OfficeTab, OfficeAiStringKey>> = {
  overview: "home",
  assistant: "askAi",
  operations: "operations",
  products: "products",
  categories: "categories",
  inventory: "inventory",
  purchasing: "purchasing",
  reports: "reports",
  cashier: "cashiers",
  eod: "endOfDay",
  deliveries: "deliveries",
  customers: "customers",
  users: "users",
  actions: "actionReview",
  workflows: "inbox",
  health: "health",
  conflicts: "conflictInbox",
  insights: "insights",
  loyalty: "loyalty",
  settings: "settings",
  audit: "audit",
  devices: "devices",
};

const DESCRIPTION_KEYS: Partial<Record<OfficeTab, OfficeAiStringKey>> = {
  overview: "whatNeedsAttention",
  assistant: "askInspectAct",
  operations: "catalogSalesPeople",
  products: "catalogPricing",
  categories: "productGroups",
  inventory: "stockLevels",
  purchasing: "suppliersPos",
  reports: "salesRefunds",
  cashier: "cashierReports",
  eod: "cashupWorkflow",
  deliveries: "deliveryQueue",
  customers: "customerRecords",
  users: "staffAccess",
  actions: "approveAiChanges",
  workflows: "inboxDescription",
  health: "healthDescription",
  conflicts: "conflictDescription",
  insights: "insightDescription",
  loyalty: "loyaltyDescription",
  settings: "appConfiguration",
  audit: "auditLog",
  devices: "deviceManagement",
};

const GROUP_KEYS: Record<string, OfficeAiStringKey> = {
  catalog: "catalog",
  sales: "sales",
  people: "people",
  review: "review",
  inbox: "inbox",
  system: "system",
  growth: "growth",
  settings: "settings",
};

const SECTION_KEYS: Record<string, OfficeAiStringKey> = {
  command: "command",
  operations: "run",
  growth: "growth",
  system: "system",
};

export function localizedOfficeTab(tab: OfficeSubTab, t: OfficeAiTranslator): OfficeSubTab {
  const labelKey = LABEL_KEYS[tab.id];
  const descriptionKey = DESCRIPTION_KEYS[tab.id];
  return {
    ...tab,
    label: labelKey ? t(labelKey) : tab.label,
    description: descriptionKey ? t(descriptionKey) : tab.description,
  };
}

export function localizedOfficeTabs(
  tabs: OfficeSubTab[],
  t: OfficeAiTranslator,
): OfficeSubTab[] {
  return tabs.map(tab => localizedOfficeTab(tab, t));
}

export function localizedOfficeGroups(
  groups: OfficeSubNavGroup[],
  t: OfficeAiTranslator,
): OfficeSubNavGroup[] {
  return groups.map(group => ({
    ...group,
    label: t(GROUP_KEYS[group.id]),
  }));
}

export function localizedOfficeNavSections(
  sections: OfficeNavSection[],
  t: OfficeAiTranslator,
): OfficeNavSection[] {
  return sections.map(section => ({
    ...section,
    label: SECTION_KEYS[section.id] ? t(SECTION_KEYS[section.id]) : section.label,
    items: section.items.map(item => {
      const labelKey = LABEL_KEYS[item.id];
      const descriptionKey = DESCRIPTION_KEYS[item.id];
      return {
        ...item,
        label: labelKey ? t(labelKey) : item.label,
        description: descriptionKey ? t(descriptionKey) : item.description,
      };
    }),
  }));
}

export const operationTabs = (t: OfficeAiTranslator) => localizedOfficeTabs(OPERATION_TABS, t);
export const controlTabs = (t: OfficeAiTranslator) => localizedOfficeTabs(CONTROL_TABS, t);
export const operationGroups = (t: OfficeAiTranslator) => localizedOfficeGroups(OPERATION_GROUPS, t);
export const controlGroups = (t: OfficeAiTranslator) => localizedOfficeGroups(CONTROL_GROUPS, t);
