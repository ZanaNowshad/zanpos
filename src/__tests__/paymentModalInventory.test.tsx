import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import PaymentModal from "../components/PaymentModal";

/**
 * The payment modal was redesigned; nothing was allowed to fall out of it.
 *
 * The till has an inventory contract (`posTillInventory`) and the modal did
 * not, which is how a redesign quietly stranded the readiness panel — it still
 * existed in the source but no journey could reach it any more. Source greps
 * cannot catch that: the element was there, the condition was not. So this
 * renders each journey and asserts on what the cashier can actually see.
 *
 * A row here is a promise about one journey, not a snapshot of the markup —
 * restyling should not break it, removing a capability should.
 */

const base = { netTotal: 220, onConfirm: vi.fn(), onCancel: vi.fn() };
const render = (props: Record<string, unknown>) =>
  renderToStaticMarkup(<PaymentModal {...base} {...props} />);

const RECEIPT_CASH = render({ initialMethod: "cash" });
const RECEIPT_CARD = render({ initialMethod: "card" });
const DELIVERY = render({ journey: "delivery" });
const DIGITAL = render({ journey: "digital" });
const ALL: [string, string][] = [
  ["receipt/cash", RECEIPT_CASH],
  ["receipt/card", RECEIPT_CARD],
  ["delivery", DELIVERY],
  ["digital", DIGITAL],
];

describe("payment modal keeps every capability reachable", () => {
  it("offers method choice, confirm, cancel and a block reason on every journey", () => {
    for (const [name, html] of ALL) {
      expect(html, `${name}: method picker`).toContain('aria-label="Payment method"');
      expect(html, `${name}: confirm`).toContain("pm-confirm-btn");
      expect(html, `${name}: cancel`).toContain("pm-cancel-btn");
      // The reason a sale cannot complete has to be visible, or the cashier is
      // left pressing a dead button with a queue behind them.
      expect(html, `${name}: block reason slot`).toContain("pm-confirm-status");
      expect(html, `${name}: print option`).toContain("Print receipt");
      expect(html, `${name}: journey switch`).toContain('aria-label="Checkout type"');
    }
  });

  it("takes cash only where cash actually crosses the counter", () => {
    // A delivery is collected by the rider; a digital sale already cleared on a
    // terminal. Offering a tender field on either invites the cashier to
    // believe money arrived that has not.
    expect(RECEIPT_CASH).toContain("Cash received");
    expect(RECEIPT_CASH).toContain("Change due");
    for (const [name, html] of [["delivery", DELIVERY], ["digital", DIGITAL]] as const) {
      expect(html, `${name}: no tender field`).not.toContain("Cash received");
      expect(html, `${name}: no change readout`).not.toContain("Change due");
    }
  });

  it("keeps the instruction for money taken on another device", () => {
    // Regression: this lived in a readiness panel that the contextual input
    // surface made unreachable. It now rides the order summary.
    expect(RECEIPT_CARD).toContain("card terminal");
    expect(DIGITAL).toContain("pm-summary-note");
  });

  it("rests on the order summary and shows the keypad only while editing", () => {
    // One fixed input zone on the right: summary at rest, keypad when a field
    // holds the caret. Cash opens straight into the tender field.
    expect(RECEIPT_CASH).toContain('aria-label="Dialpad"');
    for (const [name, html] of [["receipt/card", RECEIPT_CARD], ["delivery", DELIVERY], ["digital", DIGITAL]] as const) {
      expect(html, `${name}: rests on summary`).toContain("Order summary");
      expect(html, `${name}: no resting keypad`).not.toContain('aria-label="Dialpad"');
    }
  });

  it("keeps split payment on the counter sale it belongs to", () => {
    // Unchanged by the redesign: split was always receipt-only, because a
    // delivery is collected once and a digital sale clears once.
    expect(RECEIPT_CASH).toContain("Split payment");
    expect(DELIVERY).not.toContain("Split payment");
  });

  it("asks a delivery for an address and a rider, and nothing else does", () => {
    expect(DELIVERY).toContain("House number");
    expect(DELIVERY).toContain("Rider");
    expect(RECEIPT_CASH).not.toContain("House number");
    expect(DIGITAL).not.toContain("House number");
  });

  it("never disables the confirm button except while saving", () => {
    // A dead button teaches the cashier nothing. Delivery opens with no phone
    // and no house number, so it is blocked — and still pressable, because
    // pressing it is how the cashier finds out what is missing.
    for (const [name, html] of ALL) {
      expect(html, `${name}: confirm not disabled`).not.toMatch(/class="[^"]*pm-confirm-btn[^"]*"[^>]*disabled/);
    }
    expect(DELIVERY).toContain("pm-confirm-btn-blocked");
    expect(RECEIPT_CASH).not.toContain("pm-confirm-btn-blocked");
  });

  it("offers letters as well as digits for the fields that need them", () => {
    // A customer name, an area, a road, a flat like "3B". The till has no
    // keyboard plugged in, so the letters have to be on screen.
    expect(RECEIPT_CASH).toContain("pm-mode-btn");
    expect(RECEIPT_CASH).toContain("pm-keyboard-btn");
  });

  it("asks both contact journeys for a destination, by name or by number", () => {
    /* The field took digits only, which meant a cashier told "it's for Fatima"
       had to leave it, open a directory and come back. It now takes either, so
       the assertion is on the field and its dual purpose rather than on the
       word "phone". */
    for (const [name, html] of [["delivery", DELIVERY], ["digital", DIGITAL]] as const) {
      expect(html, `${name}: contact field`).toContain('id="payment-customer-phone"');
      expect(html, `${name}: takes a name too`).toContain("Customer name or number");
      expect(html, `${name}: directory reachable`).toContain("Browse the customer directory");
    }
    expect(RECEIPT_CASH).not.toContain("Customer name or number");
  });
});
