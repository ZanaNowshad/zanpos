import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BarChart2, Clock, ClipboardList, Building2, Bike, StickyNote,
  Sparkles, ShoppingBag
} from "lucide-react";
import type { BusinessFlags, LowStockAlert, PaymentInput, SaleListRow, SaleResult, SessionUser, Shift } from "../types";
import { type Theme, THEMES } from "../hooks/useTheme";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import { businessFlagsLoad, cashNoSale, receiptReprint, refundGetSale, whatsappStatus, whatsappSendDelivery, appConfigLoad } from "../tauri/commands";
import { loadWaFormat, buildDeliveryMessage } from "../utils/waMessageFormat";
import { useCart } from "../hooks/useCart";
import { useSyncStatus } from "../hooks/useSyncStatus";
import { usePosShortcuts } from "../hooks/usePosShortcuts";
import { useIdleTimer } from "../hooks/useIdleTimer";
import BarcodeInput, { type BarcodeInputHandle } from "../components/BarcodeInput";
import Dialpad, { applyDialpadKey } from "../components/Dialpad";
import CartPanel from "../components/CartPanel";
import BackOfficeModal from "../components/BackOfficeModal";
import DiscountModal from "../components/DiscountModal";
import PaymentModal from "../components/PaymentModal";
import ReceiptPreview from "../components/ReceiptPreview";
import SyncChip from "../components/SyncChip";
import ShiftModal from "../components/ShiftModal";
import HoldModal from "../components/HoldModal";
import RefundModal from "../components/RefundModal";
import TodayReportModal from "../components/TodayReportModal";
import CustomItemModal from "../components/CustomItemModal";
import CashEventModal from "../components/CashEventModal";
import XReportModal from "../components/XReportModal";
import HelpModal from "../components/HelpModal";
import RecentSalesModal from "../components/RecentSalesModal";
import WhatsAppStatusPill from "../components/WhatsAppStatusPill";
import WhatsAppQRModal from "../components/WhatsAppQRModal";
import StickyNotesPanel from "../components/StickyNotesPanel";
import DeliveriesTab from "../components/DeliveriesTab";

interface Props {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenAdminChat?: () => void;
  theme?: Theme;
  onToggleTheme?: () => void;
}

