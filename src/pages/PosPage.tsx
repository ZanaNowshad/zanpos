import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { LowStockAlert, ProductWithPrice, SaleResult, SessionUser, Shift } from "../types";
import { DEVICE } from "../types";
import { cashNoSale, productListAll, receiptReprint } from "../tauri/commands";
import { useCart } from "../hooks/useCart";
import { useSyncStatus } from "../hooks/useSyncStatus";
import BarcodeInput from "../components/BarcodeInput";
import ProductGrid from "../components/ProductGrid";
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

interface Props {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenAdminChat?: () => void;
  theme?: "dark" | "light";
  onToggleTheme?: () => void;
}

export default function PosPage({ sessionUser, shift, onLogout, onShiftClose, onOpenAdminChat, theme, onToggleTheme }: Props) {
  const session = {
    branch_id: DEVICE.branch_id,
    device_id: DEVICE.device_id,
    shift_id: shift.shift_id,
    cashier_user_id: sessionUser.user_id,
  };

  const [allProducts, setAllProducts] = useState<ProductWithPrice[]>([]);
  const [productLoading, setProductLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [showPayment, setShowPayment] = useState(false);
  const [saleResult, setSaleResult]           = useState<SaleResult | null>(null);
  const [lastReceiptNumber, setLastReceiptNumber] = useState<string | null>(null);
  const [showShiftClose, setShowShiftClose] = useState(false);
  const [showHold, setShowHold] = useState(false);
  const [showRefund, setShowRefund] = useState(false);
  const [showReport, setShowReport] = useState(false);
  const [showDiscount, setShowDiscount] = useState(false);
  const [showBackOffice, setShowBackOffice] = useState(false);
  const [showCustomItem, setShowCustomItem] = useState(false);
  const [showCashEvent, setShowCashEvent] = useState(false);
  const [showXReport, setShowXReport]     = useState(false);
  const canOpenBackOffice = ["owner", "manager"].includes(sessionUser.role_name);
  const canViewXReport    = canOpenBackOffice;
  const [restockAlerts, setRestockAlerts] = useState<LowStockAlert[]>([]);
  const restockTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const syncStatus = useSyncStatus(15_000);
  const {
    cart, loading, error, clearError,
    addByBarcode, addProduct, addCustomItem, updateQuantity, removeLine,
    applyBillDiscount, applyLineDiscount, setLineNote,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session);

  // ── Keyboard shortcuts (after lineCount is declared) ──────────────────────
  useEffect(() => {
    const noModalOpen = () =>
      !showPayment && !saleResult && !showShiftClose &&
      !showHold && !showRefund && !showReport &&
      !showDiscount && !showBackOffice && !showCustomItem && !showCashEvent && !showXReport;

    const handler = (e: KeyboardEvent) => {
      if (e.key === "F6") {
        e.preventDefault();
        if (noModalOpen()) setShowHold(true);
      } else if (e.key === "F9") {
        e.preventDefault();
        if (noModalOpen() && lineCount > 0) setShowPayment(true);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [showPayment, saleResult, showShiftClose, showHold, showRefund,
      showReport, showDiscount, showBackOffice, showCustomItem, showCashEvent, showXReport, lineCount]);

  useEffect(() => {
    setProductLoading(true);
    productListAll()
      .then(setAllProducts)
      .catch(e => console.error("Failed to load products", e))
      .finally(() => setProductLoading(false));
  }, []);

  const categories = useMemo(() => {
    const seen = new Map<string, string>();
    for (const p of allProducts) {
      if (!seen.has(p.category_id)) seen.set(p.category_id, p.category_name);
    }
    return Array.from(seen.entries()).map(([id, name]) => ({ id, name }));
  }, [allProducts]);

  const displayProducts = useMemo(() => {
    let products = allProducts;
    if (selectedCategory) products = products.filter(p => p.category_id === selectedCategory);
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      products = products.filter(p =>
        p.name.toLowerCase().includes(q) ||
        p.sku?.toLowerCase().includes(q) ||
        p.barcode?.includes(q)
      );
    }
    return products;
  }, [allProducts, selectedCategory, searchQuery]);

  const handleBarcode = useCallback(async (barcode: string) => {
    await addByBarcode(barcode);
  }, [addByBarcode]);

  const handleSearch = useCallback((query: string) => {
    setSearchQuery(query);
  }, []);

  const handleConfirmPayment = async (payments: import("../types").PaymentInput[], customerId?: string) => {
    try {
      const result = await finalizeSale(payments, customerId);
      setShowPayment(false);
      setSaleResult(result);
      setLastReceiptNumber(result.receipt_number);
      if (result.low_stock_alerts.length > 0) {
        if (restockTimerRef.current) clearTimeout(restockTimerRef.current);
        setRestockAlerts(result.low_stock_alerts);
        restockTimerRef.current = setTimeout(() => setRestockAlerts([]), 6000);
      }
    } catch {
      // error is set in useCart
    }
  };

  const handleNewSale = () => {
    setSaleResult(null);
    clearCart();
  };

  const handleReprintLast = async () => {
    if (!lastReceiptNumber) return;
    try {
      const reprinted = await receiptReprint(lastReceiptNumber);
      setSaleResult(reprinted);
    } catch (e: unknown) {
      console.error("Reprint failed", e);
    }
  };

  const [clockTime, setClockTime] = useState(() =>
    new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
  );

  useEffect(() => {
    const t = setInterval(() => {
      setClockTime(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
    }, 10_000);
    return () => clearInterval(t);
  }, []);

  return (
    <div className="pos-layout">
      {/* ── Top bar ── */}
      <div className="top-bar">
        <span className="top-bar-branch">{DEVICE.branch_name}</span>
        <span className="top-bar-cashier">{sessionUser.display_name}</span>
        <SyncChip status={syncStatus} />
        <span className="top-bar-spacer" />
        <span className="top-bar-time">{clockTime}</span>

        <button className="top-bar-btn" onClick={() => setShowReport(true)}>
          Report
        </button>
        {canViewXReport && (
          <button className="top-bar-btn" onClick={() => setShowXReport(true)} title="X-Report — mid-shift drawer check">
            X-Report
          </button>
        )}
        {lastReceiptNumber && (
          <button className="top-bar-btn" onClick={handleReprintLast} title={`Reprint ${lastReceiptNumber}`}>
            Reprint
          </button>
        )}
        {canOpenBackOffice && (
          <button className="top-bar-btn" onClick={() => setShowBackOffice(true)}>
            Back Office
          </button>
        )}
        {onOpenAdminChat && (
          <button className="top-bar-btn top-bar-admin" onClick={onOpenAdminChat}>
            AI Admin
          </button>
        )}
        {onToggleTheme && (
          <button
            className="top-bar-btn top-bar-theme"
            onClick={onToggleTheme}
            title={theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode"}
          >
            {theme === "dark" ? "☀" : "🌙"}
          </button>
        )}
        <button className="top-bar-btn top-bar-btn-danger" onClick={() => setShowShiftClose(true)}>
          Close Shift
        </button>
        <button className="top-bar-btn top-bar-logout" onClick={onLogout}>
          Logout
        </button>
      </div>

      {/* ── Error banner ── */}
      {error && (
        <div className="error-banner" onClick={clearError}>
          ⚠ {error} <span className="error-dismiss">✕</span>
        </div>
      )}

      {/* ── Main area ── */}
      <div className="pos-main">
        {/* Product area */}
        <div className="product-area">
          <BarcodeInput onBarcode={handleBarcode} onSearch={handleSearch} disabled={loading} />

          {categories.length > 0 && (
            <div className="category-tabs">
              <button
                className={`cat-tab ${selectedCategory === null ? "cat-tab-active" : ""}`}
                onClick={() => setSelectedCategory(null)}
              >
                All
              </button>
              {categories.map(c => (
                <button
                  key={c.id}
                  className={`cat-tab ${selectedCategory === c.id ? "cat-tab-active" : ""}`}
                  onClick={() => setSelectedCategory(c.id)}
                >
                  {c.name}
                </button>
              ))}
            </div>
          )}

          <ProductGrid products={displayProducts} onSelect={addProduct} loading={productLoading} />
        </div>

        {/* Cart area */}
        <CartPanel
          cart={cart}
          netTotal={netTotal}
          taxTotal={taxTotal}
          onUpdateQty={updateQuantity}
          onRemove={removeLine}
          onApplyLineDiscount={applyLineDiscount}
          onSetLineNote={setLineNote}
          onPay={() => lineCount > 0 && setShowPayment(true)}
        />
      </div>

      {/* ── Action bar ── */}
      <div className="action-bar">
        <button
          className="action-btn action-btn-danger"
          onClick={clearCart}
          disabled={lineCount === 0}
          title="Clear all items from cart"
        >
          Clear
        </button>
        <button className="action-btn" onClick={() => setShowHold(true)} title="Hold cart and resume later (F6)">
          Hold
        </button>
        <button className="action-btn" onClick={() => setShowCustomItem(true)} title="Add a custom item with any price">
          + Custom
        </button>
        <button
          className="action-btn"
          onClick={() => setShowDiscount(true)}
          disabled={lineCount === 0}
          title="Apply a bill-level discount"
        >
          % Discount
        </button>
        <button className="action-btn" onClick={() => setShowCashEvent(true)} title="Paid In / Paid Out / Safe Drop">
          Cash Event
        </button>
        <button
          className="action-btn"
          onClick={async () => {
            try { await cashNoSale(shift.shift_id, sessionUser.user_id); }
            catch (e) { console.error("No-sale audit failed:", e); }
          }}
          title="Open drawer without a sale — audited in the system"
        >
          No Sale
        </button>
        <button className="action-btn" onClick={() => setShowRefund(true)} title="Process a return or refund">
          Refund
        </button>
        <button
          className="action-btn action-btn-pay"
          onClick={() => lineCount > 0 && setShowPayment(true)}
          disabled={lineCount === 0}
          title="Collect payment (F9)"
        >
          Pay  F9
        </button>
      </div>

      {/* ── Modals ── */}
      {showCustomItem && (
        <CustomItemModal
          onAdd={async (name, price, qty) => {
            await addCustomItem(name, price, qty);
            setShowCustomItem(false);
          }}
          onCancel={() => setShowCustomItem(false)}
        />
      )}

      {showCashEvent && (
        <CashEventModal
          shiftId={shift.shift_id}
          userId={sessionUser.user_id}
          onDone={() => setShowCashEvent(false)}
          onCancel={() => setShowCashEvent(false)}
        />
      )}

      {showDiscount && (
        <DiscountModal
          grossMinor={cart.lines.filter(l => !l.voided).reduce((s, l) => s + l.line_total_minor, 0)}
          currentDiscountMinor={cart.bill_discount_minor}
          onApply={async (discount_minor) => {
            await applyBillDiscount(discount_minor);
            setShowDiscount(false);
          }}
          onCancel={() => setShowDiscount(false)}
        />
      )}

      {showPayment && (
        <PaymentModal
          netTotal={netTotal}
          onConfirm={handleConfirmPayment}
          onCancel={() => setShowPayment(false)}
          loading={loading}
        />
      )}

      {saleResult && (
        <ReceiptPreview sale={saleResult} onNewSale={handleNewSale} />
      )}

      {showShiftClose && (
        <ShiftModal
          mode="close"
          user={sessionUser}
          shift={shift}
          onShiftOpened={() => {}}
          onShiftClosed={() => { setShowShiftClose(false); onShiftClose(true); }}
          onCancel={() => setShowShiftClose(false)}
        />
      )}

      {showHold && (
        <HoldModal
          cart={cart}
          lineCount={lineCount}
          netTotal={netTotal}
          onHeld={() => { setShowHold(false); clearCart(); }}
          onResume={(resumed) => { setShowHold(false); replaceCart(resumed); }}
          onClose={() => setShowHold(false)}
        />
      )}

      {showRefund && (
        <RefundModal
          cashierUserId={sessionUser.user_id}
          onClose={() => setShowRefund(false)}
        />
      )}

      {showReport && (
        <TodayReportModal onClose={() => setShowReport(false)} />
      )}

      {showBackOffice && (
        <BackOfficeModal sessionUser={sessionUser} onClose={() => setShowBackOffice(false)} />
      )}

      {showXReport && (
        <XReportModal
          shiftId={shift.shift_id}
          actorUserId={sessionUser.user_id}
          onClose={() => setShowXReport(false)}
        />
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
