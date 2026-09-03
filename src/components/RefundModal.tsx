import { useMemo, useRef, useState } from "react";
import { CheckCircle2, FileSearch, ListChecks, RefreshCcw, X } from "lucide-react";
import type { SaleForRefund, SaleItemForRefund, SaleListRow, RefundResult } from "../types";
import { DEVICE } from "../types";
import { refundGetSale, refundCreate, reportSalesList, authValidateManagerPin } from "../tauri/commands";
import { formatMoney } from "../money";
import { initQtyMap, lineRefundAmount, type QtyMap } from "../utils/refundMath";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator, ownedModalLabel } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

const REASON_CODE_LABELS: Record<string, string> = {
  customer_return: "Customer return / changed mind",
  defective: "Defective / damaged",
  wrong_item: "Wrong item delivered",
  exchange: "Exchange",
  other: "Other",
};

interface Props {
  cashierUserId: string;
  onClose: () => void;
  onExchangeStarted?: (exchange: { refund: RefundResult; creditMinor: number; originalReceipt: string }) => void;
}

type RefundMode = "receipt" | "browse";
type ReturnAction = "refund" | "exchange";

function todayStr() {
  return new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

export default function RefundModal({ cashierUserId, onClose, onExchangeStarted }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [returnAction, setReturnAction] = useState<ReturnAction>("refund");
  const [mode, setMode]               = useState<RefundMode>("receipt");
  const [receiptInput, setReceiptInput] = useState("");
  const [sale, setSale]               = useState<SaleForRefund | null>(null);
  const [refundQtys, setRefundQtys]   = useState<QtyMap>(new Map());
  const [reason, setReason]           = useState("");
  const [reasonCode, setReasonCode]   = useState("customer_return");
  const [result, setResult]           = useState<RefundResult | null>(null);
  const [searching, setSearching]     = useState(false);
  const [submitting, setSubmitting]   = useState(false);
  const [error, setError]             = useState<string | null>(null);
  // Browse mode state
  const [browseDate, setBrowseDate]   = useState(todayStr());
  const [salesList, setSalesList]     = useState<SaleListRow[]>([]);
  const [listLoading, setListLoading] = useState(false);
  // Cross-device manager override
  const [showPinEntry, setShowPinEntry]     = useState(false);
  const [managerPin, setManagerPin]         = useState("");
  const [pinError, setPinError]             = useState<string | null>(null);
  const [pinLoading, setPinLoading]         = useState(false);
  const [overrideToken, setOverrideToken]   = useState<string | null>(null);
  // One key per refund attempt, kept across retries: the backend answers a
  // repeat with the refund it already made. Matters most on the manager-PIN
  // path, where the same refund is submitted twice by design.
  const attemptKey = useRef<string | null>(null);

  const isCrossDevice = sale ? (sale.origin_device_id && sale.origin_device_id !== DEVICE.device_id) : false;

  const handleSearch = async () => {
    if (!receiptInput.trim()) return;
    setSearching(true);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const found = await refundGetSale(receiptInput.trim().toUpperCase(), cashierUserId);
      attemptKey.current = null;
      setSale(found);
      setRefundQtys(initQtyMap(found.items));
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : dt("receiptNotFound");
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? dt("receiptLookupPermission")
        : msg);
    } finally {
      setSearching(false);
    }
  };

  const loadBrowseList = async (d: string) => {
    setListLoading(true);
    setError(null);
    setSalesList([]);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const page = await reportSalesList(cashierUserId, DEVICE.branch_id, d, d);
      setSalesList(page.items.filter(r => r.status !== "voided"));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : dt("failedLoadSales"));
    } finally {
      setListLoading(false);
    }
  };

  const handleBrowseDateChange = (d: string) => {
    setBrowseDate(d);
    loadBrowseList(d);
  };

  const handleBrowseSelect = async (row: SaleListRow) => {
    setSearching(true);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    try {
      const found = await refundGetSale(row.receipt_number, cashierUserId);
      setSale(found);
      setRefundQtys(initQtyMap(found.items));
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : dt("failedLoadSale");
      setError(msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")
        ? dt("receiptLookupPermission")
        : msg);
    } finally {
      setSearching(false);
    }
  };

  const switchMode = (m: RefundMode) => {
    setMode(m);
    setError(null);
    setSale(null);
    setRefundQtys(new Map());
    setReceiptInput("");
    if (m === "browse") loadBrowseList(browseDate);
  };

  const setQty = (saleItemId: string, raw: string, maxQty?: number) => {
    // FIX: clamp upper bound — type="number" input allows keyboard entry > maxQty
    const n = Math.min(
      Math.max(0, parseInt(raw, 10) || 0),
      maxQty ?? Infinity
    );
    setRefundQtys(prev => new Map(prev).set(saleItemId, n));
  };

  const toggleAll = (item: SaleItemForRefund) => {
    const current = refundQtys.get(item.sale_item_id) ?? 0;
    const max = Math.floor(parseFloat(item.quantity));
    setRefundQtys(prev => new Map(prev).set(item.sale_item_id, current === 0 ? max : 0));
  };

  const selectedItems = sale
    ? sale.items.filter(i => (refundQtys.get(i.sale_item_id) ?? 0) > 0)
    : [];

  const refundTotal = selectedItems.reduce((sum, item) => {
    const qty = refundQtys.get(item.sale_item_id) ?? 0;
    return sum + lineRefundAmount(item, qty);
  }, 0);

  const handleConfirm = async (immediateToken?: string, asExchange = false) => {
    if (!sale || selectedItems.length === 0) return;
    if (!attemptKey.current) attemptKey.current = crypto.randomUUID();
    setSubmitting(true);
    setError(null);
    try {
      const items = selectedItems.map(i => {
        const qty = refundQtys.get(i.sale_item_id) ?? 0;
        return {
          sale_item_id:          i.sale_item_id,
          product_name_snapshot: i.product_name_snapshot,
          quantity:              qty.toString(),
          unit_price_minor:      i.unit_price_minor,
          refund_amount_minor:   lineRefundAmount(i, qty),
        };
      });
      const refund = await refundCreate(
        sale.sale_id,
        items,
        reason || REASON_CODE_LABELS[asExchange ? "exchange" : reasonCode] || "Customer return",
        cashierUserId,
        asExchange ? "exchange" : reasonCode,
        // FIX: use immediateToken directly — React state (overrideToken) is not yet
        // updated when handleConfirm is called from handlePinSubmit in the same tick
        (immediateToken ?? overrideToken) ?? undefined,
        attemptKey.current ?? undefined,
      );
      if (asExchange && onExchangeStarted) {
        attemptKey.current = null;
        onExchangeStarted({ refund, creditMinor: refundTotal, originalReceipt: sale.receipt_number });
        return;
      }
      setResult(refund);
      setOverrideToken(null);
      attemptKey.current = null;
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : dt("refundFailed");
      if (msg.toLowerCase().includes("manager override") || msg.toLowerCase().includes("manager pin")) {
        setShowPinEntry(true);
        setManagerPin("");
        setPinError(null);
      } else if (msg.toLowerCase().includes("not permitted") || msg.toLowerCase().includes("permission")) {
        setError(dt("refundPermission"));
      } else {
        setError(msg);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handlePinSubmit = async () => {
    if (!managerPin.trim()) return;
    setPinLoading(true);
    setPinError(null);
    try {
      const token = await authValidateManagerPin(managerPin);
      setOverrideToken(token);
      setShowPinEntry(false);
      setManagerPin("");
      // FIX: await handleConfirm and pass token directly — React state is async,
      // overrideToken is still null in the closure until next render.
      // Also: await ensures pinLoading stays true until refund completes (prevents
      // double-submit by re-enabling the Authorise button prematurely).
      await handleConfirm(token, returnAction === "exchange");
    } catch (e: unknown) {
      const msg = typeof e === "string" ? e : dt("invalidPin");
      setPinError(msg.includes("Invalid") || msg.includes("Permission") ? dt("invalidManagerPin") : msg);
    } finally {
      setPinLoading(false);
    }
  };

  if (result) {
    return (
      <div className="modal-overlay">
        <div className="modal refund-modal">
          <div className="refund-success">
            <div className="refund-success-icon">✓</div>
            <h2>{t("refundComplete")}</h2>
            <div className="refund-receipt-num">{result.refund_receipt_number}</div>
            <div className="refund-amount">
              {DEVICE.currency} {formatMoney(result.refund_total_minor, DEVICE.currency_exponent)}
            </div>
            <button className="modal-btn-primary" onClick={onClose}>{t("done")}</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal refund-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">{t("refund")}</h2>
          <button className="modal-close" onClick={onClose} aria-label={t("close")}><X size={18} /></button>
        </div>

        <div className="refund-action-tabs" aria-label={t("returnAction")}>
          <button
            className={`refund-action-tab ${returnAction === "refund" ? "refund-action-tab-active" : ""}`}
            onClick={() => setReturnAction("refund")}
          >
            <RefreshCcw size={15} /> {t("refund")}
          </button>
          <button
            className={`refund-action-tab ${returnAction === "exchange" ? "refund-action-tab-active" : ""}`}
            onClick={() => { setReturnAction("exchange"); setReasonCode("exchange"); }}
          >
            <CheckCircle2 size={15} /> {t("exchange")}
          </button>
        </div>

        {/* Mode selector */}
        <div className="refund-mode-tabs">
          <button
            className={`refund-mode-tab ${mode === "receipt" ? "refund-mode-tab-active" : ""}`}
            onClick={() => switchMode("receipt")}
          ><FileSearch size={15} /> {t("byReceipt")}</button>
          <button
            className={`refund-mode-tab ${mode === "browse" ? "refund-mode-tab-active" : ""}`}
            onClick={() => switchMode("browse")}
          ><ListChecks size={15} /> {t("browseSales")}</button>
        </div>

        {mode === "receipt" && (
          <div className="refund-search">
            <input
              className="field-input"
              placeholder={t("receiptNumberExample")}
              value={receiptInput}
              onChange={e => setReceiptInput(e.target.value)}
              onKeyDown={e => e.key === "Enter" && handleSearch()}

            />
            <button className="modal-btn-secondary" onClick={handleSearch} disabled={searching}>
              {searching ? "…" : t("search")}
            </button>
          </div>
        )}

        {mode === "browse" && !sale && (
          <div className="refund-browse-section">
            <div className="refund-browse-header">
              <label htmlFor="a11y-input-1">{t("date")}:</label>
              <input id="a11y-input-1"
                className="field-input refund-browse-date"
                type="date"
                value={browseDate}
                onChange={e => handleBrowseDateChange(e.target.value)}
              />
            </div>
            {listLoading && <div className="refund-browse-loading">{t("loading")}</div>}
            <div className="refund-browse-list">
              {salesList.map(row => (
                <button
                  key={row.sale_id}
                  className="refund-browse-row"
                  onClick={() => handleBrowseSelect(row)}
                  disabled={searching}
                >
                  <span className="refund-br-receipt">{row.receipt_number}</span>
                  <span className="refund-br-time">
                    {new Date(row.sold_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </span>
                  <span className="refund-br-cashier">{row.cashier_name}</span>
                  <span className="refund-br-total">
                    {DEVICE.currency} {formatMoney(row.net_total_minor, DEVICE.currency_exponent)}
                  </span>
                </button>
              ))}
              {!listLoading && salesList.length === 0 && (
                <div className="refund-browse-empty">No refundable sales for {browseDate}</div>
              )}
            </div>
            {searching && <div className="refund-browse-loading">{t("loading")}</div>}
          </div>
        )}

        {mode === "browse" && sale && (
          <div className="refund-browse-back">
            <button className="modal-btn-secondary" onClick={() => { setSale(null); setRefundQtys(new Map()); }}>
              <span className="icon-directional" aria-hidden="true">←</span> Back to list
            </button>
            <span className="refund-receipt">{sale.receipt_number}</span>
          </div>
        )}

        {error && <div className="modal-error">{error}</div>}

        {sale && (
          <>
            <div className="refund-sale-info">
              <span className="refund-receipt">{sale.receipt_number}</span>
              <span className="refund-cashier">{sale.cashier_name}</span>
              <span className={`refund-status refund-status-${sale.status}`}>{ownedModalLabel(language, sale.status)}</span>
            </div>

            {isCrossDevice && (
              <div className="refund-cross-device-banner">
                This sale was made on another device. Manager override is required for refund.
              </div>
            )}

            <div className="refund-items">
              {sale.items.map(item => {
                const maxQty  = Math.floor(parseFloat(item.quantity));
                const current = refundQtys.get(item.sale_item_id) ?? 0;
                const lineRefund = lineRefundAmount(item, current);

                return (
                  <div key={item.sale_item_id} className={`refund-item ${current > 0 ? "refund-item-selected" : ""}`}>
                    {/* Checkbox toggles full quantity */}
                    <input
                      type="checkbox"
                      checked={current > 0}
                      onChange={() => toggleAll(item)}
                    />

                    <span className="refund-item-name">{item.product_name_snapshot}</span>

                    {/* Quantity stepper */}
                    <div className="refund-qty-stepper">
                      <button
                        className="refund-qty-btn"
                        onClick={() => setQty(item.sale_item_id, String(current - 1), maxQty)}
                        disabled={current <= 0}
                      >−</button>
                      <input
                        className="refund-qty-input"
                        type="number"
                        min={0}
                        max={maxQty}
                        value={current}
                        onChange={e => setQty(item.sale_item_id, e.target.value, maxQty)}
                      />
                      <button
                        className="refund-qty-btn"
                        onClick={() => setQty(item.sale_item_id, String(current + 1), maxQty)}
                        disabled={current >= maxQty}
                      >+</button>
                      <span className="refund-qty-of">/ {maxQty}</span>
                    </div>

                    <span className="refund-item-total">
                      {DEVICE.currency} {formatMoney(lineRefund, DEVICE.currency_exponent)}
                    </span>
                  </div>
                );
              })}
            </div>

            <select
              className="field-input refund-reason-select"
              value={reasonCode}
              onChange={e => setReasonCode(e.target.value)}
              disabled={returnAction === "exchange"}
            >
              {["customer_return", "defective", "wrong_item", "exchange", "other"].map(code => (
                <option key={code} value={code}>{ownedModalLabel(language, code)}</option>
              ))}
            </select>

            <input
              className="field-input"
              placeholder={t("additionalNotes")}
              value={reason}
              onChange={e => setReason(e.target.value)}
            />

            <div className="refund-footer">
              <div className="refund-total-line">
                {returnAction === "exchange" ? dt("exchangeCredit") : dt("refundTotal")}:{" "}
                <strong>{DEVICE.currency} {formatMoney(refundTotal, DEVICE.currency_exponent)}</strong>
                {returnAction === "exchange" && (
                  <span className="refund-exchange-hint">{t("scanReplacementItems")}</span>
                )}
              </div>
              <button
                className={returnAction === "exchange" ? "modal-btn-primary" : "modal-btn-danger"}
                onClick={() => handleConfirm(undefined, returnAction === "exchange")}
                disabled={submitting || selectedItems.length === 0}
              >
                {submitting
                  ? t("processing")
                  : returnAction === "exchange"
                    ? `Start exchange (${selectedItems.length})`
                    : `Refund ${selectedItems.length} line${selectedItems.length !== 1 ? "s" : ""}`}
              </button>
            </div>
          </>
        )}
        {showPinEntry && (
          <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal-overlay pin-overlay" onClick={() => setShowPinEntry(false)}>
            <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal pin-modal" onClick={e => e.stopPropagation()}>
              <div className="modal-header">
                <h3>{t("managerOverrideRequired")}</h3>
                <button className="modal-close" onClick={() => setShowPinEntry(false)} aria-label={t("close")}><X size={18} /></button>
              </div>
              <p className="pin-explain">
                Cross-device refunds require a manager or owner to authorise.
                Enter your PIN to proceed.
              </p>
              <input
                className="field-input"
                type="password"
                inputMode="numeric"
                maxLength={6}
                placeholder={t("managerPin")}
                value={managerPin}
                onChange={e => setManagerPin(e.target.value)}
                onKeyDown={e => e.key === "Enter" && handlePinSubmit()}

              />
              {pinError && <div className="modal-error">{pinError}</div>}
              <div className="pin-actions">
                <button className="modal-btn-secondary" onClick={() => setShowPinEntry(false)}>{t("cancel")}</button>
                <button className="modal-btn-primary" onClick={handlePinSubmit} disabled={pinLoading || !managerPin.trim()}>
                  {pinLoading ? t("verifying") : t("authorise")}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </button>
  );
}
