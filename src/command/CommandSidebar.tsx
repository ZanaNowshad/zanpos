import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import type { ReactNode } from "react";
import { useLanguage } from "../hooks/useLanguage";
import { commandTranslator, type CommandStringKey } from "../i18n/commandStrings";

// ─── Rail (L1) ───────────────────────────────────────────────────────────────
// Domains are supplied by the caller from src/navigation/config — the single
// source of truth. This component no longer declares its own list; the former
// local ALL_DOMAINS was one of five competing navigation models.

export interface CommandDomain {
  id: string;
  labelKey: string;
  icon: ReactNode;
  defaultTab: string; // OfficeTab value to switch to
  managerOnly?: boolean;
  ownerOnly?: boolean;
}

interface Props {
  activeDomainId: string;
  collapsed: boolean;
  storeName: string;
  roleName: string;
  /** True when a contextual section nav is rendered for the active domain. */
  hasSectionNav?: boolean;
  domains?: CommandDomain[];
  onSelectDomain: (domain: CommandDomain) => void;
  onToggleCollapsed: () => void;
  onBackToPOS: () => void;
}

export default function CommandSidebar({
  activeDomainId,
  collapsed,
  storeName,
  hasSectionNav = false,
  domains,
  onSelectDomain,
  onToggleCollapsed,
  onBackToPOS,
}: Props) {
  const { language } = useLanguage();
  const t = commandTranslator(language);

  const visible = domains ?? [];

  return (
    <aside className={`oa-primary-nav${collapsed ? " oa-primary-nav-collapsed" : ""}`}>
      <div className="oa-primary-nav-brand">
        {!collapsed && (
          <div>
            <div className="oa-nav-wordmark">ZANPOS<span>Command</span></div>
            <div className="oa-nav-store">{storeName}</div>
          </div>
        )}
        <button
          className="oa-icon-btn"
          onClick={onToggleCollapsed}
          title={collapsed ? "Expand navigation" : "Collapse navigation"}
          aria-label={collapsed ? "Expand navigation" : "Collapse navigation"}
        >
          {collapsed
            ? <PanelLeftOpen className="icon-directional" size={17} />
            : <PanelLeftClose className="icon-directional" size={17} />}
        </button>
      </div>

      <nav className="oa-primary-nav-items" aria-label="Primary navigation">
        {visible.map((domain) => {
          const active = activeDomainId === domain.id;
          return (
            <button
              key={domain.id}
              className={`oa-nav-item${active ? " oa-nav-item-active" : ""}`}
              onClick={() => onSelectDomain(domain)}
              title={collapsed ? t(domain.labelKey as CommandStringKey) : undefined}
              /* The rail marks the current *domain*, not the current page —
                 the section nav (or the breadcrumb) owns "page". Exactly one
                 element in the document may claim aria-current="page". */
              aria-current={active ? (hasSectionNav ? "true" : "page") : undefined}
            >
              <span className="oa-nav-icon">{domain.icon}</span>
              {!collapsed && <span className="oa-nav-label">{t(domain.labelKey as CommandStringKey)}</span>}
            </button>
          );
        })}
      </nav>

      <div className="oa-nav-footer">
        <button className="oa-back-btn" onClick={onBackToPOS} title={t("backToPosShortcut")}>
          <PanelLeftClose className="icon-directional" size={14} />
          {!collapsed && <span>{t("backToPos")}</span>}
        </button>
      </div>
    </aside>
  );
}
