import { useMemo } from "react";
import type { OfficeTab } from "./officeAiTypes";
import type { OfficeNavSection } from "./nav";
import { IcoClose, IcoOpen, IcoStocktake, IcoSync, OPERATION_TAB_IDS, TAB_ICON } from "./nav";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import { localizedOfficeNavSections } from "./officeAiNavigation";

interface Props {
  sections: OfficeNavSection[];
  activeTab: OfficeTab;
  collapsed: boolean;
  storeName: string;
  initials: string;
  userName: string;
  roleLabel: string;
  canUseManagerTools: boolean;
  canOpenSyncQueue: boolean;
  onSelect: (tab: OfficeTab) => void;
  onToggleCollapsed: () => void;
  onStockTake: () => void;
  onSyncQueue: () => void;
  onBackToPOS: () => void;
}

export default function OfficeAIWorkspaceNav({
  sections,
  activeTab,
  collapsed,
  storeName,
  initials,
  userName,
  roleLabel,
  canUseManagerTools,
  canOpenSyncQueue,
  onSelect,
  onToggleCollapsed,
  onStockTake,
  onSyncQueue,
  onBackToPOS,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => officeAiTranslator(language), [language]);
  const localizedSections = useMemo(
    () => localizedOfficeNavSections(sections, t),
    [sections, t],
  );
  return (
    <aside className={`oa-nav${collapsed ? " oa-nav-collapsed" : ""}`}>
      <div className="oa-nav-brand">
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
        >
          {collapsed ? <IcoOpen className="icon-directional" size={17} /> : <IcoClose className="icon-directional" size={17} />}
        </button>
      </div>

      <nav className="oa-nav-scroll" aria-label={t("zanAiWorkspaces")}>
        {localizedSections.map(section => (
          <div key={section.id} className="oa-nav-section">
            {!collapsed && <div className="oa-nav-section-label">{section.label}</div>}
            {section.items.map(item => {
              const active = activeTab === item.id || (item.id === "operations" && OPERATION_TAB_IDS.includes(activeTab));
              return (
                <button
                  key={`${section.id}-${item.id}`}
                  className={`oa-nav-item${active ? " oa-nav-item-active" : ""}`}
                  onClick={() => onSelect(item.id)}
                  title={collapsed ? item.label : item.description ?? item.label}
                >
                  <span className="oa-nav-icon">{TAB_ICON[item.id]}</span>
                  {!collapsed && (
                    <span className="oa-nav-copy">
                      <span className="oa-nav-label">{item.label}</span>
                      {item.description && <span className="oa-nav-desc">{item.description}</span>}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="oa-nav-footer">
        {canUseManagerTools && (
          <div className="oa-nav-tools">
            <button className="oa-sidebar-tool" onClick={onStockTake} title={t("stockTake")}>
              <IcoStocktake size={15} />
              {!collapsed && <span>{t("stockTake")}</span>}
            </button>
            {canOpenSyncQueue && (
              <button className="oa-sidebar-tool" onClick={onSyncQueue} title={t("syncQueue")}>
                <IcoSync size={15} />
                {!collapsed && <span>{t("syncQueue")}</span>}
              </button>
            )}
          </div>
        )}

        {!collapsed && (
          <div className="oa-user-card">
            <div className="oa-user-avatar">{initials}</div>
            <div className="oa-user-meta">
              <div className="oa-user-name">{userName}</div>
              <div className="oa-user-role">{roleLabel}</div>
            </div>
          </div>
        )}

        <button className="oa-back-btn" onClick={onBackToPOS} title={t("backToPosShortcut")}>
          <IcoClose className="icon-directional" size={14} />
          {!collapsed && <span>{t("backToPos")}</span>}
        </button>
      </div>
    </aside>
  );
}
