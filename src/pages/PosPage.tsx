import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { LowStockAlert, ProductWithPrice, SaleResult, SessionUser, Shift } from "../types";
import { DEVICE } from "../types";
import { productListAll } from "../tauri/commands";
import { useCart } from "../hooks/useCart";
import { useSyncStatus } from "../hooks/useSyncStatus";
import BarcodeInput from "../components/BarcodeInput";
import ProductGrid from "../components/ProductGrid";
import CartPanel from "../components/CartPanel";
import PaymentModal from "../components/PaymentModal";
import ReceiptPreview from "../components/ReceiptPreview";
import SyncChip from "../components/SyncChip";
import ShiftModal from "../components/ShiftModal";
import HoldModal from "../components/HoldModal";
import RefundModal from "../components/RefundModal";
import TodayReportModal from "../components/TodayReportModal";

interface Props {
  sessionUser: SessionUser;
  shift: Shift;
  onLogout: () => void;
  onShiftClose: (closed: boolean) => void;
  onOpenAdminChat?: () => void;
}

export default function PosPage({ sessionUser, shift, onLogout, onShiftClose, onOpenAdminChat }: Props) {
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
  const [saleResult, setSaleResult] = useState<SaleResult | null>(null);
  const [showShiftClose, setShowShiftClose] = useState(false);
  const [showHold, setShowHold] = useState(false);
  const [showRefund, setShowRefund] = useState(false);
  const [showReport, setShowReport] = useState(false);
  const [restockAlerts, setRestockAlerts] = useState<LowStockAlert[]>([]);
  const restockTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const syncStatus = useSyncStatus(15_000);
  const {
    cart, loading, error, clearError,
    addByBarcode, addProduct, updateQuantity, removeLine,
    finalizeSale, clearCart, replaceCart,
    netTotal, taxTotal, lineCount,
  } = useCart(session);

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

  const handleConfirmPayment = async (payments: import("../types").PaymentInput[]) => {
    try {
      const result = await finalizeSale(payments);
      setShowPayment(false);
      setSaleResult(result);
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

  const now = new Date();
  const timeStr = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div className="pos-layout">
      {/* ── Top bar ── */}
      <div className="top-bar">
        <span className="top-bar-branch">{DEVICE.branch_name}</span>
        <span className="top-bar-cashier">👤 {sessionUser.display_name}</span>
        <SyncChip status={syncStatus} />
        <span className="top-bar-time">{timeStr}</span>
        <button className="top-bar-btn" onClick={() => setShowReport(true)} title="Today's Report">
          📊
        </button>
        {onOpenAdminChat && (
          <button className="top-bar-btn top-bar-admin" onClick={onOpenAdminChat} title="Admin Chat">
            AI
          </button>
        )}
        <button className="top-bar-btn" onClick={() => setShowShiftClose(true)} title="Close Shift">
          🔒
        </button>
        <button className="top-bar-btn top-bar-logout" onClick={onLogout} title="Logout">
          ⏻
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
          onPay={() => lineCount > 0 && setShowPayment(true)}
        />
      </div>

      {/* ── Action bar ── */}
      <div className="action-bar">
        <button className="action-btn" onClick={clearCart} disabled={lineCount === 0}>
          Clear
        </button>
        <button className="action-btn" onClick={() => setShowHold(true)}>
          Hold (F6)
        </button>
        <button className="action-btn" onClick={() => setShowRefund(true)}>
          Refund
        </button>
        <button
          className="action-btn action-btn-pay"
          onClick={() => lineCount > 0 && setShowPayment(true)}
          disabled={lineCount === 0}
        >
          Pay (F9)
        </button>
      </div>

      {/* ── Modals ── */}
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
