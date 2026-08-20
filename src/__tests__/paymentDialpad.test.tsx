import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { applyDialpadKeyToField } from "../components/Dialpad";
import { PaymentCommandPanel } from "../components/PaymentExperience";

/**
 * The tender keypad has to serve every field in the payment modal, not just the
 * cash amount: on a delivery or digital sale the phone, house, flat and road
 * fields are the ones the cashier fills, usually on a touchscreen with no
 * physical keyboard attached.
 */
describe("dialpad typing into a focused field", () => {
  it("inserts at the caret rather than appending", () => {
    expect(applyDialpadKeyToField("3300666", 4, 4, "5")).toEqual({ value: "33005666", caret: 5 });
  });

  it("replaces a selection", () => {
    expect(applyDialpadKeyToField("33050666", 0, 8, "7")).toEqual({ value: "7", caret: 1 });
  });

  it("backspaces the character before the caret", () => {
    expect(applyDialpadKeyToField("33050666", 8, 8, "⌫")).toEqual({ value: "3305066", caret: 7 });
    expect(applyDialpadKeyToField("33050666", 3, 3, "⌫")).toEqual({ value: "3350666", caret: 2 });
  });

  it("backspaces a selection in one press instead of one character", () => {
    expect(applyDialpadKeyToField("33050666", 2, 5, "⌫")).toEqual({ value: "33666", caret: 2 });
  });

  it("does nothing at the start of an empty field", () => {
    expect(applyDialpadKeyToField("", 0, 0, "⌫")).toEqual({ value: "", caret: 0 });
  });

  it("clears the whole field on C", () => {
    expect(applyDialpadKeyToField("33050666", 4, 4, "C")).toEqual({ value: "", caret: 0 });
  });

  it("types the multi-character keys as written", () => {
    expect(applyDialpadKeyToField("1", 1, 1, "00")).toEqual({ value: "100", caret: 3 });
    expect(applyDialpadKeyToField("1", 1, 1, ".")).toEqual({ value: "1.", caret: 2 });
  });

  it("survives a caret reported beyond the value", () => {
    // Some fields report a stale selection after a programmatic write.
    expect(applyDialpadKeyToField("12", 99, 99, "3")).toEqual({ value: "123", caret: 3 });
  });
});

describe("payment command panel", () => {
  const props = {
    /* These cases all exercise the input surface — the summary surface is the
       resting state and has no dialpad to assert on. */
    surface: "input" as const,
    summaryTitle: "Order summary",
    summaryRows: [],
    fieldLabel: "",
    fieldValue: "",
    onDone: () => {},
    blockers: [],
    attempted: false,
    onAttemptBlocked: () => {},
    inputMode: "pad" as const,
    onToggleInputMode: () => {},
    showTendered: false,
    activeLabel: "Amount",
    currency: "BHD",
    tenderedStr: "",
    totalLabel: "0.650",
    methodName: "Card",
    readinessInstruction: "Confirm approval on the card terminal",
    confirmBlockReason: null,
    canConfirm: true,
    completionLabel: "Complete card sale",
    keyboardError: null,
    onKey: () => {},
    onOpenKeyboard: () => {},
    onConfirm: () => {},
    onCancel: () => {},
  };

  it("offers the OS keyboard for the fields the keypad cannot fill", () => {
    // A delivery sale asks for a road and a customer name; the till has no
    // physical keyboard.
    const html = renderToStaticMarkup(
      <PaymentCommandPanel {...props} showNumericEntry={false} />,
    );

    expect(html).toContain("pm-keyboard-btn");
    expect(html).toContain("Keyboard");
  });

  it("keeps the dialpad on screen when there is no cash amount to type", () => {
    // It used to be swapped out for a readiness panel, which left card, wallet
    // and delivery sales with no keypad for their own fields. That panel is
    // gone — the resting state is now the order summary on the other surface —
    // but the rule it existed to break still holds: while this surface is up,
    // there is always something to type into.
    const html = renderToStaticMarkup(
      <PaymentCommandPanel {...props} showNumericEntry={false} />,
    );

    expect(html).toContain('class="dialpad"');
    expect(html).toContain("pm-keyboard-btn");
    expect(html).not.toContain("pm-ready-panel");
  });

  it("shows the entry heading instead of the readiness panel while typing cash", () => {
    const html = renderToStaticMarkup(
      <PaymentCommandPanel {...props} showNumericEntry showTendered tenderedStr="5.000" />,
    );

    expect(html).toContain('class="dialpad"');
    expect(html).not.toContain("pm-ready-panel");
    expect(html).toContain("5.000");
  });
});
