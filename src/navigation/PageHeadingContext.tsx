import { createContext, useContext } from "react";

/**
 * The label of the section tab the shell is currently showing.
 *
 * Exists so a page can tell whether its own `<h1>` would merely repeat the tab
 * the user just pressed. `Catalogue › Categories` in a breadcrumb, `Categories`
 * in the tab strip and `Categories` again as a heading is the same word three
 * times before any content — on a 768px screen that is roughly two cart rows
 * spent saying nothing.
 *
 * The heading is hidden visually, never removed: screen readers still need a
 * document heading, and the tab strip is a `<nav>`, not one.
 */
/**
 * Both labels naming the operator's current position: the rail entry and, when
 * the domain has one, the section tab.
 *
 * A string was not enough. Customers/Directory printed "Customers" as its
 * heading while the rail said Customers two inches to the left, and Purchasing
 * did the same — neither matched its *tab*, so the old comparison missed both
 * and each kept a ~93px band to restate a word already on screen.
 */
export interface PageHeadingLabels {
  domain: string | null;
  section: string | null;
}

const PageHeadingContext = createContext<PageHeadingLabels | null>(null);

export const PageHeadingProvider = PageHeadingContext.Provider;

const same = (a: string | null | undefined, b: string) =>
  !!a && a.trim().toLowerCase() === b.trim().toLowerCase();

/** True when this title is already stated by the rail or the section tab. */
export function useTitleIsRedundant(title: string): boolean {
  const labels = useContext(PageHeadingContext);
  if (!labels) return false;
  return same(labels.section, title) || same(labels.domain, title);
}
