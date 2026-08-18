/**
 * Location indicator: "Domain › Section".
 *
 * The previous shell had no breadcrumb on any screen, so a user landing on a
 * page had no way to tell which domain they were inside or how to get back to
 * it. Derived entirely from the canonical navigation config.
 */
import type { OfficeTab } from "../officeai/officeAiTypes";
import { breadcrumbForTab } from "./config";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import "./navigation.css";

interface Props {
  tab: OfficeTab;
  onNavigate: (tab: OfficeTab) => void;
}

export default function Breadcrumb({ tab, onNavigate }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const crumbs = breadcrumbForTab(tab);

  if (crumbs.length === 0) return null;

  return (
    <nav className="zp-breadcrumb" aria-label={t("navigateWithSearch")}>
      <ol>
        {crumbs.map((crumb, i) => {
          const isLast = i === crumbs.length - 1;
          const label = t(crumb.labelKey);
          return (
            <li key={`${crumb.labelKey}-${i}`}>
              {i > 0 && <span className="zp-breadcrumb-sep" aria-hidden="true">›</span>}
              {isLast || !crumb.tab ? (
                /* "location" not "page": the section nav already owns "page",
                   and a breadcrumb states position in the hierarchy. */
                <span className="zp-breadcrumb-current" aria-current="location">{label}</span>
              ) : (
                <button
                  type="button"
                  className="zp-breadcrumb-link"
                  onClick={() => crumb.tab && onNavigate(crumb.tab)}
                >
                  {label}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
