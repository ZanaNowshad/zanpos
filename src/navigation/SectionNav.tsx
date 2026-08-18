/**
 * Level-2 contextual navigation.
 *
 * Renders the sections of the ACTIVE domain only. It replaces both
 * OfficeAISecondaryNav instances (the "Operations" and "Control" sidebars),
 * which were global menus rendered in a secondary position — the same set of
 * links appeared regardless of which domain the user was in.
 *
 * Renders nothing when the active domain has fewer than two visible sections,
 * so a one-item sidebar is structurally impossible.
 */
import type { OfficeTab } from "../officeai/officeAiTypes";
import type { NavSection } from "./config";
import { TAB_ICON } from "../officeai/nav";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator, type OfficeAiStringKey } from "../i18n/officeAiStrings";
import "./navigation.css";

interface Props {
  domainLabelKey: OfficeAiStringKey;
  sections: NavSection[];
  activeTab: OfficeTab;
  onSelect: (tab: OfficeTab) => void;
}

export default function SectionNav({ domainLabelKey, sections, activeTab, onSelect }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);

  if (sections.length < 2) return null;

  return (
    /* The domain name is already stated by the active rail item and by the
       breadcrumb; repeating it as a heading here made it appear three times
       on one screen. The accessible name stays on the <nav>. */
    <nav className="zp-section-nav" aria-label={t(domainLabelKey)}>
      <ul>
        {sections.map(section => {
          const active = activeTab === section.id;
          return (
            <li key={section.id}>
              <button
                type="button"
                className={`zp-section-nav-item${active ? " is-active" : ""}`}
                onClick={() => onSelect(section.id)}
                aria-current={active ? "page" : undefined}
              >
                <span className="zp-section-nav-icon" aria-hidden="true">{TAB_ICON[section.id]}</span>
                <span className="zp-section-nav-text">{t(section.labelKey)}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
