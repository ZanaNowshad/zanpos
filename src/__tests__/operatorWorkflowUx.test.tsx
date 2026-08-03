import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import CartPanel from "../components/CartPanel";
import DeliveriesTab, { deliveryStatusActionLabel } from "../components/DeliveriesTab";
import PaymentModal from "../components/PaymentModal";
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
      <CartPanel
        cart={cart}
        netTotal={0}
        taxTotal={0}
        onUpdateQty={vi.fn()}
        onRemove={vi.fn()}
        onApplyLineDiscount={vi.fn()}
        onSetLineNote={vi.fn()}
        onPaySplit={vi.fn()}
        onPayFast={vi.fn()}
        onPayDirect={vi.fn()}
        recentLineId={null}
        onBumpLine={vi.fn()}
        compact
      />,
    );
    expect(html).toContain("Ready for the next sale");
    expect(html).toContain("Scan barcode");
    expect(html).toContain("Search");
    expect(html).toContain("<kbd>F2</kbd>");
  });

  it("makes amount due and the completion action explicit in payment", () => {
    const html = renderToStaticMarkup(
      <PaymentModal netTotal={220} initialMethod="card" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(html).toContain("Amount due");
    expect(html).toContain("Complete card sale");
    expect(html).toContain("Optional order details");
    expect(html).toContain('aria-label="Payment method"');
    expect(html).toContain('role="radio"');
    expect(html).toContain('aria-checked="true"');
    expect(html).not.toContain("autofocus");
  });

  it("replaces the dialpad with a readiness summary for exact card payments", () => {
    const html = renderToStaticMarkup(
      <PaymentModal netTotal={220} initialMethod="card" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(html).toContain("Ready to complete");
    expect(html).toContain("Confirm approval on the card terminal");
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
    expect(html).toContain("Complete cash sale");
  });

  it("labels deliveries as an exception queue with an actionable empty state", () => {
    const html = renderToStaticMarkup(<DeliveriesTab sessionUser={user} />);
    expect(html).toContain("Delivery queue");
    expect(html).toContain("No deliveries need attention");
  });

  it("keeps delivery transitions distinct for pending orders", () => {
    expect(deliveryStatusActionLabel("dispatched")).toBe("Out for delivery");
    expect(deliveryStatusActionLabel("delivered")).toBe("Delivered");
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
});
