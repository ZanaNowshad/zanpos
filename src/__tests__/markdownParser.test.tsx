import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MarkdownContent, parseMarkdown } from "../officeai/markdown";

function render(text: string): string {
  return renderToStaticMarkup(createElement(MarkdownContent, { text }));
}

describe("parseMarkdown termination (Ask AI freeze regression)", () => {
  // A mid-line pipe used to satisfy neither the table branch (doesn't start
  // with "|") nor the paragraph branch (`includes("|")` guard), so the parser
  // never advanced and the webview froze the moment chat history rendered.
  it("terminates on a line containing a mid-line pipe", () => {
    const html = render("Net sales | BHD 0.500");
    expect(html).toContain("Net sales | BHD 0.500");
  });

  it("terminates when a mid-line pipe follows a paragraph", () => {
    const html = render("Here is today's summary:\nsales | refunds | net\nAll good.");
    expect(html).toContain("summary");
    expect(html).toContain("All good.");
  });

  it("still renders real tables (leading pipe)", () => {
    const html = render("| Product | Qty |\n| --- | --- |\n| Cola | 3 |");
    expect(html).toContain("<table");
    expect(html).toContain("Cola");
  });

  it("terminates on pathological mixed content", () => {
    const text = [
      "# Report",
      "a | b",
      "| h1 | h2 |",
      "| - | - |",
      "| x | y |",
      "tail | end",
      "- item | with pipe",
      "1. numbered | with pipe",
      "done",
    ].join("\n");
    const nodes = parseMarkdown(text);
    expect(nodes.length).toBeGreaterThan(0);
    const html = render(text);
    expect(html).toContain("done");
  });
});
