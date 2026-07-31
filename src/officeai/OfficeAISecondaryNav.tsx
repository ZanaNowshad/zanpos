import type { OfficeSubNavGroup, OfficeSubTab } from "./nav";
import { TAB_ICON } from "./nav";
import type { OfficeTab } from "./officeAiTypes";

interface Props {
  label: string;
  groups: OfficeSubNavGroup[];
  tabs: OfficeSubTab[];
  activeTab: OfficeTab;
  onSelect: (tab: OfficeTab) => void;
}

export default function OfficeAISecondaryNav({ label, groups, tabs, activeTab, onSelect }: Props) {
  const tabById = new Map(tabs.map(tab => [tab.id, tab]));

  return (
    <aside className="oa-secondary-nav" aria-label={label}>
      {groups.map(group => {
        const items = group.tabs.map(id => tabById.get(id)).filter((item): item is OfficeSubTab => Boolean(item));
        if (items.length === 0) return null;
        return (
          <section key={group.id}>
            <h2>{group.label}</h2>
            {items.map(item => (
              <button
                key={item.id}
                className={activeTab === item.id ? "active" : ""}
                onClick={() => onSelect(item.id)}
                aria-current={activeTab === item.id ? "page" : undefined}
                title={item.description}
              >
                {TAB_ICON[item.id]}
                <span>{item.label}</span>
              </button>
            ))}
          </section>
        );
      })}
    </aside>
  );
}
