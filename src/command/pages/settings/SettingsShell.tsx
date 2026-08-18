import { useMemo, useState, type ReactNode } from "react";
import { Search } from "lucide-react";
import { useLanguage } from "../../../hooks/useLanguage";
import { commandTranslator, type CommandStringKey } from "../../../i18n/commandStrings";
import {
  groupsForRole, searchGroups, type SettingsGroup, type SettingsGroupId,
} from "./settingsConfig";
import "./settings.css";

/** Kept as the exported name so existing callers do not need to change. */
export type SettingsSection = SettingsGroupId;

interface Props {
  activeSection: SettingsGroupId;
  roleName: string;
  onSelectSection: (section: SettingsGroupId) => void;
  children: ReactNode;
}

/**
 * Settings workspace: one contextual rail, one work area.
 *
 * The rail replaces the previous arrangement where Integrations and System each
 * stacked several full feature panels into a single scroll. Groups are declared
 * once in settingsConfig and filtered by role there, so the rail and the
 * deep-link guard cannot disagree about who may see what.
 */
export default function SettingsShell({
  activeSection, roleName, onSelectSection, children,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => commandTranslator(language), [language]);
  const label = useMemo(() => (k: string) => t(k as CommandStringKey), [t]);
  const [query, setQuery] = useState("");

  const all = useMemo(() => groupsForRole(roleName), [roleName]);
  const matches = useMemo(
    () => searchGroups(query, roleName, label),
    [query, roleName, label],
  );

  const ordinary = matches.filter(g => !g.danger);
  const dangerous = matches.filter(g => g.danger);

  const renderGroup = (g: SettingsGroup) => {
    const active = g.id === activeSection;
    return (
      <li key={g.id}>
        <button
          type="button"
          className={`zp-set-nav-item${active ? " is-on" : ""}${g.danger ? " is-danger" : ""}`}
          aria-current={active ? "page" : undefined}
          onClick={() => onSelectSection(g.id)}
        >
          <span className="zp-set-nav-icon" aria-hidden="true">{g.icon}</span>
          <span className="zp-set-nav-text">
            <span className="zp-set-nav-label">{label(g.labelKey)}</span>
            <span className="zp-set-nav-desc">{label(g.descriptionKey)}</span>
          </span>
        </button>
      </li>
    );
  };

  return (
    <div className="zp-settings">
      <nav className="zp-set-nav" aria-label={t("settings")}>
        <div className="zp-set-search">
          <Search size={14} aria-hidden="true" />
          <input
            type="search"
            value={query}
            placeholder={t("searchSettings")}
            aria-label={t("searchSettings")}
            onChange={e => setQuery(e.target.value)}
          />
        </div>

        {matches.length === 0 ? (
          <p className="zp-set-nav-empty">{t("noSettingsMatch")}</p>
        ) : (
          <>
            <ul>{ordinary.map(renderGroup)}</ul>
            {/* Irreversible territory is separated, never mixed into the list. */}
            {dangerous.length > 0 && (
              <>
                <div className="zp-set-nav-sep" role="presentation" />
                <ul>{dangerous.map(renderGroup)}</ul>
              </>
            )}
          </>
        )}

        {query && matches.length > 0 && matches.length < all.length && (
          <p className="zp-set-nav-hint">{matches.length} / {all.length}</p>
        )}
      </nav>

      <div className="zp-set-content">{children}</div>
    </div>
  );
}
