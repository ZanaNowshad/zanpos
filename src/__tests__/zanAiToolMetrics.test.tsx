import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ZanAiToolMetrics, type ZanAiToolMetricRow } from "../components/settings/ZanAiToolMetrics";

describe("ZanAiToolMetrics", () => {
  it("shows latency, failure rate, estimated tokens, and audit volume per tool", () => {
    const rows: ZanAiToolMetricRow[] = [{
      tool_name: "get_stock_levels",
      invocation_count: 20,
      success_count: 18,
      failure_count: 2,
      average_latency_ms: 125,
      last_latency_ms: 90,
      estimated_tokens: 840,
      last_error_at: "2026-08-15T01:00:00Z",
      updated_at: "2026-08-15T01:05:00Z",
    }];

    const html = renderToStaticMarkup(<ZanAiToolMetrics rows={rows} loading={false} />);
    expect(html).toContain("get_stock_levels");
    expect(html).toContain("125 ms");
    expect(html).toContain("10.0%");
    expect(html).toContain("840");
    expect(html).toContain("20 calls");
  });
});
