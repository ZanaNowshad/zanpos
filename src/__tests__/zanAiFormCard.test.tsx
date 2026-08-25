import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import AiFormCard from "../officeai/AiFormCard";
import { normalizeAiForm } from "../officeai/aiForm";

/**
 * What the operator actually sees.
 *
 * The card is rendered from a spec the model wrote, on a 1024x768 touch panel.
 * Two things have to hold every time: the right keyboard opens for the value
 * being asked for, and nothing the model sent is treated as markup.
 */

function render(spec: unknown, disabled = false) {
  const form = normalizeAiForm(spec)!;
  return renderToStaticMarkup(
    <AiFormCard form={form} disabled={disabled} onSubmit={() => {}} onDismiss={() => {}} />,
  );
}

describe("ZanAI form card", () => {
  it("labels every box and opens the keypad the value needs", () => {
    const html = render({
      title: "Update price",
      note: "Scan or type the barcode.",
      submit_label: "Update price",
      fields: [
        { name: "barcode", label: "Product barcode", type: "barcode", required: true },
        { name: "new_price", label: "New price (BHD)", type: "money", required: true },
      ],
    });

    expect(html).toContain("Update price");
    expect(html).toContain("Scan or type the barcode.");
    expect(html).toContain("Product barcode");
    // A money field that raises the alphabet is a field the cashier fights.
    // Lowercased because the attribute's casing in the serialised output is a
    // React detail; the browser reads it either way.
    expect(html.toLowerCase()).toContain('inputmode="numeric"');
    expect(html.toLowerCase()).toContain('inputmode="decimal"');
    // Every input is bound to its own label, not to an index-numbered id.
    const forCount = (html.match(/for="/g) ?? []).length;
    expect(forCount).toBe(2);
    expect(html).not.toContain('for=""');
    expect(html).not.toContain("aria-label=\"undefined\"");
  });

  /** A supplier's delivery note is untrusted text. It reaches this card through
   *  OCR, and a product name carrying markup must show up as an odd product
   *  name, never as markup. */
  it("escapes model-authored text instead of rendering it", () => {
    const html = render({
      title: "<img src=x onerror=alert(1)>",
      fields: [{ name: "n", label: "**bold**", type: "text" }],
    });

    expect(html).not.toContain("<img src=x");
    expect(html).toContain("&lt;img src=x");
    // Plain text, not markdown: the label is the label the model wrote.
    expect(html).toContain("**bold**");
    expect(html).not.toContain("<strong>bold</strong>");
  });

  it("draws a bill as a numbered grid with a labelled cell per column", () => {
    const html = render({
      title: "Purchase bill",
      table: {
        columns: [
          { name: "barcode", label: "Barcode", type: "barcode", required: true },
          { name: "name", label: "Product", type: "text", required: true },
          { name: "qty", label: "Qty", type: "integer", required: true },
        ],
        rows: [
          { barcode: "6281234567890", name: "Nadec Laban 1L", qty: "24" },
          { barcode: "", name: "Local Dates 500g", qty: "6" },
        ],
        row_label: "line",
        allow_add: true,
        allow_remove: true,
      },
    });

    expect(html).toContain('value="6281234567890"');
    expect(html).toContain('value="Nadec Laban 1L"');
    // Every cell carries its own label rather than relying on a header row the
    // dock is too narrow to keep — three columns over two lines is six labels.
    expect((html.match(/class="aiform-label"/g) ?? []).length).toBe(6);
    expect(html).toContain('aria-label="line 2"');
    expect(html).toContain("Add line");
    expect(html).toContain('aria-label="Remove line 2"');
  });

  it("offers a choice as a button carrying its detail line", () => {
    const html = render({
      title: "Apply where?",
      choices: [
        { value: "this", label: "This branch", detail: "Amwaj", style: "primary" },
        { value: "all", label: "Every branch", style: "danger" },
      ],
    });

    expect(html).toContain("aiform-choice-primary");
    expect(html).toContain("aiform-choice-danger");
    expect(html).toContain("Amwaj");
    // Choices answer on their own; there is nothing to submit alongside them.
    expect(html).not.toContain("aiform-submit");
  });

  it("locks every control while a reply is still streaming", () => {
    const html = render(
      {
        title: "Update price",
        fields: [{ name: "barcode", label: "Barcode", type: "barcode" }],
        choices: [{ value: "cancel", label: "Cancel" }],
      },
      true,
    );

    expect((html.match(/disabled=""/g) ?? []).length).toBeGreaterThanOrEqual(3);
  });
});
