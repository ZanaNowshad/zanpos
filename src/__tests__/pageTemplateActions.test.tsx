import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import PageTemplate from "../components/templates/PageTemplate";
import { PageHeadingProvider } from "../navigation/PageHeadingContext";

/**
 * The header band can now vanish, and secondary actions can now hide behind a
 * menu. Both are space wins and both are ways to lose a button silently.
 *
 * Products carries six actions; if the overflow threshold or the collapse
 * condition regresses, the cashier loses Import, Export, Labels, Duplicates or
 * Fetch images with nothing on screen to say so. These assert on rendered
 * output rather than the source, because the failure mode is "the element
 * exists but nothing renders it".
 */

const ACTIONS = ["Fetch images", "Duplicates", "Labels", "Import", "Export"];

function render(opts: {
  title: string;
  sectionLabel?: string;
  domainLabel?: string;
  subtitle?: string;
  toolbar?: boolean;
  secondaryCount?: number;
}) {
  const secondary = ACTIONS.slice(0, opts.secondaryCount ?? 0).map(label => ({ label, onClick: vi.fn() }));
  return renderToStaticMarkup(
    <PageHeadingProvider value={{ domain: opts.domainLabel ?? null, section: opts.sectionLabel ?? null }}>
      <PageTemplate
        header={{
          title: opts.title,
          subtitle: opts.subtitle,
          primaryAction: { label: "+ New Product", onClick: vi.fn() },
          secondaryActions: secondary,
        }}
        toolbar={opts.toolbar ? <div className="zp-toolbar">filters</div> : undefined}
      >
        <div>content</div>
      </PageTemplate>
    </PageHeadingProvider>,
  );
}

describe("page template keeps every action reachable", () => {
  it("collapses the header band when the tab already says the title", () => {
    const html = render({ title: "Products", sectionLabel: "Products", toolbar: true, secondaryCount: 5 });
    expect(html).toContain("oa-subnav-line-merged");
    // The heading survives for screen readers, just not as a visible band.
    expect(html).toContain("zp-visually-hidden");
    expect(html).not.toContain('class="oa-topbar"');
  });

  it("collapses even with no toolbar to merge into", () => {
    // Staff has no filters and was spending ~100px on a redundant title plus
    // one button stacked beneath it.
    const html = render({ title: "Staff", sectionLabel: "Staff", secondaryCount: 0 });
    expect(html).toContain("oa-subnav-line-merged");
    expect(html).toContain("+ New Product");
    expect(html).not.toContain('class="oa-topbar"');
  });

  it("collapses when the title repeats the rail entry rather than the tab", () => {
    // Customers/Directory printed "Customers" while the rail said Customers,
    // and Purchasing did the same — neither matched its own tab, so comparing
    // against the section label alone missed both.
    const html = render({ title: "Customers", domainLabel: "Customers", sectionLabel: "Directory", toolbar: true });
    expect(html).toContain("oa-subnav-line-merged");
    expect(html).not.toContain('class="oa-topbar"');
  });

  it("keeps the band when the title is not redundant", () => {
    const html = render({ title: "Staff Users", sectionLabel: "Staff", toolbar: true });
    expect(html).toContain('class="oa-topbar"');
  });

  it("keeps the band when a subtitle carries real content", () => {
    const html = render({ title: "Products", sectionLabel: "Products", subtitle: "Ten one-tap items", toolbar: true });
    expect(html).toContain('class="oa-topbar"');
    expect(html).toContain("Ten one-tap items");
  });

  it("puts every secondary action in the menu once there are too many to fit", () => {
    const html = render({ title: "Products", sectionLabel: "Products", toolbar: true, secondaryCount: 5 });
    expect(html).toContain("oa-overflow-trigger");
    for (const label of ACTIONS) {
      // Closed menu renders nothing, so the labels must not be inline either —
      // what matters is that the trigger exists to reach them.
      expect(html, `${label} must not be an inline button`).not.toContain(`>${label}<`);
    }
    // The primary action always stays in the open.
    expect(html).toContain("+ New Product");
  });

  it("leaves a small number of secondary actions inline", () => {
    const html = render({ title: "Products", sectionLabel: "Products", toolbar: true, secondaryCount: 2 });
    expect(html).not.toContain("oa-overflow-trigger");
    expect(html).toContain("Fetch images");
    expect(html).toContain("Duplicates");
  });
});
