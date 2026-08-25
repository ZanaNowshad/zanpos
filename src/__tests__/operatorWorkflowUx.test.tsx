import { createElement, type ComponentProps } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import PosCartTable from "../components/pos/PosCartTable";
import DiscountModal, { calculateDiscountMinor } from "../components/DiscountModal";
import PriceInputModal from "../components/PriceInputModal";
import DeliveriesTab, { deliveryStatusActionLabel } from "../components/DeliveriesTab";
import PaymentModal from "../components/PaymentModal";
import PosTotalsPanel from "../components/pos/PosTotalsPanel";
import PosSidebar from "../components/pos/PosSidebar";
import TodayReportModal from "../components/TodayReportModal";
import BusinessTab from "../components/settings/BusinessTab";
import { operationsTranslator } from "../i18n/operationsStrings";
import StickyNotesPanel from "../components/StickyNotesPanel";
import LoginScreen, { isValidLoginPin } from "../pages/LoginScreen";
import { toLocalDateTimeInput, tomorrowMorningInput } from "../utils/stickyNotes";
import type { Cart, SessionUser } from "../types";

const user: SessionUser = {
  user_id: "u-1",
  branch_id: "main",
  display_name: "Reni",
  username: "reni",
  role_id: "owner-role",
  role_name: "owner",
  session_token: "session",
  session_expires_at: "2026-07-25T00:00:00Z",
};

const cart: Cart = {
  cart_id: "cart-1",
  branch_id: "main",
  device_id: "pos-1",
  shift_id: "shift-1",
  cashier_user_id: "u-1",
  lines: [],
  bill_discount_minor: 0,
  bill_discount_reason: null,
};

type CheckoutJourney = "receipt" | "delivery" | "digital";

function renderPaymentJourney(journey: CheckoutJourney, initialMethod: "cash" | "card" | "wallet" = "cash") {
  const props: ComponentProps<typeof PaymentModal> & { journey: CheckoutJourney } = {
    netTotal: 220,
    initialMethod,
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
    journey,
  };
  return renderToStaticMarkup(createElement(PaymentModal, props));
}

