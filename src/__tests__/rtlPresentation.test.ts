import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import overviewSource from "../officeai/OfficeAIOverview.tsx?raw";
import chatSource from "../officeai/ChatPanel.tsx?raw";
import lockScreenSource from "../components/LockScreen.tsx?raw";
import loginSource from "../pages/LoginScreen.tsx?raw";
import reportsSource from "../components/ReportsTab.tsx?raw";
import OfficeAIOverview, { splitBidiNumericText } from "../officeai/OfficeAIOverview";
import posSidebarSource from "../components/pos/PosSidebar.tsx?raw";

const tokensCss = readFileSync(new URL("../styles/tokens.css", import.meta.url), "utf8");

describe("RTL presentation contract", () => {
  it("marks login navigation arrows as directional", () => {
    expect(loginSource).toContain("login-panel-icon icon-directional");
    expect(loginSource).toContain("user-arrow icon-directional");
    expect(loginSource).toContain('<span className="icon-directional" aria-hidden="true">←</span>');
  });

  it("keeps mirroring opt-in and numeric isolation explicit", () => {
    expect(tokensCss).toContain('[dir="rtl"] :where(.icon-directional, [data-icon-direction="inline"])');
    expect(tokensCss).toContain("transform: scaleX(-1)");
    expect(tokensCss).toContain("direction: ltr");
    expect(tokensCss).toContain("unicode-bidi: isolate");
    expect(tokensCss).not.toMatch(/\[dir=["']rtl["']\][^{]*(?:svg|lucide)[^{]*\{/i);
  });

  it("marks both report pagination directions", () => {
    expect(reportsSource).toContain('<span className="icon-directional" aria-hidden="true">←</span>');
    expect(reportsSource).toContain('<span className="icon-directional" aria-hidden="true">→</span>');
  });

  it("marks directional Lucide controls but leaves refresh controls unmarked", () => {
    expect(overviewSource).toContain('<ArrowRight className="icon-directional"');
    expect(overviewSource).not.toContain('<RefreshCw className="icon-directional"');
    expect(chatSource).toContain('<SendHorizontal className="icon-directional"');
  });

  it("does not mirror neutral POS icons", () => {
    expect(posSidebarSource).toContain('<LogOut className="icon-directional"');
    expect(posSidebarSource).not.toContain('<Bell className="icon-directional"');
    expect(posSidebarSource).not.toContain('<Power className="icon-directional"');
  });

  it("keeps the PIN deletion key out of directional mirroring", () => {
    expect(lockScreenSource).not.toContain("icon-directional");
  });

  it("separates numeric fragments from RTL prose", () => {
    expect(splitBidiNumericText("27 transactions with BHD 123.456 and 2 approvals"))
      .toEqual(["27", " transactions with ", "BHD 123.456", " and ", "2", " approvals"]);
    expect(splitBidiNumericText("margin -2.5% and USD +12.50"))
      .toEqual(["margin ", "-2.5%", " and ", "USD +12.50"]);
  });

  it("renders isolated numeric boundaries in the OfficeAI summary", () => {
    const html = renderToStaticMarkup(createElement(OfficeAIOverview, {
      snapshot: {
        loading: false,
        refreshedAt: null,
        errors: [],
        today: null,
        lowStockCount: 4,
        outOfStockCount: 2,
        sync: null,
        whatsapp: null,
        whatsappUnread: 0,
        paymentConfirmations: [],
        provider: null,
        aiEnabled: true,
        aiConfig: null,
        featureToggles: null,
        health: null,
        benefitNumber: null,
      },
      currencyExp: 3,
      canUseManagerTools: true,
      pendingActionCount: 2,
      onOpenTab: () => {},
      onRefresh: () => {},
    }));
    expect(html).toContain('<bdi class="numeric-ltr" dir="ltr">0</bdi>');
    expect(html).toContain('<strong class="numeric-ltr">BHD 0.000</strong>');
  });
});
