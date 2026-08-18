import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  filterToolCentreRows,
  ZanAiToolCentre,
  type ZanAiToolCentreRow,
} from "../components/settings/ZanAiToolCentre";

const rows: ZanAiToolCentreRow[] = [
  {
    name: "lookup_barcode",
    description: "Look up product data by barcode.",
    kind: "read",
    execution: "read",
    permission: "cashier",
    risk: "medium",
    confirmation: "automatic",
    enabled: true,
    feature: null,
    undo: "none",
  },
  {
    name: "send_whatsapp_message",
    description: "Send an outbound WhatsApp message.",
    kind: "mutation",
    execution: "action",
    permission: "manager",
    risk: "critical",
    confirmation: "required",
    enabled: false,
    feature: "whatsapp",
    undo: "none",
  },
];

describe("ZanAiToolCentre", () => {
  it("renders permission, risk, enabled state, and the real tool description", () => {
    const html = renderToStaticMarkup(
      <ZanAiToolCentre rows={rows} loading={false} onEnabledChange={() => {}} />,
    );

    expect(html).toContain("lookup_barcode");
    expect(html).toContain("cashier");
    expect(html).toContain("medium");
    expect(html).toContain("Look up product data by barcode.");
    expect(html).toContain("1 enabled");
    expect(html).toContain("1 protected");
  });

  it("searches names and descriptions and filters by risk", () => {
    expect(filterToolCentreRows(rows, "outbound", "all").map(row => row.name))
      .toEqual(["send_whatsapp_message"]);
    expect(filterToolCentreRows(rows, "", "critical").map(row => row.name))
      .toEqual(["send_whatsapp_message"]);
  });
});
