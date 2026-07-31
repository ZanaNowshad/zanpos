import { MessageSquare, RefreshCw, Search, X } from "lucide-react";
import type { ReactNode } from "react";
import type { OfficePrimarySpace, OfficeTab } from "./officeAiTypes";
import { TAB_ICON } from "./nav";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";

export interface OfficePulseItem {
  id: string;
  label: string;
  level: "ok" | "warning" | "critical";
  icon: ReactNode;
}

interface Props {
  activeTab: OfficeTab;
  activeSpace: OfficePrimarySpace;
  title: string;
  subtitle: string;
  dockOpen: boolean;
  onToggleDock: () => void;
  onOpenCommand?: () => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  pulseItems?: OfficePulseItem[];
  children: ReactNode;
}

export default function OfficeAIShell({
  activeTab,
  activeSpace,
  title,
  subtitle,
  dockOpen,
  onToggleDock,
  onOpenCommand,
  onRefresh,
  refreshing,
  pulseItems = [],
  children,
}: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <main className="oa-main" data-space={activeSpace}>
      <header className="oa-topbar">
        <div className="oa-title-block">
          <span className="oa-title-icon">{TAB_ICON[activeTab]}</span>
          <div>
            <h1 className="oa-title">{title}</h1>
            <p className="oa-subtitle">{subtitle}</p>
          </div>
        </div>
        <div className="oa-topbar-actions">
          {onOpenCommand && (
            <button className="oa-command-trigger" onClick={onOpenCommand} title={t("searchZanAiShortcut")}>
              <Search size={15} />
              <span>{t("searchZanAi")}</span>
              <kbd>Ctrl K</kbd>
            </button>
          )}
          {onRefresh && (
            <button className="oa-tool-btn" onClick={onRefresh} disabled={refreshing} title={t("refreshWorkspace")}>
              <RefreshCw size={15} className={refreshing ? "oa-spin" : ""} />
              <span>{t(refreshing ? "refreshing" : "refresh")}</span>
            </button>
          )}
          <button className="oa-tool-btn" onClick={onToggleDock} title={t(dockOpen ? "closeCopilot" : "openCopilot")}>
            {dockOpen ? <X size={15} /> : <MessageSquare size={15} />}
            <span>{t(dockOpen ? "hideCopilot" : "copilot")}</span>
          </button>
        </div>
      </header>

      {pulseItems.length > 0 && (
        <div className="oa-pulse" aria-label={t("activeWarnings")}>
          {pulseItems.map(item => (
            <span key={item.id} className={`oa-pulse-chip oa-pulse-${item.level}`} title={item.label}>
              {item.icon}
              <span>{item.label}</span>
            </span>
          ))}
        </div>
      )}

      <section className="oa-workspace">
        {children}
      </section>
    </main>
  );
}
