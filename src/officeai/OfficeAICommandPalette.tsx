import { Search, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import { TAB_ICON } from "./nav";
import type { OfficeTab } from "./officeAiTypes";

/** A palette row. Built by OfficeAIPage from the canonical navigation config. */
export interface PaletteItem {
  id: OfficeTab;
  label: string;
  description: string;
}

interface Props {
  items: PaletteItem[];
  onSelect: (tab: OfficeTab) => void;
  onDismiss: () => void;
}

export default function OfficeAICommandPalette({ items, onSelect, onDismiss }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return items;
    return items.filter(item => `${item.label} ${item.description}`.toLocaleLowerCase().includes(normalized));
  }, [items, query]);

  return (
  <div className="oa-modal-backdrop" onMouseDown={onDismiss}>
      <section className="oa-command-palette" role="dialog" aria-modal="true" aria-label={t("openWorkspace")} onMouseDown={event => event.stopPropagation()}>
        <div className="oa-command-search">
          <Search size={17} />
          <input

            value={query}
            onChange={event => setQuery(event.target.value)}
            placeholder={t("searchZanAi")}
            aria-label={t("searchZanAi")}
          />
          <button onClick={onDismiss} aria-label={t("closeCommandSearch")}><X size={16} /></button>
        </div>
        <div className="oa-command-results">
          {filtered.map(item => (
            <button key={item.id} onClick={() => { onSelect(item.id); onDismiss(); }}>
              <span className="oa-command-result-icon">{TAB_ICON[item.id]}</span>
              <span><strong>{item.label}</strong><small>{item.description}</small></span>
            </button>
          ))}
          {filtered.length === 0 && <div className="oa-command-empty">{t("noMatchingWorkspace")}</div>}
        </div>
        <footer><span>{t("navigateWithSearch")}</span><kbd>Esc</kbd><span>{t("close")}</span></footer>
      </section>
    </div>
  );
}
