import { useMemo, useRef, useState } from "react";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import Dialpad, { applyDialpadKey } from "./Dialpad";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  grossMinor:           number;
  currentDiscountMinor: number;
  onApply:  (discount_minor: number, reason: string) => void;
  onCancel: () => void;
}

type Mode = "pct" | "flat";

export default function DiscountModal({ grossMinor, currentDiscountMinor, onApply, onCancel }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [mode, setMode]     = useState<Mode>("pct");
  const [value, setValue]   = useState("");
  const [reason, setReason] = useState("");
  const confirmRef = useRef<HTMLButtonElement>(null);
  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;

  // ── Computed ──
  function computeMinor(): number {
    if (mode === "pct") {
      const bp = Math.round(parseFloat(value || "0") * 100);
      if (isNaN(bp) || bp <= 0) return 0;
      return Math.round(grossMinor * Math.min(bp, 10000) / 10000);
    }
    return Math.min(parseMoney(value, exp), grossMinor);
  }

  const preview    = computeMinor();
  const netAfter   = Math.max(0, grossMinor - preview);
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
    if (isValid) onApply(preview, reason.trim());
  };

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="cash-event-shell" role="dialog" aria-modal="true" aria-label={t("applyDiscount")}>

        {/* ── Left: form ── */}
        <div className="modal cash-event-left">
          <h2 className="modal-title">{t("applyDiscount")}</h2>

          {/* Mode tabs */}
          <div className="ce-type-tabs">
            <button
              className={`ce-type-tab${mode === "pct" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("pct"); setValue(""); }}
            >
              <span className="ce-tab-icon">%</span>
              <span className="ce-tab-label">{t("percent")}</span>
            </button>
            <button
              className={`ce-type-tab${mode === "flat" ? " ce-type-tab-active" : ""}`}
              onClick={() => { setMode("flat"); setValue(""); }}
            >
              <span className="ce-tab-icon">{cur}</span>
              <span className="ce-tab-label">{t("flatAmount")}</span>
            </button>
          </div>

          {/* Bill total reference */}
          <div className="discount-bill-ref">
            <span className="discount-bill-label">{t("billTotal")}</span>
            <span className="discount-bill-value">{cur} {formatMoney(grossMinor, exp)}</span>
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
    </div>
  );
}