export default function PosPage({
  sessionUser, shift, onLogout, onShiftClose, onOpenAdminChat, theme, onToggleTheme,
}: Props) {
  const session = {
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
  };

  // ── Modal state ───────────────────────────────────────────────────────────────
  const [showPayment, setShowPayment]     = useState(false);
  const [paymentMethod, setPaymentMethod] = useState<PaymentInput["method"] | undefined>(undefined);
  const [paymentSplit, setPaymentSplit]   = useState(false);
  const [saleResult, setSaleResult]       = useState<SaleResult | null>(null);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  const [lastSaleStatus, setLastSaleStatus]       = useState<string | null>(null);
  const [showShiftClose, setShowShiftClose] = useState(false);
  const [showHold, setShowHold]             = useState(false);
  const [showRefund, setShowRefund]         = useState(false);
  const [showReport, setShowReport]         = useState(false);
  const [showDiscount, setShowDiscount]     = useState(false);
  const [showBackOffice, setShowBackOffice] = useState(false);
  const [showCustomItem, setShowCustomItem] = useState(false);
  const [showCashEvent, setShowCashEvent]   = useState(false);
  const [showXReport, setShowXReport]       = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [showReceiptModal, setShowReceiptModal] = useState(false);
  const [isReprintView, setIsReprintView]       = useState(false);
  const [showHelp, setShowHelp]             = useState(false);
  const [showRecent, setShowRecent]         = useState(false);
  const [showWaQR, setShowWaQR]             = useState(false);
  const [showNotes, setShowNotes]           = useState(false);
  const [showDeliveries, setShowDeliveries] = useState(false);
  const [payFastLoading, setPayFastLoading] = useState(false);
  const [restockAlerts, setRestockAlerts]   = useState<LowStockAlert[]>([]);
  const restockTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [bizFlags, setBizFlags] = useState<BusinessFlags>({
    allow_negative_stock: false,
    require_discount_reason: true,
    cashier_can_discount: false,
    auto_print_receipt: false,
  });

  const [numpadValue, setNumpadValue] = useState("1");

  // Saved custom-item suggestions — loaded from localStorage, refreshed after modal closes
  const [suggestions, setSuggestions] = useState<{ id: string; name: string; price: string }[]>(() => {
    try { return JSON.parse(localStorage.getItem("zanpos_custom_suggestions") || "[]"); } catch { return []; }
  });
  const refreshSuggestions = useCallback(() => {
    try { setSuggestions(JSON.parse(localStorage.getItem("zanpos_custom_suggestions") || "[]")); } catch { setSuggestions([]); }
  }, []);

  const canOpenBackOffice = ["owner", "manager"].includes(sessionUser.role_name);
  const canViewXReport    = canOpenBackOffice;
  const canRefund         = ["owner", "manager", "cashier"].includes(sessionUser.role_name);

  // ── Idle auto-lock: log out after 5 minutes of inactivity ────────────────────
  // Prevents unattended terminals from staying authenticated.
  // The timeout fires `onLogout` which returns the user to the PIN screen.
  const IDLE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
  useIdleTimer(IDLE_TIMEOUT_MS, onLogout);

  const syncStatus = useSyncStatus(15_000);

  const {
    cart, loading, error, clearError,
    recentLineId,
    addByBarcode, addProduct, addCustomItem,
    updateQuantity, removeLine, removeRecentLine, bumpRecentQty, bumpLine,
    applyBillDiscount, applyLineDiscount, setLineNote,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session);

  // ── Barcode input ref for programmatic focus ──────────────────────────────────
  const barcodeRef = useRef<BarcodeInputHandle>(null);
  const focusBarcode = useCallback(() => barcodeRef.current?.focus(), []);

  // ── noModalOpen — stable boolean for shortcut guard ───────────────────────────
  // All 14 modal-visibility deps are intentional: every modal that blocks keyboard
  // shortcuts (F-keys, barcode scan, +/−) must be listed here so the guard stays
  // accurate. Adding a new modal? Add its state boolean to both the expression and
  // the dependency array below.
  const noModalOpen = useMemo(() =>
    !showPayment && !showReceiptModal && !showShiftClose &&
    !showHold && !showRefund && !showReport &&
    !showDiscount && !showBackOffice && !showCustomItem &&
    !showCashEvent && !showXReport && !showClearConfirm &&
    !showHelp && !showRecent,
    [showPayment, showReceiptModal, showShiftClose, showHold, showRefund,
     showReport, showDiscount, showBackOffice, showCustomItem,
     showCashEvent, showXReport, showClearConfirm, showHelp, showRecent]
  );

  // ── Pay Fast ─────────────────────────────────────────────────────────────────
  const handlePayFast = useCallback(async () => {
    if (lineCount === 0 || payFastLoading || !noModalOpen) return;
    setPayFastLoading(true);
    try {
      const payment: PaymentInput = {
        method: "cash",
        amount_minor: netTotal,
        tendered_minor: netTotal,
      };
      const result = await finalizeSale([payment]);
      setLastReceiptNumber(result.receipt_number);
      setLastSaleStatus(`✓ #${result.receipt_number} · Cash · ${DEVICE.currency} ${(netTotal / Math.pow(10, DEVICE.currency_exponent)).toFixed(DEVICE.currency_exponent)}`);
      // Auto-print receipt — read fresh flags at point-of-use so Settings changes
      // take effect without requiring a page reload.
      const currentFlags = await businessFlagsLoad().catch(() => bizFlags);
      setBizFlags(currentFlags);
      if (currentFlags.auto_print_receipt) {
        // TODO: call thermalPrintReceipt(result.receipt_number) when that command exists
        // The flag gate is wired — ready for when the thermal receipt print command is built.
      }
      // No ReceiptPreview — cart already cleared in finalizeSale
      focusBarcode();
      if (result.low_stock_alerts.length > 0) {
        if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
        setRestockAlerts(result.low_stock_alerts);
        restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
      }
    } catch {
      // error is set in useCart; cart remains intact
    } finally {
      setPayFastLoading(false);
    }
  }, [lineCount, payFastLoading, noModalOpen, netTotal, finalizeSale, focusBarcode]);

  // ── Direct payment (opens modal pre-configured to method) ─────────────────────
  const openPayDirect = useCallback((method: PaymentInput["method"]) => {
    if (lineCount === 0) return;
    setPaymentMethod(method);
    setPaymentSplit(false);
    setShowPayment(true);
  }, [lineCount]);

  const openPaySplit = useCallback(() => {
    if (lineCount === 0) return;
    setPaymentMethod(undefined);
    setPaymentSplit(true);
    setShowPayment(true);
  }, [lineCount]);

  // openPay — generic payment modal (no pre-selected method, no split).
  // Not shown as a visible button; invoked via the F9 keyboard shortcut in
  // usePosShortcuts. Kept separate from openPayDirect / openPaySplit so the
  // shortcut remains available even after the footer buttons were removed.
  const openPay = useCallback(() => {
    if (lineCount === 0) return;
    setPaymentMethod(undefined);
    setPaymentSplit(false);
    setShowPayment(true);
  }, [lineCount]);

  // ── Confirm payment ───────────────────────────────────────────────────────────
  const handleConfirmPayment = async (payments: PaymentInput[], customerId?: string, deliveryInput?: import("../types").DeliveryInput) => {
    try {
      const result = await finalizeSale(payments, customerId, deliveryInput);
      setShowPayment(false);
      setShowReceiptModal(false);   // banner only — not blocking modal
      setSaleResult(result);
      setLastReceiptNumber(result.receipt_number);
      setLastSaleStatus(`✓ #${result.receipt_number}`);
      // Auto-print receipt — read fresh flags at point-of-use so Settings changes
      // take effect without requiring a page reload.
      const currentFlags = await businessFlagsLoad().catch(() => bizFlags);
      setBizFlags(currentFlags);
      if (currentFlags.auto_print_receipt) {
        // TODO: call thermalPrintReceipt(result.receipt_number) when that command exists
        // The flag gate is wired — ready for when the thermal receipt print command is built.
      }
      focusBarcode();               // cart is clear — cashier can scan immediately
      if (result.low_stock_alerts.length > 0) {
        if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
        setRestockAlerts(result.low_stock_alerts);
        restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
      }
      // ── WhatsApp delivery message ────────────────────────────────────────────
      if (result.delivery && result.delivery.contact_number) {
        const d = result.delivery;
        const sendWA = async () => {
          try {
            const waStatus = await whatsappStatus();
            if (waStatus.connected) {
              // Build message from custom template (falls back to Rust builder if not set)
              let messageOverride: string | undefined;
              try {
                const fmt = loadWaFormat();
                const cfg = await appConfigLoad();
                const activeLines = fmt.language === "ar" ? fmt.ar_lines : fmt.en_lines;
                const now = new Date();
                const dateStr = now.toLocaleDateString(fmt.language === "ar" ? "ar-BH" : "en-GB", {
                  day: "numeric", month: "long", year: "numeric",
                }) + ", " + now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
                const method = result.payments[0]?.method ?? "cash";
                const methodLabel = method === "wallet" ? "BenefitPay" : method.charAt(0).toUpperCase() + method.slice(1);
                const vars = {
                  customer_name:  d.customer_name ?? "",
                  receipt_number: result.receipt_number,
                  date:           dateStr,
                  amount:         `${DEVICE.currency} ${formatMoney(result.net_total_minor, DEVICE.currency_exponent)}`,
                  address:        d.address_text,
                  house_number:   d.house_number ?? "",
                  area:           d.area ?? "",
                  delivery_note:  d.delivery_note ?? "",
                  method:         methodLabel,
                  benefit_number: cfg.whatsapp_benefit_number ?? "",
                  store_name:     DEVICE.branch_name,
                  store_phone:    "",
                };
                messageOverride = buildDeliveryMessage(activeLines, vars, result.items, DEVICE.currency_exponent);
              } catch { /* if template build fails, fall through to Rust builder */ }

              await whatsappSendDelivery({
                to:                d.contact_number,
                receipt_number:    result.receipt_number,
                net_total_minor:   result.net_total_minor,
                currency_exponent: DEVICE.currency_exponent,
                address_text:      d.address_text,
                house_number:      d.house_number ?? undefined,
                area:              d.area ?? undefined,
                message_override:  messageOverride,
              });
            } else if (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") {
              setShowWaQR(true);
            }
            // cashier + disconnected: silent skip
          } catch { /* never block the sale */ }
        };
        sendWA(); // fire-and-forget — never block the receipt flow
      }
    } catch {
      // error is set in useCart; modal stays open
    }
  };

  const handleNewSale = useCallback(() => {
    setSaleResult(null);
    setShowReceiptModal(false);
    clearCart();   // no-op after finalize (cart already empty), handles reprint path
    focusBarcode();
  }, [clearCart, focusBarcode]);

  const handleReprintLast = useCallback(async () => {
    if (!lastReceiptNumber) return;
    try {
      const reprinted = await receiptReprint(lastReceiptNumber, sessionUser.user_id);
      setSaleResult(reprinted);
      setIsReprintView(true);
      setShowReceiptModal(true);   // reprint always opens full modal
    } catch (e: unknown) {
      console.error("Reprint failed", e);
    }
  }, [lastReceiptNumber, sessionUser.user_id]);

  const handleNoSale = useCallback(async () => {
    try { await cashNoSale(shift.shift_id, sessionUser.user_id); }
    catch (e) { console.error("No-sale audit failed:", e); }
  }, [shift.shift_id, sessionUser.user_id]);

  // ── Edit a past sale (load items back into cart as custom items) ──────────────
  // Items are added in parallel (Promise.allSettled) to avoid O(n) sequential
  // IPC round-trips. allSettled ensures a single failed item doesn't leave the
  // cart partially populated silently — failures are surfaced via console.error.
  const handleEditSale = useCallback(async (sale: SaleListRow) => {
    try {
      const detail = await refundGetSale(sale.receipt_number, sessionUser.user_id);
      clearCart();
      const results = await Promise.allSettled(
        detail.items.map(item => {
          const priceMajor = formatMoney(item.unit_price_minor, DEVICE.currency_exponent);
          return addCustomItem(item.product_name_snapshot, priceMajor, item.quantity);
        })
      );
      const failed = results.filter(r => r.status === "rejected");
      if (failed.length > 0) {
        console.error(`Edit sale: ${failed.length}/${detail.items.length} items failed to load`, failed);
      }
      setShowRecent(false);
      focusBarcode();
    } catch (e) {
      console.error("Failed to load sale for edit", e);
    }
  }, [clearCart, addCustomItem, focusBarcode, sessionUser.user_id]);

  // ── Clear cart with confirmation ──────────────────────────────────────────────
  const handleClearCartRequest = useCallback(() => {
    if (lineCount === 0) return;
    setShowClearConfirm(true);
  }, [lineCount]);

  const handleClearConfirmed = useCallback(() => {
    clearCart();
    setShowClearConfirm(false);
    focusBarcode();
  }, [clearCart, focusBarcode]);

  // ── Recent item controls ──────────────────────────────────────────────────────
  const handleIncrementRecent = useCallback(() => bumpRecentQty(1),  [bumpRecentQty]);
  const handleDecrementRecent = useCallback(() => bumpRecentQty(-1), [bumpRecentQty]);

  // ── Hold / Resume ─────────────────────────────────────────────────────────────
  const handleOpenHold = useCallback(() => setShowHold(true), []);

  // ── Barcode scan handler ──────────────────────────────────────────────────────
  const handleBarcode = useCallback(async (barcode: string, qty?: number) => {
    // If BarcodeInput already parsed a "3*barcode" prefix, use that qty.
    // Otherwise use numpadValue as the pending multiplier.
    const effectiveQty = qty ?? (parseInt(numpadValue) || 1);
    try {
      await addByBarcode(barcode, effectiveQty);
      barcodeRef.current?.flashSuccess();
      setNumpadValue("1"); // reset after successful scan
    } catch {
      barcodeRef.current?.flashError();
    }
  }, [addByBarcode, numpadValue]);

  // ── Shortcut manager ─────────────────────────────────────────────────────────
  usePosShortcuts({
    noModalOpen,
    lineCount,
    hasRecentLine: recentLineId !== null,
    lastReceiptNumber,
    onFocusBarcode:      focusBarcode,
    onHold:              handleOpenHold,
    onResumeHeld:        handleOpenHold,
    onPay:               openPay,
    onPayFast:           handlePayFast,
    onDiscount:          () => setShowDiscount(true),
    onRefund:            () => canRefund && setShowRefund(true),
    onClearCart:         handleClearCartRequest,
    onReprintLast:       handleReprintLast,
    onNoSale:            handleNoSale,
    onXReport:           canViewXReport ? () => setShowXReport(true) : undefined,
    onIncrementRecent:   handleIncrementRecent,
    onDecrementRecent:   handleDecrementRecent,
    onRemoveRecent:      removeRecentLine,
    onLock:              onLogout,
    onReport:            () => setShowReport(true),
    onCustomItem:        () => setShowCustomItem(true),
    onHelp:              () => setShowHelp(true),
  });

  // ── Focus barcode after any modal closes ──────────────────────────────────────
  useEffect(() => {
    if (noModalOpen) focusBarcode();
  }, [noModalOpen, focusBarcode]);

  // ── Load business flags on mount ─────────────────────────────────────────────
  useEffect(() => {
    businessFlagsLoad()
      .then(f => setBizFlags(f))
      .catch(() => {}); // non-fatal — defaults apply
  }, []);

  // ── Clock ─────────────────────────────────────────────────────────────────────
  const [clockTime, setClockTime] = useState(() =>
    new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
  );
  useEffect(() => {
    const t = setInterval(() =>
      setClockTime(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })),
      10_000
    );
    return () => clearInterval(t);
  }, []);

  // ── Numpad key handler ────────────────────────────────────────────────────────
  const handleNumpadKey = useCallback((key: string) => {
    // "C" clears back to "1"
    if (key === "C") {
      setNumpadValue("1");
      return;
    }
    const next = applyDialpadKey(numpadValue === "1" && key !== "⌫" ? "" : numpadValue, key);
    // Keep value at minimum "1" visually, but store "" as "1" on confirm
    const clamped = next === "" ? "1" : next;
    setNumpadValue(clamped);
    // Live-update recent cart line qty
    if (recentLineId && next !== "" && next !== "0") {
      updateQuantity(recentLineId, next);
    }
  }, [numpadValue, recentLineId, updateQuantity]);

  // ── Sync status helpers ───────────────────────────────────────────────────────
  const isOnline      = syncStatus?.online ?? false;
  const pendingEvents = syncStatus?.pending_events ?? 0;

  return (
    <div className={`pos-layout ${lineCount > 0 ? "pos-has-cart" : "pos-idle"} ${showPayment || payFastLoading ? "pos-payment-started" : ""} ${!isOnline ? "pos-offline" : "pos-online"}`}>
      {/* ── Top bar ── */}
      <div className="top-bar">
        {/* Left: brand */}
        <div className="top-bar-left">
          <span className="top-bar-logo">ZAN<span>POS</span></span>
          <span className="top-bar-sep">·</span>
          <span className="top-bar-branch">{DEVICE.branch_name}</span>
        </div>

        {/* Centre: operational status pills */}
        <div className="top-bar-center">
          <span className="top-bar-pill top-bar-pill-success">Shift Open</span>
          <SyncChip status={syncStatus} />
          <WhatsAppStatusPill
            sessionRole={sessionUser.role_name}
            onOpenQR={() => setShowWaQR(true)}
          />
        </div>

        {/* Right: time + user + actions */}
        <div className="top-bar-right">
          <span className="top-bar-time">{clockTime}</span>
          <span className="top-bar-cashier">{sessionUser.display_name}</span>
          {lastReceiptNumber && (
            <button className="top-bar-btn" onClick={handleReprintLast} title={`Reprint #${lastReceiptNumber} (Ctrl+P)`}>
              Reprint
            </button>
          )}
          {onToggleTheme && theme && (() => {
            const meta   = THEMES.find(t => t.id === theme)!;
            const idx    = THEMES.findIndex(t => t.id === theme);
            const next   = THEMES[(idx + 1) % THEMES.length];
            return (
              <button
                className="top-bar-btn top-bar-theme"
                onClick={onToggleTheme}
                title={`Theme: ${meta.label} — click for ${next.label}`}
              >
                {meta.icon} {meta.label}
              </button>
            );
          })()}
          <button className="top-bar-btn top-bar-btn-danger" onClick={() => setShowShiftClose(true)}>
            Close Shift
          </button>
          <button className="top-bar-btn top-bar-logout" onClick={onLogout} title="Ctrl+L">
            Logout
          </button>
        </div>
      </div>

      {/* ── Error banner ── */}
      {error && (
        <div className="error-banner" onClick={clearError} role="alert">
          ⚠ {error}
          <button className="error-banner-action" onClick={(e) => { e.stopPropagation(); clearError(); focusBarcode(); }}>
            Dismiss & focus scan
          </button>
          <span className="error-dismiss" onClick={clearError}>✕</span>
        </div>
      )}

      {/* ── Main area ── */}
      <div className="pos-main" style={{ gridTemplateColumns: "88px minmax(0, 1fr) 300px" }}>
        {/* Icon sidebar */}
        <div className="pos-sidebar">
          <button className="pos-sidebar-item active" title="Quick Sale">
            <ShoppingBag size={18} strokeWidth={1.75} />
            <span>Sale</span>
          </button>
          <div className="pos-sidebar-divider" />
          <button className="pos-sidebar-item" title="Today's Report" onClick={() => setShowReport(true)}>
            <BarChart2 size={18} strokeWidth={1.75} />
            <span>Reports</span>
          </button>
          <button className="pos-sidebar-item" title="Recent Sales — reprint or void" onClick={() => setShowRecent(true)}>
            <Clock size={18} strokeWidth={1.75} />
            <span>Recent</span>
          </button>
          {canViewXReport && (
            <button className="pos-sidebar-item" title="X-Report — mid-shift drawer check" onClick={() => setShowXReport(true)}>
              <ClipboardList size={18} strokeWidth={1.75} />
              <span>X-Report</span>
            </button>
          )}
          {canOpenBackOffice && (
            <button className="pos-sidebar-item" title="Back Office" onClick={() => setShowBackOffice(true)}>
              <Building2 size={18} strokeWidth={1.75} />
              <span>Back Office</span>
            </button>
          )}
          <button className="pos-sidebar-item" title="Deliveries" onClick={() => setShowDeliveries(true)}>
            <Bike size={18} strokeWidth={1.75} />
            <span>Deliveries</span>
          </button>
          {canOpenBackOffice && (
            <button className="pos-sidebar-item" title="Notes (admin)" onClick={() => setShowNotes(true)}>
              <StickyNote size={18} strokeWidth={1.75} />
              <span>Notes</span>
            </button>
          )}
          <div className="pos-sidebar-spacer" />
          {onOpenAdminChat && (
            <button className="pos-sidebar-item" title="AI Admin" onClick={onOpenAdminChat}>
              <Sparkles size={18} strokeWidth={1.75} />
              <span>AI</span>
            </button>
          )}
        </div>

        {/* Cart column — scan strip + custom item btn + full cart */}
        <div className="pos-cart-col">
          <BarcodeInput
            ref={barcodeRef}
            onBarcode={handleBarcode}
            onSelectProduct={async (product) => {
              const qty = parseInt(numpadValue) > 1 ? numpadValue : undefined;
              try {
                await addProduct(product, qty);
                barcodeRef.current?.flashSuccess();
                setNumpadValue("1");
              } catch {
                barcodeRef.current?.flashError();
              }
            }}
            onSearch={() => {}}
            onEscape={() => {}}
            disabled={loading || payFastLoading}
          />
          {/* Quick-add strip: Custom Item chip + saved suggestion chips */}
          <div className="pos-quickadd-strip">
            <button
              className="pos-quickadd-custom"
              onClick={() => setShowCustomItem(true)}
              disabled={loading || payFastLoading}
              title="Add a custom item"
            >✦ Custom</button>
            {suggestions.map(s => (
              <button
                key={s.id}
                className="pos-quickadd-chip"
                disabled={loading || payFastLoading}
                onClick={async () => {
                  await addCustomItem(s.name, s.price, numpadValue);
                  setNumpadValue("1");
                  focusBarcode();
                }}
                title={`${s.name} — ${DEVICE.currency} ${s.price}`}
              >
                <span className="pqc-name">{s.name}</span>
                <span className="pqc-price">{s.price}</span>
              </button>
            ))}
          </div>
          <CartPanel
            cart={cart}
            netTotal={netTotal}
            taxTotal={taxTotal}
            onUpdateQty={updateQuantity}
            onRemove={removeLine}
            onApplyLineDiscount={applyLineDiscount}
            onSetLineNote={setLineNote}
            onPaySplit={openPaySplit}
            onPayFast={handlePayFast}
            onPayDirect={openPayDirect}
            payFastLoading={payFastLoading}
            paymentStarted={showPayment}
            recentLineId={recentLineId}
            onBumpLine={bumpLine}
            compact={true}
          />
        </div>

        {/* Numpad panel */}
        <div className="numpad-panel">
          <div className="numpad-display">
            <span className="numpad-multiplier">× {numpadValue}</span>
            {recentLineId && (
              <span className="numpad-recent-name">
                {cart.lines.find(l => l.cart_line_id === recentLineId && !l.voided)?.product_name}
              </span>
            )}
          </div>
          <Dialpad onKey={handleNumpadKey} />

          {/* Totals + Payment */}
          {(() => {
            const activeLines = cart.lines.filter(l => !l.voided);
            const grossTotal = activeLines.reduce((s, l) => s + l.line_total_minor, 0);
            const totalDiscount = cart.bill_discount_minor + activeLines.reduce((s, l) => s + l.line_discount_minor, 0);
            const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;
            const canPay = activeLines.length > 0 && netTotal > 0 && !(payFastLoading || showPayment);
            return (
              <div className="np-pay-section">
                {/* Totals */}
                <div className="np-totals">
                  <div className="np-total-row"><span>Subtotal</span><span>{fmt(grossTotal)}</span></div>
                  {totalDiscount > 0
                    ? <div className="np-total-row np-discount-active"><span>Discount</span><span>−{fmt(totalDiscount)}</span></div>
                    : <div className="np-total-row np-discount-zero"><span>Discount</span><span>{fmt(0)}</span></div>
                  }
                  <div className="np-total-row"><span>Tax</span><span>{fmt(taxTotal)}</span></div>
                  <div className="np-total-row np-grand"><span>TOTAL</span><span>{fmt(netTotal)}</span></div>
                </div>
                {/* Fast Cash full-width */}
                <button
                  className="np-fast-cash-btn"
                  disabled={!canPay || payFastLoading}
                  onClick={handlePayFast}
                  title={!canPay ? "Add items to pay" : "Fast Cash · F12"}
                >
                  {payFastLoading ? "…" : <><span>Fast Cash <kbd>F12</kbd></span><span className="np-fast-total">{fmt(netTotal)}</span></>}
                </button>
                {/* Cash / Card / Wallet / Split */}
                <div className="np-methods">
                  <button className="np-method-btn" disabled={!canPay} onClick={() => openPayDirect("cash")}>Cash</button>
                  <button className="np-method-btn" disabled={!canPay} onClick={() => openPayDirect("card")}>Card</button>
                  <button className="np-method-btn" disabled={!canPay} onClick={() => openPayDirect("wallet")}>Wallet</button>
                  <button className="np-method-btn np-split-btn" disabled={!canPay} onClick={openPaySplit}>Split</button>
                </div>
              </div>
            );
          })()}
        </div>
      </div>

      {/* ── Action bar ── */}
      <div className="action-bar">
        <div className="action-group action-group-transaction">
          <button
            className="action-btn action-btn-danger"
            onClick={handleClearCartRequest}
            disabled={lineCount === 0}
            title="Clear cart — Ctrl+Delete"
          >
            Clear <kbd>Ctrl+⌫</kbd>
          </button>
          <button
            className="action-btn"
            onClick={() => setShowHold(true)}
            title="Hold current order or resume a held order — F6"
          >
            Hold / Resume <kbd>F6</kbd>
          </button>
        </div>
        <div className="action-group action-group-modifiers">
          <button
            className="action-btn"
            onClick={() => setShowDiscount(true)}
            disabled={lineCount === 0}
            title="Apply bill discount — F8"
          >
            Discount <span className="action-lock">🔒</span> <kbd>F8</kbd>
          </button>
          {canRefund && (
            <button
              className="action-btn action-btn-danger"
              onClick={() => setShowRefund(true)}
              title="Process a refund — Ctrl+R"
            >
              Refund <span className="action-lock">🔒</span> <kbd>F10</kbd>
            </button>
          )}
        </div>
        <div className="action-group action-group-operational">
          <button
            className="action-btn"
            onClick={() => setShowCashEvent(true)}
            title="Cash In / Out / Safe Drop"
          >
            Cash Event <span className="action-lock">🔒</span>
          </button>
          <button
            className="action-btn"
            onClick={handleNoSale}
            title="Open drawer without sale — F11"
          >
            No Sale <span className="action-lock">🔒</span> <kbd>F11</kbd>
          </button>
        </div>
      </div>

      {/* ── Status bar ── */}
      <div className="pos-status-bar">
        <span className="status-cashier">👤 {sessionUser.display_name}</span>
        <span className="status-sep">·</span>
        <span className="status-branch">{DEVICE.branch_name}</span>
        <span className="status-sep">·</span>
        <span className="status-device" title={`Device: ${DEVICE.device_id}`}>
          {DEVICE.device_id.substring(0, 8)}…
        </span>
        <span className="status-sep">·</span>
        <span className="status-shift-open">⬤ Shift Open</span>
        <span className="status-sep">·</span>
        <span className={`status-sync ${isOnline ? "status-online" : "status-offline"}`}>
          {isOnline ? "⬤ Online" : "⬤ Offline"}
          {pendingEvents > 0 && <span className="status-pending"> · {pendingEvents} pending</span>}
        </span>
        {lastSaleStatus && (
          <>
            <span className="status-sep">·</span>
            <span className="status-last-sale">{lastSaleStatus}</span>
          </>
        )}
        <span className="status-spacer" />
        <span className="status-shortcuts">
          <kbd>F2</kbd> Scan &nbsp;
          <kbd>F12</kbd> Fast Cash &nbsp;
          <kbd>F6</kbd> Hold &nbsp;
          <kbd>F8</kbd> Discount &nbsp;
          <kbd>+</kbd><kbd>−</kbd> Qty &nbsp;
          <kbd>Ctrl+H</kbd> Help
        </span>
      </div>

      {/* ── Clear cart confirmation ── */}
      {showClearConfirm && (
        <div className="modal-overlay" onClick={() => { setShowClearConfirm(false); focusBarcode(); }}>
          <div className="modal clear-confirm-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <span className="modal-title">Clear Cart?</span>
            </div>
            <p className="clear-confirm-body">
              Remove all {lineCount} item{lineCount !== 1 ? "s" : ""} from the cart?
              This will be recorded as a pre-tender void.
            </p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => { setShowClearConfirm(false); focusBarcode(); }}>
                Cancel
              </button>
              <button className="btn-danger" onClick={handleClearConfirmed}>
                🗑 Clear Cart
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Modals ── */}
      {showCustomItem && (
        <CustomItemModal
          onAdd={async (name, price, qty) => {
            await addCustomItem(name, price, qty);
            setShowCustomItem(false);
            refreshSuggestions();
            focusBarcode();
          }}
          onCancel={() => { setShowCustomItem(false); refreshSuggestions(); focusBarcode(); }}
        />
      )}

      {showCashEvent && (
        <CashEventModal
          shiftId={shift.shift_id}
          userId={sessionUser.user_id}
          cashierName={sessionUser.display_name}
          onDone={() => { setShowCashEvent(false); focusBarcode(); }}
          onCancel={() => { setShowCashEvent(false); focusBarcode(); }}
        />
      )}

      {showDiscount && (
        <DiscountModal
          grossMinor={cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.line_total_minor, 0)}
          currentDiscountMinor={cart.bill_discount_minor}
          onApply={async (discount_minor, reason) => {
            await applyBillDiscount(discount_minor, reason);
            setShowDiscount(false);
            focusBarcode();
          }}
          onCancel={() => { setShowDiscount(false); focusBarcode(); }}
        />
      )}

      {showPayment && (
        <PaymentModal
          netTotal={netTotal}
          initialMethod={paymentMethod}
          splitMode={paymentSplit}
          onConfirm={handleConfirmPayment}
          onCancel={() => { setShowPayment(false); focusBarcode(); }}
          loading={loading}
          sessionUserId={sessionUser.user_id}
        />
      )}

      {/* ── Post-sale success banner (non-blocking) ── */}
      {saleResult && !showReceiptModal && (
        <div className="sale-banner">
          <span className="sale-banner-icon">✓</span>
          <span className="sale-banner-text">Sale #{saleResult.receipt_number}</span>
          <button className="sale-banner-print" onClick={() => { setIsReprintView(false); setShowReceiptModal(true); }}>
            🖨 Print Receipt
          </button>
          <button className="sale-banner-dismiss" onClick={handleNewSale} title="Dismiss">×</button>
        </div>
      )}

      {/* ── Full receipt modal (explicit print or reprint) ── */}
      {saleResult && showReceiptModal && (
        <ReceiptPreview
          sale={saleResult}
          isReprint={isReprintView}
          onNewSale={() => { setShowReceiptModal(false); setIsReprintView(false); handleNewSale(); }}
        />
      )}

      {showShiftClose && (
        <ShiftModal
          mode="close"
          user={sessionUser}
          shift={shift}
          onShiftOpened={() => {}}
          onShiftClosed={() => { setShowShiftClose(false); onShiftClose(true); }}
          onCancel={() => { setShowShiftClose(false); focusBarcode(); }}
        />
      )}

      {showHold && (
        <HoldModal
          cart={cart}
          lineCount={lineCount}
          netTotal={netTotal}
          onHeld={() => { setShowHold(false); clearCart(); focusBarcode(); }}
          onResume={(resumed) => { setShowHold(false); replaceCart(resumed); focusBarcode(); }}
          onClose={() => { setShowHold(false); focusBarcode(); }}
        />
      )}

      {showRefund && (
        <RefundModal
          cashierUserId={sessionUser.user_id}
          onClose={() => { setShowRefund(false); focusBarcode(); }}
        />
      )}

      {showReport && (
        <TodayReportModal onClose={() => { setShowReport(false); focusBarcode(); }} />
      )}

      {showBackOffice && (
        <BackOfficeModal
          sessionUser={sessionUser}
          onClose={() => { setShowBackOffice(false); focusBarcode(); }}
        />
      )}

      {showXReport && (
        <XReportModal
          shiftId={shift.shift_id}
          actorUserId={sessionUser.user_id}
          onClose={() => { setShowXReport(false); focusBarcode(); }}
        />
      )}

      {showHelp && (
        <HelpModal onClose={() => { setShowHelp(false); focusBarcode(); }} />
      )}

      {showRecent && (
        <RecentSalesModal
          onReprint={async (receiptNumber) => {
            try {
              const reprinted = await receiptReprint(receiptNumber, sessionUser.user_id);
              setSaleResult(reprinted);
              setIsReprintView(true);
              setShowReceiptModal(true);
              setShowRecent(false);
            } catch (e) { console.error("Reprint failed", e); }
          }}
          onEdit={handleEditSale}
          onClose={() => { setShowRecent(false); focusBarcode(); }}
        />
      )}

      {showWaQR && (
        <WhatsAppQRModal onClose={() => setShowWaQR(false)} />
      )}

      {showNotes && (
        <StickyNotesPanel onClose={() => setShowNotes(false)} />
      )}

      {showDeliveries && (
        <div className="dlv-modal-overlay">
          <div className="dlv-modal-shell">
            <div className="dlv-modal-header">
              <span className="dlv-modal-title">🛵 Deliveries</span>
              <button className="dlv-modal-close" onClick={() => { setShowDeliveries(false); focusBarcode(); }}>✕</button>
            </div>
            <div className="dlv-modal-body">
              <DeliveriesTab sessionUser={sessionUser} />
            </div>
          </div>
        </div>
      )}

      {/* ── Restock alerts toast ── */}
      {restockAlerts.length > 0 && (
        <div
          className="restock-toast-overlay"
          onClick={() => { if (restockTimerRef.current) clearTimeout(restockTimerRef.current); setRestockAlerts([]); }}
        >
          {restockAlerts.map(alert => (
            <div key={alert.product_id} className="restock-toast">
              <span className="restock-toast-title">⚠ Low Stock</span>
              <span className="restock-toast-body">
                {alert.product_name}: {alert.quantity_on_hand} left (reorder at {alert.reorder_point})
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
