import { useMemo } from "react";
import { ALIAS_TABS, domainForTab, sectionsForDomain, type NavDomain } from "../navigation/config";
import type { OfficeAiTranslator } from "../i18n/officeAiStrings";
import type { OfficeTab } from "./officeAiTypes";

/**
 * Where the operator currently is, derived from the active tab.
 *
 * Lifted out of OfficeAIPage, which had grown past the 500-line limit the ship
 * gate enforces. These four values are one thought — resolve the tab to its
 * domain, its sections, the section actually highlighted, and that section's
 * label — and they are only ever read together.
 */
export function useActiveNavigation(
  tab: OfficeTab,
  roleName: string,
  navDomains: NavDomain[],
  t: OfficeAiTranslator,
) {
  const activeDomainId = useMemo(() => domainForTab(tab), [tab]);

  const activeSections = useMemo(
    () => sectionsForDomain(activeDomainId, roleName),
    [activeDomainId, roleName],
  );

  const activeDomain = useMemo(
    () => navDomains.find(d => d.id === activeDomainId),
    [navDomains, activeDomainId],
  );

  /** The section highlighted in L2 — resolves aliases so "operations" lights up "products". */
  const activeSectionTab = useMemo<OfficeTab>(() => {
    const aliased = (ALIAS_TABS[tab] ?? tab) as OfficeTab;
    return activeSections.some(s => s.id === aliased) ? aliased : (activeDomain?.defaultTab ?? aliased);
  }, [tab, activeSections, activeDomain]);

  /* The label the active tab is showing. A page whose own heading says the same
     word suppresses it rather than stating the location a third time. */
  const activeSectionLabel = useMemo(() => {
    const section = activeSections.find(s => s.id === activeSectionTab);
    return section ? t(section.labelKey) : null;
  }, [activeSections, activeSectionTab, t]);

  return { activeDomainId, activeSections, activeDomain, activeSectionTab, activeSectionLabel };
}
