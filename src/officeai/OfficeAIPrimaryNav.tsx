import { Bot, Boxes, Gauge, LayoutDashboard } from "lucide-react";
import type { ReactNode } from "react";
import type { OfficePrimarySpace, OfficeTab } from "./officeAiTypes";
import { IcoClose, IcoOpen } from "./nav";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator, type OfficeAiStringKey } from "../i18n/officeAiStrings";

interface PrimaryNavEntry {
  space: OfficePrimarySpace;
  label: string;
  icon: ReactNode;
  defaultTab: OfficeTab;
}

const PRIMARY_ENTRIES: PrimaryNavEntry[] = [
  { space: "home", label: "Home", icon: <LayoutDashboard size={19} strokeWidth={1.7} />, defaultTab: "overview" },
  { space: "ask-ai", label: "Ask AI", icon: <Bot size={19} strokeWidth={1.7} />, defaultTab: "assistant" },
  { space: "operations", label: "Operations", icon: <Boxes size={19} strokeWidth={1.7} />, defaultTab: "operations" },
  { space: "control", label: "Control", icon: <Gauge size={19} strokeWidth={1.7} />, defaultTab: "actions" },
];

interface Props {
  activeSpace: OfficePrimarySpace;
  collapsed: boolean;
  storeName: string;
  entries?: PrimaryNavEntry[];
  onSelectSpace: (entry: PrimaryNavEntry) => void;
  onToggleCollapsed: () => void;
  onBackToPOS: () => void;
}

export default function OfficeAIPrimaryNav({
  activeSpace,
  collapsed,
  storeName,
  entries = PRIMARY_ENTRIES,
  onSelectSpace,
  onToggleCollapsed,
  onBackToPOS,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const entryKey: Record<OfficePrimarySpace, OfficeAiStringKey> = {
    home: "home",
    "ask-ai": "askAi",
    operations: "operations",
    control: "control",
  };
  return (
    <aside className={`oa-primary-nav${collapsed ? " oa-primary-nav-collapsed" : ""}`}>
      <div className="oa-primary-nav-brand">
        {!collapsed && (
          <div>
            <div className="oa-nav-wordmark">Office<span>AI</span></div>
            <div className="oa-nav-store">{storeName}</div>
          </div>
        )}
        <button
          className="oa-icon-btn"
          onClick={onToggleCollapsed}
          title={t(collapsed ? "expandNavigation" : "collapseNavigation")}
          aria-label={t(collapsed ? "expandNavigation" : "collapseNavigation")}
        >
          {collapsed ? <IcoOpen className="icon-directional" size={17} /> : <IcoClose className="icon-directional" size={17} />}
        </button>
      </div>

      <nav className="oa-primary-nav-items" aria-label={t("primarySpaces")}>
        {entries.map(entry => {
          const active = activeSpace === entry.space;
          return (
            <button
              key={entry.space}
              className={`oa-nav-item${active ? " oa-nav-item-active" : ""}`}
              onClick={() => onSelectSpace(entry)}
              title={collapsed ? t(entryKey[entry.space]) : undefined}
            >
              <span className="oa-nav-icon">{entry.icon}</span>
              {!collapsed && <span className="oa-nav-label">{t(entryKey[entry.space])}</span>}
            </button>
          );
        })}
      </nav>

      <div className="oa-nav-footer">
        <button className="oa-back-btn" onClick={onBackToPOS} title={t("backToPosShortcut")}>
          <IcoClose className="icon-directional" size={14} />
          {!collapsed && <span>{t("backToPos")}</span>}
        </button>
      </div>
    </aside>
  );
}

export { PRIMARY_ENTRIES };
export type { PrimaryNavEntry };
