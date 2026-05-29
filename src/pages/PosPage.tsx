import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BarChart2, Clock, ClipboardList, Building2, Bike, StickyNote,
  Sparkles, ShoppingBag
} from "lucide-react";
import type { BusinessFlags, CustomerRow, LowStockAlert, PaymentInput, SaleListRow, SaleResult, SessionUser, Shift } from "../types";
import { type Theme, THEMES } from "../hooks/useTheme";
import { formatMoney } from "../money";
import { DEVICE } from "../types";
import { businessFlagsLoad, cashNoSale, ghostRecord, receiptReprint, refundGetSale, whatsappStatus, whatsappSendDelivery, appConfigLoad, openCashDrawer, settingsGetBranch, thermalGetConfig, printReceiptRaw } from "../tauri/commands";
import { buildReceiptLines } from "../utils/receiptLines";
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

// ── Modal state machine ───────────────────────────────────────────────────────
// A discriminated union ensures only ONE blocking modal can be active at a time,
// eliminating the class of bugs where two modal flags are simultaneously true.
// Secondary overlays that don't block shortcuts (WaQR, Notes, Deliveries) remain
// as separate booleans because they are intentionally non-exclusive.
type ActiveModal =
  | { kind: "none" }
  | { kind: "payment"; method?: PaymentInput["method"]; split: boolean }
  | { kind: "receipt"; isReprint: boolean; result: SaleResult }
  | { kind: "shiftClose" }
  | { kind: "hold" }
  | { kind: "refund" }
  | { kind: "report" }
  | { kind: "discount" }
  | { kind: "backOffice" }
  | { kind: "customItem" }
  | { kind: "cashEvent" }
  | { kind: "xReport" }
  | { kind: "clearConfirm" }
  | { kind: "help" }
  | { kind: "recent" };

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
  const session = useMemo(() => ({
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
  }), [shift.shift_id, sessionUser.user_id]);

  // ── Sidebar visibility — persisted across sessions ────────────────────────────
  const [showSidebar, setShowSidebar] = useState<boolean>(() =>
    localStorage.getItem("zanpos_sidebar") === "1"
  );
  const toggleSidebar = useCallback(() => {
    setShowSidebar(v => {
      const next = !v;
      localStorage.setItem("zanpos_sidebar", next ? "1" : "0");
      return next;
    });
  }, []);

  // ── Modal state ───────────────────────────────────────────────────────────────
  // Single blocking-modal slot — only one variant active at a time.
  const [activeModal, setActiveModal] = useState<ActiveModal>({ kind: "none" });
  // Non-blocking post-sale banner (shown alongside the POS, not over it).
  const [bannerResult, setBannerResult] = useState<SaleResult | null>(null);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  // Secondary overlays — non-exclusive, do not block keyboard shortcuts.
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
  // Cached branch settings + thermal config — loaded once on mount,
  // used to auto-print the receipt after every non-fast-cash checkout.
  const [branchSettings, setBranchSettings] = useState<import("../types").BranchSettings | null>(null);
  const [thermalEnabled, setThermalEnabled] = useState(false);

  const [numpadValue, setNumpadValue] = useState("1");
  const numpadRef = useRef(numpadValue);
  useEffect(() => { numpadRef.current = numpadValue; }, [numpadValue]);

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
  const barcodeRef  = useRef<BarcodeInputHandle>(null);
  /** Serialises rapid USB scanner submissions — each scan awaits the previous. */
  const scanQueueRef = useRef<Promise<void>>(Promise.resolve());
  const focusBarcode = useCallback(() => barcodeRef.current?.focus(), []);

  // ── noModalOpen — stable boolean for shortcut guard ───────────────────────────
  // Because only one ActiveModal variant can be active at a time, this is now
  // trivially derived from the union.  Adding a new blocking modal? Just add its
  // variant to the ActiveModal union — no need to update this expression.
  const noModalOpen = activeModal.kind === "none";

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
      // Open cash drawer — best-effort, non-fatal
      openCashDrawer().catch(() => {});
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
    setActiveModal({ kind: "payment", method, split: false });
  }, [lineCount]);

  const openPaySplit = useCallback(() => {
    if (lineCount === 0) return;
    setActiveModal({ kind: "payment", method: undefined, split: true });
  }, [lineCount]);

  // openPay — generic payment modal (no pre-selected method, no split).
  // Not shown as a visible button; invoked via the F9 keyboard shortcut in
  // usePosShortcuts. Kept separate from openPayDirect / openPaySplit so the
  // shortcut remains available even after the footer buttons were removed.
  const openPay = useCallback(() => {
    if (lineCount === 0) return;
    setActiveModal({ kind: "payment", method: undefined, split: false });
  }, [lineCount]);

  // ── Confirm payment ───────────────────────────────────────────────────────────
  const handleConfirmPayment = async (payments: PaymentInput[], customerId?: string, deliveryInput?: import("../types").DeliveryInput, selectedCustomer?: CustomerRow) => {
    try {
      const result = await finalizeSale(payments, customerId, deliveryInput);
      setActiveModal({ kind: "none" });
      setBannerResult(result);
      setLastReceiptNumber(result.receipt_number);
      // Open cash drawer if any payment was cash — best-effort, non-fatal
      if (payments.some(p => p.method === "cash")) {
      openCashDrawer().catch((e: unknown) => console.warn("Cash drawer open failed:", e));
      }
      // ── Auto-print receipt to thermal printer ────────────────────────────────
      // Fires for every checkout path EXCEPT Fast Cash (handlePayFast is
      // separate and intentionally skips printing for maximum speed).
      // Non-fatal: a printer error never rolls back the sale.
      if (thermalEnabled) {
        printReceiptRaw(
          result.branch_name,
          buildReceiptLines(result, branchSettings, false),
        ).catch((e: unknown) => console.warn("Receipt print failed:", e)); // fire-and-forget
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

      // ── WhatsApp receipt confirmation to selected customer ───────────────────
      // Only fires when a customer with a phone number is selected at checkout
      // AND the sale is NOT a delivery (delivery already gets its own WA message).
      if (selectedCustomer?.phone && !deliveryInput) {
        const sendCustomerWA = async () => {
          try {
            const waStatus = await whatsappStatus();
            if (!waStatus.connected) return; // silent skip
            const cfg = await appConfigLoad();
            const phone = selectedCustomer.phone!;
            // Normalise to Bahrain E.164: prepend 973 if not already prefixed
            const to = /^97[0-9]/.test(phone) ? phone : `973${phone}`;
            const now = new Date();
            const dateStr = now.toLocaleDateString("en-GB", {
              day: "numeric", month: "long", year: "numeric",
            }) + ", " + now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
            const method = result.payments[0]?.method ?? "cash";
            const methodLabel = method === "wallet" ? "BenefitPay" : method.charAt(0).toUpperCase() + method.slice(1);
            const amtStr = `${DEVICE.currency} ${formatMoney(result.net_total_minor, DEVICE.currency_exponent)}`;
            const benefitLine = (method === "wallet" && cfg.whatsapp_benefit_number)
              ? `\nBenefitPay: ${cfg.whatsapp_benefit_number}` : "";
            const message =
              `✅ Thank you, ${selectedCustomer.name}!\n` +
              `Receipt #${result.receipt_number}\n` +
              `Date: ${dateStr}\n` +
              `Amount: ${amtStr}\n` +
              `Paid by: ${methodLabel}${benefitLine}\n` +
              `─────────────────\n` +
              `شكرًا لك، ${selectedCustomer.name}!\n` +
              `إيصال رقم ${result.receipt_number}\n` +
              `المبلغ: ${amtStr}\n` +
              `طريقة الدفع: ${methodLabel}`;
            await whatsappSendDelivery({
              to,
              receipt_number:    result.receipt_number,
              net_total_minor:   result.net_total_minor,
              currency_exponent: DEVICE.currency_exponent,
              address_text:      "",
              message_override:  message,
            });
          } catch { /* never block the receipt flow */ }
        };
        sendCustomerWA(); // fire-and-forget
      }
    } catch {
      // error is set in useCart; modal stays open
    }
  };

  const handleNewSale = useCallback(() => {
    setBannerResult(null);
    setActiveModal({ kind: "none" });
    clearCart();   // no-op after finalize (cart already empty), handles reprint path
    focusBarcode();
  }, [clearCart, focusBarcode]);

  const handleReprintLast = useCallback(async () => {
    if (!lastReceiptNumber) return;
    try {
      const reprinted = await receiptReprint(lastReceiptNumber, sessionUser.user_id);
      setActiveModal({ kind: "receipt", isReprint: true, result: reprinted });
    } catch (e: unknown) {
      console.error("Reprint failed", e);
    }
  }, [lastReceiptNumber, sessionUser.user_id]);

  const handleNoSale = useCallback(async () => {
    try { await cashNoSale(shift.shift_id, sessionUser.user_id); }
    catch (e) { console.error("No-sale audit failed:", e); }
    // Open cash drawer — best-effort
    openCashDrawer().catch(() => {});
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
      setActiveModal({ kind: "none" });
      focusBarcode();
    } catch (e) {
      console.error("Failed to load sale for edit", e);
    }
  }, [clearCart, addCustomItem, focusBarcode, sessionUser.user_id]);

  // ── Clear cart with confirmation ──────────────────────────────────────────────
  const handleClearCartRequest = useCallback(() => {
    if (lineCount === 0) return;
    setActiveModal({ kind: "clearConfirm" });
  }, [lineCount]);

  const handleClearConfirmed = useCallback(() => {
    clearCart();
    setActiveModal({ kind: "none" });
    focusBarcode();
  }, [clearCart, focusBarcode]);

  // ── Recent item controls ──────────────────────────────────────────────────────
  const handleIncrementRecent = useCallback(() => bumpRecentQty(1),  [bumpRecentQty]);
  const handleDecrementRecent = useCallback(() => bumpRecentQty(-1), [bumpRecentQty]);

  // ── Hold / Resume ─────────────────────────────────────────────────────────────
  const handleOpenHold = useCallback(() => setActiveModal({ kind: "hold" }), []);

  // ── Barcode scan handler ──────────────────────────────────────────────────────
  const handleBarcode = useCallback((barcode: string, qty?: number) => {
    const effectiveQty = qty ?? (parseInt(numpadRef.current) || 1);
    scanQueueRef.current = scanQueueRef.current.then(async () => {
      try {
        await addByBarcode(barcode, effectiveQty);
        barcodeRef.current?.flashSuccess();
        setNumpadValue("1"); // reset after successful scan
      } catch {
        barcodeRef.current?.flashError();
        void ghostRecord(barcode); // fire-and-forget, never throws
      } finally {
        focusBarcode();
      }
    });
  }, [addByBarcode, focusBarcode]);

  // ── Shortcut manager ─────────────────────────────────────────────────────────
  const shortcutHandlers = useMemo(() => ({
    noModalOpen,
    lineCount,
    hasRecentLine: recentLineId !== null,
    lastReceiptNumber,
    onFocusBarcode:      focusBarcode,
    onHold:              handleOpenHold,
    onResumeHeld:        handleOpenHold,
    onPay:               openPay,
    onPayFast:           handlePayFast,
    onDiscount:          () => setActiveModal({ kind: "discount" }),
    onRefund:            () => canRefund && setActiveModal({ kind: "refund" }),
    onClearCart:         handleClearCartRequest,
    onReprintLast:       handleReprintLast,
    onNoSale:            handleNoSale,
    onXReport:           canViewXReport ? () => setActiveModal({ kind: "xReport" }) : undefined,
    onIncrementRecent:   handleIncrementRecent,
    onDecrementRecent:   handleDecrementRecent,
    onRemoveRecent:      removeRecentLine,
    onLock:              onLogout,
    onReport:            () => setActiveModal({ kind: "report" }),
    onCustomItem:        () => setActiveModal({ kind: "customItem" }),
    onHelp:              () => setActiveModal({ kind: "help" }),
  }), [noModalOpen, lineCount, recentLineId, lastReceiptNumber, canRefund, canViewXReport,
       focusBarcode, handleOpenHold, openPay, handlePayFast, handleClearCartRequest,
       handleReprintLast, handleNoSale, handleIncrementRecent, handleDecrementRecent,
       removeRecentLine, onLogout]);
  usePosShortcuts(shortcutHandlers);

  // ── Focus barcode after any modal closes ──────────────────────────────────────
  useEffect(() => {
    if (noModalOpen) focusBarcode();
  }, [noModalOpen, focusBarcode]);

  // ── Cleanup restock alert timer on unmount ───────────────────────────────────
  useEffect(() => {
    return () => {
      if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
    };
  }, []);

  // ── Load business flags, branch settings, and thermal config on mount ────────
  useEffect(() => {
    businessFlagsLoad().then(setBizFlags).catch(() => {});
    settingsGetBranch().then(setBranchSettings).catch(() => {});
    thermalGetConfig().then(c => setThermalEnabled(c.enabled)).catch(() => {});
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
    const next = applyDialpadKey(numpadRef.current === "1" && key !== "⌫" ? "" : numpadRef.current, key);
    // Keep value at minimum "1" visually, but store "" as "1" on confirm
    const clamped = next === "" ? "1" : next;
    setNumpadValue(clamped);
    // Live-update recent cart line qty
    if (recentLineId && next !== "" && next !== "0") {
      updateQuantity(recentLineId, next);
    }
  }, [recentLineId, updateQuantity]);

  // ── Sync status helpers ───────────────────────────────────────────────────────
  const isOnline      = syncStatus?.online ?? false;

  return (
    <div className={`pos-layout ${lineCount > 0 ? "pos-has-cart" : "pos-idle"} ${activeModal.kind === "payment" || payFastLoading ? "pos-payment-started" : ""} ${!isOnline ? "pos-offline" : "pos-online"}`}>
      {/* ── Top bar ── */}
      <div className="top-bar" data-tauri-drag-region="true">
        {/* Left: sidebar toggle + brand + store stack */}
        <div className="top-bar-left" data-tauri-drag-region="true">
          <button
            className="top-bar-sidebar-toggle"
            onClick={toggleSidebar}
            title={showSidebar ? "Hide sidebar" : "Show sidebar"}
            aria-label={showSidebar ? "Hide sidebar" : "Show sidebar"}
            data-tauri-drag-region="false"
          >☰</button>
          <button
            className="top-bar-logo top-bar-logo-btn"
            onClick={() => { setActiveModal({ kind: "none" }); setBannerResult(null); focusBarcode(); }}
            title="Back to POS"
            data-tauri-drag-region="false"
          >ZAN<span>POS</span></button>
          {/* Store name + cashier stacked vertically */}
          <div className="top-bar-store-stack" data-tauri-drag-region="true">
            <span className="top-bar-store-name">{DEVICE.branch_name}</span>
            <span className="top-bar-cashier-sub">{sessionUser.display_name}</span>
          </div>
        </div>

        {/* Centre: status pills + shift actions stacked */}
        <div className="top-bar-center" data-tauri-drag-region="true">
          <div className="top-bar-shift-group" data-tauri-drag-region="true">
            <div className="top-bar-shift-row" data-tauri-drag-region="true">
              <span className="top-bar-pill top-bar-pill-success">Shift Open</span>
              <SyncChip status={syncStatus} />
              <WhatsAppStatusPill
                sessionRole={sessionUser.role_name}
                onOpenQR={() => setShowWaQR(true)}
              />
            </div>
            <div className="top-bar-shift-row top-bar-shift-actions" data-tauri-drag-region="false">
              <button className="top-bar-btn top-bar-btn-danger top-bar-close-shift-btn" onClick={() => setActiveModal({ kind: "shiftClose" })}>
                Close Shift
              </button>
            </div>
          </div>
        </div>

        {/* Right: time + utility actions */}
        <div className="top-bar-right" data-tauri-drag-region="true">
          <span className="top-bar-time" data-tauri-drag-region="true">{clockTime}</span>
          {lastReceiptNumber && (
            <button className="top-bar-btn" onClick={handleReprintLast} title={`Reprint #${lastReceiptNumber} (Ctrl+P)`} data-tauri-drag-region="false">
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
                data-tauri-drag-region="false"
              >
                {meta.icon} {meta.label}
              </button>
            );
          })()}
          <button className="top-bar-btn top-bar-logout" onClick={onLogout} title="Ctrl+L" data-tauri-drag-region="false">
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
      <div className="pos-main" style={{ gridTemplateColumns: showSidebar ? "88px minmax(0, 1fr) 370px" : "0px minmax(0, 1fr) 370px" }}>
        {/* Icon sidebar */}
        <div className={`pos-sidebar${showSidebar ? "" : " pos-sidebar-hidden"}`}>
          <button className="pos-sidebar-item active" aria-label="Quick Sale">
            <ShoppingBag size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>Sale</span>
          </button>
          <div className="pos-sidebar-divider" />
          <button className="pos-sidebar-item" aria-label="Today's Report" onClick={() => setActiveModal({ kind: "report" })}>
            <BarChart2 size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>Reports</span>
          </button>
          <button className="pos-sidebar-item" aria-label="Recent Sales — reprint or void" onClick={() => setActiveModal({ kind: "recent" })}>
            <Clock size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>Recent</span>
          </button>
          {canViewXReport && (
            <button className="pos-sidebar-item" aria-label="X-Report — mid-shift drawer check" onClick={() => setActiveModal({ kind: "xReport" })}>
              <ClipboardList size={18} strokeWidth={1.75} aria-hidden="true" />
              <span>X-Report</span>
            </button>
          )}
          {canOpenBackOffice && (
            <button className="pos-sidebar-item" aria-label="Back Office" onClick={() => setActiveModal({ kind: "backOffice" })}>
              <Building2 size={18} strokeWidth={1.75} aria-hidden="true" />
              <span>Back Office</span>
            </button>
          )}
          <button className="pos-sidebar-item" aria-label="Deliveries" onClick={() => setShowDeliveries(true)}>
            <Bike size={18} strokeWidth={1.75} aria-hidden="true" />
            <span>Deliveries</span>
          </button>
          {canOpenBackOffice && (
            <button className="pos-sidebar-item" aria-label="Notes (admin)" onClick={() => setShowNotes(true)}>
              <StickyNote size={18} strokeWidth={1.75} aria-hidden="true" />
              <span>Notes</span>
            </button>
          )}
          <div className="pos-sidebar-spacer" />
          {onOpenAdminChat && (
            <button className="pos-sidebar-item" aria-label="AI Admin" onClick={onOpenAdminChat}>
              <Sparkles size={18} strokeWidth={1.75} aria-hidden="true" />
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
              onClick={() => setActiveModal({ kind: "customItem" })}
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
            paymentStarted={activeModal.kind === "payment"}
            recentLineId={recentLineId}
            onBumpLine={bumpLine}
            compact={true}
          />
        </div>

        {/* Numpad panel */}
        <div className="numpad-panel">
          {/* Top: multiplier display + dialpad — pushed to top */}
          <div className="numpad-top">
            <div className="numpad-display">
              <span className="numpad-multiplier">× {numpadValue}</span>
              {recentLineId && (
                <span className="numpad-recent-name">
                  {cart.lines.find(l => l.cart_line_id === recentLineId && !l.voided)?.product_name}
                </span>
              )}
            </div>
            <Dialpad onKey={handleNumpadKey} />
          </div>

          {/* Totals + Payment — anchored to bottom */}
          {(() => {
            const activeLines = cart.lines.filter(l => !l.voided);
            const grossTotal = activeLines.reduce((s, l) => s + l.line_total_minor, 0);
            const totalDiscount = cart.bill_discount_minor + activeLines.reduce((s, l) => s + l.line_discount_minor, 0);
            const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;
            const canPay = activeLines.length > 0 && netTotal > 0 && !(payFastLoading || activeModal.kind === "payment");
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
            onClick={handleOpenHold}
            title="Hold current order or resume a held order — F6"
          >
            Hold / Resume <kbd>F6</kbd>
          </button>
        </div>
        <div className="action-group action-group-modifiers">
          {(canOpenBackOffice || bizFlags.cashier_can_discount) && (
          <button
            className="action-btn"
            onClick={() => setActiveModal({ kind: "discount" })}
            disabled={lineCount === 0}
            title="Apply bill discount — F8"
          >
            Discount <span className="action-lock">🔒</span> <kbd>F8</kbd>
          </button>
          )}
          {canRefund && (
            <button
              className="action-btn action-btn-danger"
              onClick={() => setActiveModal({ kind: "refund" })}
              title="Process a refund — Ctrl+R"
            >
              Refund <span className="action-lock">🔒</span> <kbd>F10</kbd>
            </button>
          )}
        </div>
        <div className="action-group action-group-operational">
          <button
            className="action-btn"
            onClick={() => setActiveModal({ kind: "cashEvent" })}
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

      {/* ── Clear cart confirmation ── */}
      {activeModal.kind === "clearConfirm" && (
        <div className="modal-overlay" onClick={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}>
          <div className="modal clear-confirm-modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <span className="modal-title">Clear Cart?</span>
            </div>
            <p className="clear-confirm-body">
              Remove all {lineCount} item{lineCount !== 1 ? "s" : ""} from the cart?
              This will be recorded as a pre-tender void.
            </p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}>
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
      {activeModal.kind === "customItem" && (
        <CustomItemModal
          onAdd={async (name, price, qty) => {
            await addCustomItem(name, price, qty);
            setActiveModal({ kind: "none" });
            refreshSuggestions();
            focusBarcode();
          }}
          onCancel={() => { setActiveModal({ kind: "none" }); refreshSuggestions(); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "cashEvent" && (
        <CashEventModal
          shiftId={shift.shift_id}
          userId={sessionUser.user_id}
          cashierName={sessionUser.display_name}
          onDone={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
          onCancel={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "discount" && (
        <DiscountModal
          grossMinor={cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.line_total_minor, 0)}
          currentDiscountMinor={cart.bill_discount_minor}
          onApply={async (discount_minor, reason) => {
            await applyBillDiscount(discount_minor, reason);
            setActiveModal({ kind: "none" });
            focusBarcode();
          }}
          onCancel={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "payment" && (
        <PaymentModal
          netTotal={netTotal}
          initialMethod={activeModal.method}
          splitMode={activeModal.split}
          onConfirm={handleConfirmPayment}
          onCancel={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
          loading={loading}
          sessionUserId={sessionUser.user_id}
        />
      )}

      {/* ── Post-sale success banner (non-blocking) ── */}
      {bannerResult && activeModal.kind !== "receipt" && (
        <div className="sale-banner">
          <span className="sale-banner-icon">✓</span>
          <span className="sale-banner-text">Sale #{bannerResult.receipt_number}</span>
          <button className="sale-banner-print" onClick={() => setActiveModal({ kind: "receipt", isReprint: false, result: bannerResult! })}>
            🖨 Print Receipt
          </button>
          <button className="sale-banner-dismiss" onClick={handleNewSale} title="Dismiss">×</button>
        </div>
      )}

      {/* ── Full receipt modal (explicit print or reprint) ── */}
      {activeModal.kind === "receipt" && (
        <ReceiptPreview
          sale={activeModal.result}
          isReprint={activeModal.isReprint}
          onNewSale={() => { setActiveModal({ kind: "none" }); handleNewSale(); }}
        />
      )}

      {activeModal.kind === "shiftClose" && (
        <ShiftModal
          mode="close"
          user={sessionUser}
          shift={shift}
          onShiftOpened={() => {}}
          onShiftClosed={() => { setActiveModal({ kind: "none" }); onShiftClose(true); }}
          onCancel={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "hold" && (
        <HoldModal
          cart={cart}
          lineCount={lineCount}
          netTotal={netTotal}
          onHeld={() => { setActiveModal({ kind: "none" }); clearCart(); focusBarcode(); }}
          onResume={(resumed) => { setActiveModal({ kind: "none" }); replaceCart(resumed); focusBarcode(); }}
          onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "refund" && (
        <RefundModal
          cashierUserId={sessionUser.user_id}
          onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "report" && (
        <TodayReportModal onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }} />
      )}

      {activeModal.kind === "backOffice" && (
        <BackOfficeModal
          sessionUser={sessionUser}
          onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "xReport" && (
        <XReportModal
          shiftId={shift.shift_id}
          actorUserId={sessionUser.user_id}
          onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
        />
      )}

      {activeModal.kind === "help" && (
        <HelpModal onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }} />
      )}

      {activeModal.kind === "recent" && (
        <RecentSalesModal
          onReprint={async (receiptNumber) => {
            try {
              const reprinted = await receiptReprint(receiptNumber, sessionUser.user_id);
              setActiveModal({ kind: "receipt", isReprint: true, result: reprinted });
            } catch (e) { console.error("Reprint failed", e); }
          }}
          onEdit={handleEditSale}
          onClose={() => { setActiveModal({ kind: "none" }); focusBarcode(); }}
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
          <div className="dlv-modal-shell" role="dialog" aria-modal="true" aria-labelledby="dlv-title">
            <div className="dlv-modal-header">
              <span className="dlv-modal-title" id="dlv-title">🛵 Deliveries</span>
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