describe("operator-first workflow hierarchy", () => {
  it("frames login as a fast register handoff", () => {
    const html = renderToStaticMarkup(<LoginScreen onLogin={vi.fn()} />);
    expect(html).toContain("Who’s on register?");
    expect(html).toContain("Tap your name");
  });

  it("blocks incomplete PINs before they consume an authentication attempt", () => {
    expect(isValidLoginPin("123")).toBe(false);
    expect(isValidLoginPin("1234")).toBe(true);
    expect(isValidLoginPin("123456")).toBe(true);
    expect(isValidLoginPin("1234567")).toBe(false);
  });

  it("turns the empty cart into a next-action runway", () => {
    const html = renderToStaticMarkup(
      <PosCartTable
        cart={cart}
        selectedLineId={null}
        disabled={false}
        onSelectLine={vi.fn()}
        onBumpQty={vi.fn()}
        onEditPrice={vi.fn()}
      />,
    );
    // The empty till names the next action and where focus lives, because a
    // blank basket is the moment a new cashier most needs telling.
    expect(html).toContain("Scan an item to start the sale");
    expect(html).toContain("F2");
  });

  it("uses a cart-line click for price editing instead of line discounting", () => {
    const cartWithItem: Cart = {
      ...cart,
      lines: [{
        cart_line_id: "line-1",
        product_id: "product-1",
        product_name: "Cola",
        sku: "COLA",
        barcode: "123456789",
        quantity: "1",
        unit_price_minor: 400,
        line_discount_minor: 0,
        line_discount_reason: null,
        tax_rule_id: "tax-1",
        tax_rate_basis_points: 0,
        tax_inclusive: false,
        tax_amount_minor: 0,
        line_total_minor: 400,
        note: null,
        voided: false,
      }],
    };

    const html = renderToStaticMarkup(
      <PosCartTable
        cart={cartWithItem}
        selectedLineId="line-1"
        disabled={false}
        onSelectLine={vi.fn()}
        onBumpQty={vi.fn()}
        onEditPrice={vi.fn()}
      />,
    );

    // A tap selects the row, and the selected row reveals the two corrections
    // a till actually needs on a line: quantity, and a price that disagrees
    // with the shelf. No double-tap target — slow and undiscoverable on touch.
    expect(html).toContain("till-row-select");
    expect(html).toContain("is-selected");
    expect(html).toContain("till-qty-step");
    expect(html).toContain("till-price-btn");
    expect(html).toContain("Change the price of Cola");
    expect(html).toContain("One more Cola");
    expect(html).toContain("One fewer Cola");
    expect(html).not.toContain("Tap to edit quantity, discount, or note");
  });

  it("keeps unselected rows free of controls so the basket reads as a list", () => {
    const cartWithTwo: Cart = {
      ...cart,
      lines: [
        {
          cart_line_id: "line-1", product_id: "p1", product_name: "Cola", sku: "COLA",
          barcode: "1", quantity: "1", unit_price_minor: 400, line_discount_minor: 0,
          line_discount_reason: null, tax_rule_id: "t", tax_rate_basis_points: 0,
          tax_inclusive: false, tax_amount_minor: 0, line_total_minor: 400,
          note: null, voided: false,
        },
      ],
    };
    const html = renderToStaticMarkup(
      <PosCartTable
        cart={cartWithTwo}
        selectedLineId={null}
        disabled={false}
        onSelectLine={vi.fn()}
        onBumpQty={vi.fn()}
        onEditPrice={vi.fn()}
      />,
    );
    expect(html).not.toContain("till-qty-step");
    expect(html).not.toContain("till-price-btn");
  });

  it("shows the scanned product image in its cart row", () => {
    const cartWithImage = {
      ...cart,
      lines: [{
        cart_line_id: "line-image",
        product_id: "product-cola",
        product_name: "Cola",
        sku: "COLA",
        barcode: "123456789",
        image_path: "https://images.example.test/cola.jpg",
        quantity: "1",
        unit_price_minor: 400,
        line_discount_minor: 0,
        line_discount_reason: null,
        tax_rule_id: "tax-1",
        tax_rate_basis_points: 0,
        tax_inclusive: false,
        tax_amount_minor: 0,
        line_total_minor: 400,
        note: null,
        voided: false,
      }],
    } as unknown as Cart;

    const html = renderToStaticMarkup(
      <PosCartTable
        cart={cartWithImage}
        selectedLineId={null}
        disabled={false}
        onSelectLine={vi.fn()}
        onBumpQty={vi.fn()}
        onEditPrice={vi.fn()}
      />,
    );

    expect(html).toContain("till-c-thumb");
    expect(html).toContain('src="https://images.example.test/cola.jpg"');
  });

  it("offers whole-bill and individual-item discount scopes from one modal", () => {
    const html = renderToStaticMarkup(
      <DiscountModal
        grossMinor={400}
        currentBillDiscountMinor={0}
        lines={[{
          cart_line_id: "line-1",
          product_id: "product-1",
          product_name: "Cola",
          sku: "COLA",
          barcode: "123456789",
          quantity: "2",
          unit_price_minor: 200,
          line_discount_minor: 0,
          line_discount_reason: null,
          tax_rule_id: "tax-1",
          tax_rate_basis_points: 0,
          tax_inclusive: false,
          tax_amount_minor: 0,
          line_total_minor: 400,
          note: null,
          voided: false,
        }]}
        initialLineId="line-1"
        onApplyBill={vi.fn()}
        onApplyLine={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(html).toContain("Whole bill");
    expect(html).toContain("Individual item");
    expect(html).toContain("Percentage");
    expect(html).toContain("Fixed amount");
    expect(html).toContain("Cola × 2");
    expect(html).toContain("Item subtotal");
  });

  it("calculates per-item percentage and fixed discounts in integer minor units", () => {
    expect(calculateDiscountMinor("pct", "12.5", 800, 3)).toBe(100);
    expect(calculateDiscountMinor("flat", "0.125", 800, 3)).toBe(125);
    expect(calculateDiscountMinor("flat", "9.000", 800, 3)).toBe(800);
  });

  it("prefills the cart-line price editor with the current item price", () => {
    const html = renderToStaticMarkup(
      <PriceInputModal
        productName="Cola"
        currentPriceMinor={400}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(html).toContain("Change item price");
    expect(html).toContain("Current price: BHD 0.400");
    expect(html).toContain('value="0.400"');
  });

  it("makes amount due and the completion action explicit in payment", () => {
    const html = renderPaymentJourney("receipt", "card");
    expect(html).toContain("Amount due");
    // The button names the action and the sum, not a generic "complete".
    expect(html).toContain("Take BHD 0.220");
    // The journey is switchable in place — a customer who changes their mind
    // must not cost the cashier everything they have typed.
    expect(html).toContain('aria-label="Checkout type"');
    expect(html).toContain("Print receipt");
    expect(html).not.toContain("Optional order details");
    expect(html).not.toContain("Customer phone");
    expect(html).not.toContain("House number");
    expect(html).toContain('aria-label="Payment method"');
    expect(html).toContain('role="radio"');
    expect(html).toContain('aria-checked="true"');
    expect(html).not.toContain("autofocus");
  });

  it("offers three checkout journeys below Fast Cash instead of payment methods", () => {
    const html = renderToStaticMarkup(
      <PosTotalsPanel
        cart={cart}
        taxTotal={0}
        payableTotal={0}
        exchangeCredit={null}
        exchangeBalance={null}
        payFastLoading={false}
        paymentStarted={false}
        onCancelExchange={vi.fn()}
        onCompleteCoveredExchange={vi.fn()}
        onPayFast={vi.fn()}
        onOpenPaymentJourney={vi.fn()}
        onOpenMore={vi.fn()}
      />,
    );
    expect(html).toContain("Fast Cash");
    expect(html).toContain("Receipt");
    expect(html).toContain("Delivery");
    expect(html).toContain("Digital");
    expect(html).not.toContain(">Wallet<");
    // Split moved into More Options rather than competing with the journeys.
    expect(html).not.toContain(">Split<");
    expect(html).toContain("More Options");
  });

  it("shows direct contact and required address fields for delivery checkout", () => {
    const html = renderPaymentJourney("delivery");
    expect(html).toContain("pm-shell-contact");
    expect(html).toContain("Customer name or number");
    expect(html).toContain('aria-label="Browse the customer directory"');
    expect(html).toContain("House number");
    expect(html).toContain("Flat");
    expect(html).toContain("Road");
    expect(html).toContain('autoComplete="address-line1"');
    expect(html).toContain('autoComplete="address-line2"');
    expect(html).toContain('enterKeyHint="next"');
    expect(html).toContain("BenefitPay");
    expect(html).not.toContain("Optional order details");
  });

  it("keeps digital checkout contactable without showing delivery address fields", () => {
    const html = renderPaymentJourney("digital");
    expect(html).toContain("pm-shell-contact");
    expect(html).toContain("Customer name or number");
    expect(html).toContain('aria-label="Browse the customer directory"');
    expect(html).toContain("Where the receipt is sent");
    expect(html).toContain("BenefitPay");
    expect(html).not.toContain("House number");
    expect(html).not.toContain("Flat");
    expect(html).not.toContain("Road");
  });

  /*
   * This used to assert the opposite — that an exact card payment *replaced*
   * the dialpad with the readiness summary. That reasoned only about the cash
   * amount, which a card sale has no need to type. It missed the other fields
   * in the same modal: the customer phone on a digital receipt, and the house,
   * flat and road on a delivery, none of which have any on-screen keypad on a
   * till with no physical keyboard. The summary now shares the column with the
   * dialpad instead of standing in for it.
   */
  /* The right-hand column is one fixed input surface: the order summary at
     rest, the keypad while a field is being edited. A card sale types nothing
     at the counter — the terminal takes it — so it rests on the summary. Cash
     opens straight into the tender field and therefore onto the keypad. */
  it("rests the input column on the order summary when nothing is being typed", () => {
    const html = renderToStaticMarkup(
      <PaymentModal netTotal={220} initialMethod="card" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(html).toContain("Order summary");
    expect(html).toContain("BHD 0.220");
    expect(html).not.toContain('aria-label="Dialpad"');
  });

  it("starts cash checkout on amount received with a visible change preview", () => {
    const html = renderToStaticMarkup(
      <PaymentModal netTotal={220} initialMethod="cash" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(html).toContain("Cash received");
    expect(html).toContain("Change due");
    expect(html).toContain('aria-label="Dialpad"');
    expect(html).toContain("Take BHD 0.220");
    // Change only reads as "live" once there is some to hand back; a permanent
    // green 0.000 trains the eye to skip it.
    expect(html).not.toContain("pm-change-live");
  });

  it("labels deliveries as an exception queue with an actionable empty state", () => {
    const html = renderToStaticMarkup(<DeliveriesTab sessionUser={user} />);
    expect(html).toContain("Delivery queue");
    expect(html).toContain("No deliveries need attention");
  });

  it("keeps delivery transitions distinct for pending orders", () => {
    // The label is translated now; the persisted status identifier is not.
    const en = operationsTranslator("en");
    expect(deliveryStatusActionLabel("dispatched", en)).toBe("Out for delivery");
    expect(deliveryStatusActionLabel("delivered", en)).toBe("Delivered");
  });

  it("translates delivery transition labels rather than hardcoding English", () => {
    const ar = operationsTranslator("ar");
    expect(deliveryStatusActionLabel("dispatched", ar)).not.toMatch(/[A-Za-z]/);
    expect(deliveryStatusActionLabel("delivered", ar)).not.toMatch(/[A-Za-z]/);
  });

  it("gives notes a clear capture-first hierarchy", () => {
    const html = renderToStaticMarkup(<StickyNotesPanel onClose={vi.fn()} />);
    expect(html).toContain("Quick notes");
    expect(html).toContain("Capture a reminder");
    expect(html).toContain("New note");
    expect(html).toContain('role="dialog"');
  });

  it("keeps reminder presets in the operator's local wall-clock time", () => {
    expect(toLocalDateTimeInput(new Date(2026, 6, 24, 22, 30))).toBe("2026-07-24T22:30");
    expect(tomorrowMorningInput(new Date(2026, 6, 24, 22, 30))).toBe("2026-07-25T09:00");
  });

  it("exposes one reports destination and no practice shortcut in the POS rail", () => {
    const html = renderToStaticMarkup(
      <PosSidebar
        visible
        t={key => key}
        canOpenBackOffice
        commerceEnabled
        notifCount={0}
        orderCount={0}
        onOpenReport={vi.fn()}
        onOpenOfficeAI={vi.fn()}
        onOpenNotes={vi.fn()}
        onOpenOrders={vi.fn()}
        onOpenNotifications={vi.fn()}
        onCloseShift={vi.fn()}
        onLogout={vi.fn()}
      />,
    );
    expect(html.match(/>Reports</g)).toHaveLength(1);
    expect(html).not.toContain("X-Report");
    expect(html).not.toContain("practice");
    expect(html).not.toContain("training mode");
  });

  it("presents sales and drawer reconciliation in one POS report view", () => {
    const props: ComponentProps<typeof TodayReportModal> & { shiftId: string; actorUserId: string } = {
      sessionUserId: "u-1",
      shiftId: "shift-1",
      actorUserId: "u-1",
      onClose: vi.fn(),
    };
    const html = renderToStaticMarkup(createElement(TodayReportModal, props));
    expect(html).toContain("POS reports");
    expect(html).toContain("Today’s sales");
    expect(html).toContain("Cash drawer");
  });

  it("places the Practice launch inside sales settings", () => {
    const props: ComponentProps<typeof BusinessTab> & { onStartPractice: () => void } = {
      flags: { allow_negative_stock: false, require_discount_reason: true, cashier_can_discount: false, auto_print_receipt: false },
      setFlags: vi.fn(), taxRules: [], editingRule: null, setEditingRule: vi.fn(),
      taxRuleError: null, setTaxRuleError: vi.fn(), savingFlags: false, savedFlags: false,
      flagsError: null, savingRule: false, handleSaveFlags: vi.fn(), handleSaveTaxRule: vi.fn(),
      handleDeleteTaxRule: vi.fn(), sessionUserId: "u-1", onStartPractice: vi.fn(),
    };
    const html = renderToStaticMarkup(createElement(BusinessTab, props));
    expect(html).toContain("Practice mode");
    expect(html).toContain("Start practice sale");
  });
});
