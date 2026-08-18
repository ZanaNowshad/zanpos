import type { ReactNode } from "react";
import { useLanguage } from "../../hooks/useLanguage";
import { commandTranslator, type CommandStringKey } from "../../i18n/commandStrings";
import type { OfficeTab } from "../../officeai/officeAiTypes";
import { TAB_ICON } from "../../officeai/nav";

interface SubPage {
  id: OfficeTab;
  labelKey: string;
  icon?: ReactNode;
}

interface Props {
  activeTab: OfficeTab;
  subPages: SubPage[];
  onNavigate: (tab: OfficeTab) => void;
  children: ReactNode;
}

/**
 * Domain-level shell for Catalogue, Reports, Customers, and other multi-page
 * domains. Provides a horizontal sub-navigation bar above the content area.
 */
export default function DomainShell({ activeTab, subPages, onNavigate, children }: Props) {
  const { language } = useLanguage();
  const t = commandTranslator(language);

  return (
    <div className="oa-space-layout">
      <div className="oa-subnav-line" role="tablist" aria-label="Domain sections">
        {subPages.map((page) => (
          <button
            key={page.id}
            role="tab"
            aria-selected={activeTab === page.id}
            className={`oa-subnav-tab${activeTab === page.id ? " active" : ""}`}
            onClick={() => onNavigate(page.id)}
          >
            {page.icon ?? TAB_ICON[page.id] ?? null}
            <span>{t(page.labelKey as CommandStringKey)}</span>
          </button>
        ))}
      </div>
      <div className="oa-space-content oa-embedded-tab oa-embedded-tab-flat">
        {children}
      </div>
    </div>
  );
}
