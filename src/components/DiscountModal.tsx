import { useMemo, useRef, useState } from "react";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import Dialpad, { applyDialpadKey } from "./Dialpad";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";
import type { CartLine } from "../types";

interface Props {
  grossMinor: number;
  currentBillDiscountMinor: number;
  lines: CartLine[];
  initialLineId?: string;
  onApplyBill: (discount_minor: number, reason: string) => void;
  onApplyLine: (line_id: string, discount_minor: number, reason: string) => void;
  onCancel: () => void;
}

export type DiscountMode = "pct" | "flat";
type Scope = "bill" | "item";

export function calculateDiscountMinor(
  mode: DiscountMode,
  value: string,
  baseMinor: number,
  currencyExponent: number,
): number {
  if (mode === "pct") {
    const basisPoints = Math.round(parseFloat(value || "0") * 100);
    if (Number.isNaN(basisPoints) || basisPoints <= 0) return 0;
    return Math.round(baseMinor * Math.min(basisPoints, 10_000) / 10_000);
  }
  return Math.min(parseMoney(value, currencyExponent), baseMinor);
}

export default function DiscountModal({
  grossMinor,
  currentBillDiscountMinor,
  lines,
  initialLineId,
  onApplyBill,
  onApplyLine,
  onCancel,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const initialItemId = lines.some(line => line.cart_line_id === initialLineId)
    ? initialLineId
    : lines[0]?.cart_line_id;
  const [scope, setScope] = useState<Scope>(initialLineId && initialItemId ? "item" : "bill");
  const [selectedLineId, setSelectedLineId] = useState(initialItemId ?? "");
  const [mode, setMode]     = useState<DiscountMode>("pct");
  const [value, setValue]   = useState("");
  const [reason, setReason] = useState("");
  const confirmRef = useRef<HTMLButtonElement>(null);
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  const selectedLine = lines.find(line => line.cart_line_id === selectedLineId);
  const lineSubtotalMinor = selectedLine
    ? Math.round(selectedLine.unit_price_minor * (parseFloat(selectedLine.quantity) || 0))
    : 0;
  const discountBaseMinor = scope === "bill" ? grossMinor : lineSubtotalMinor;
  const currentDiscountMinor = scope === "bill"
    ? currentBillDiscountMinor
    : selectedLine?.line_discount_minor ?? 0;

  // ── Computed ──
  function computeMinor(): number {
    return calculateDiscountMinor(mode, value, discountBaseMinor, exp);
  }

  const preview    = computeMinor();
  const netAfter   = Math.max(0, discountBaseMinor - preview);
  const pctDisplay = mode === "pct" && value ? `${value}%` : null;
  const isValid    = preview > 0 && reason.trim().length > 0;

  // ── Dialpad ──
  const handleDialpadKey = (key: string) => {
    // For percent mode: block decimal, cap at "100"
    if (mode === "pct") {
      if (key === "." || key === "00") return;
      const next = applyDialpadKey(value, key);
      const num = parseInt(next, 10);
      if (!isNaN(num) && num > 100) return;
      setValue(next);
    } else {
      setValue(prev => applyDialpadKey(prev, key));
    }
  };

  const handleApply = () => {
    if (!isValid) return;
    if (scope === "item" && selectedLine) {
      onApplyLine(selectedLine.cart_line_id, preview, reason.trim());
    } else if (scope === "bill") {
      onApplyBill(preview, reason.trim());
    }
  };

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="cash-event-shell" role="dialog" aria-modal="true" aria-label={t("applyDiscount")}>

        {/* ── Left: form ── */}
        <div className="modal cash-event-left">
          <h2 className="modal-title">{t("applyDiscount")}</h2>

          <div className="discount-scope-tabs" role="radiogroup" aria-label={t("discountAppliesTo")}>
            <button
              type="button"
              role="radio"
              aria-checked={scope === "bill"}
              className={`discount-scope-tab${scope === "bill" ? " discount-scope-tab-active" : ""}`}
              onClick={() => { setScope("bill"); setValue(""); }}
            >
              <span className="discount-scope-title">{t("wholeBill")}</span>
              <span className="discount-scope-detail">{t("everyItemThisSale")}</span>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={scope === "item"}
              className={`discount-scope-tab${scope === "item" ? " discount-scope-tab-active" : ""}`}
              disabled={lines.length === 0}
              onClick={() => { setScope("item"); setValue(""); }}
            >
              <span className="discount-scope-title">{t("individualItem")}</span>
              <span className="discount-scope-detail">{t("oneSelectedCartLine")}</span>
            </button>
          </div>

          {scope === "item" && (
            <label className="discount-item-picker">
              <span>{t("selectItem")}</span>
              <select value={selectedLineId} onChange={event => { setSelectedLineId(event.target.value); setValue(""); }}>
                {lines.map(line => (
                  <option key={line.cart_line_id} value={line.cart_line_id}>
                    {line.product_name} × {line.quantity} — {cur} {formatMoney(Math.round(line.unit_price_minor * (parseFloat(line.quantity) || 0)), exp)}
                  </option>
                ))}
              </select>
            </label>
          )}

          {/* Mode tabs */}
          <div className="ce-type-tabs">
            <button
              className={`ce-type-tab${mode === "pct" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("pct"); setValue(""); }}
            >
              <span className="ce-tab-icon">%</span>
              <span className="ce-tab-label">{t("percentage")}</span>
            </button>
            <button
              className={`ce-type-tab${mode === "flat" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("flat"); setValue(""); }}
            >
              <span className="ce-tab-icon">{cur}</span>
              <span className="ce-tab-label">{t("fixedAmount")}</span>
            </button>
          </div>

          {/* Discount base reference */}
          <div className="discount-bill-ref">
            <span className="discount-bill-label">{scope === "bill" ? t("billTotal") : t("itemSubtotal")}</span>
            <span className="discount-bill-value">{cur} {formatMoney(discountBaseMinor, exp)}</span>
          </div>

          {/* Amount display */}
          <div className="ce-amount-block">
            {mode === "pct" ? (
              <>
                <span className={`ce-amount-value${!value ? " ce-amount-placeholder" : ""}`}
                  style={value ? { color: "var(--accent)" } : {}}>
                  {value || "0"}
                </span>
                <span className="ce-amount-cur">%</span>
              </>
            ) : (
              <>
                <span className="ce-amount-cur">{cur}</span>
                <span className={`ce-amount-value${!value ? " ce-amount-placeholder" : ""}`}
                  style={value ? { color: "var(--accent)" } : {}}>
                  {value || `0.${"0".repeat(exp)}`}
                </span>
              </>
            )}
            <span className="touch-cursor">|</span>
          </div>

          {/* Live preview */}
          {preview > 0 && (
            <div className="discount-preview-block">
              <div className="discount-preview-row">
                <span>{t("discounts")}</span>
                <span className="discount-preview-off">− {cur} {formatMoney(preview, exp)}</span>
              </div>
              <div className="discount-preview-row discount-preview-net">
                <span>{t("newTotal")}</span>
                <span className="discount-preview-new">{cur} {formatMoney(netAfter, exp)}</span>
              </div>
            </div>
          )}

          {currentDiscountMinor > 0 && (
            <p className="discount-current-note">
              Current discount: {cur} {formatMoney(currentDiscountMinor, exp)} — will be replaced.
            </p>
          )}

          {/* Reason */}
          <label className="ce-note-label" htmlFor="discount-reason">{t("reasonRequired")}</label>
          <input
            id="discount-reason"
            className="ce-note-input"
            type="text"
            maxLength={120}
            placeholder={dt("managerDiscountExample")}
            value={reason}
            onChange={e => setReason(e.target.value)}
            onKeyDown={e => e.key === "Enter" && handleApply()}
          />
        </div>

        {/* ── Right: dialpad ── */}
        <div className="payment-dialpad-panel">
          <div className="dialpad-field-indicator">
            {mode === "pct" ? dt("enterPercent") : `${dt("enterAmount")} (${cur})`}
          </div>

          <Dialpad onKey={handleDialpadKey} />

          <div className="dialpad-actions">
            <button
              ref={confirmRef}
              className="dialpad-confirm-btn"
              onClick={handleApply}
              disabled={!isValid}
            >
              {isValid ? (
                <>
                  <span className="dialpad-confirm-icon">✓</span>
                  <span className="dialpad-confirm-label">{t("applyDiscount")}</span>
                  {pctDisplay
                    ? <span className="dialpad-confirm-total">{pctDisplay}</span>
                    : preview > 0
                    ? <span className="dialpad-confirm-total">− {cur} {formatMoney(preview, exp)}</span>
                    : null}
                </>
              ) : (
                <span className="dialpad-confirm-label">{t("applyDiscount")}</span>
              )}
            </button>
            <button className="dialpad-cancel-btn" onClick={onCancel}>
              ✕ Cancel
            </button>
          </div>
        </div>

      </div>
    </button>
  );
}
