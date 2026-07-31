import { useMemo, useRef, useState } from "react";
import type { CartLine } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  line: CartLine;
  onUpdateQty: (id: string, qty: string) => void;
  onApplyLineDiscount: (id: string, discount_minor: number, reason: string) => void;
  onSetLineNote: (id: string, note: string | null) => void;
  onRemove: (id: string) => void;
  onClose: () => void;
}

type DiscountMode = "pct" | "flat";

export default function LineEditModal({
  line,
  onUpdateQty,
  onApplyLineDiscount,
  onSetLineNote,
  onRemove,
  onClose,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onClose);

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;
  const fmt = (n: number) => `${cur} ${formatMoney(n, exp)}`;

  const [qty, setQty] = useState(line.quantity);
  const [discountMode, setDiscountMode] = useState<DiscountMode>("pct");
  const [discountValue, setDiscountValue] = useState(
    line.line_discount_minor > 0 ? "" : ""
  );
  const [discountReason, setDiscountReason] = useState(line.line_discount_reason ?? "");
  const [note, setNote] = useState(line.note ?? "");

  const qtyNum = parseFloat(qty);
  const qtyValid = !isNaN(qtyNum) && qtyNum > 0;
  // Display-only preview: integer math avoids float rounding in subtotal
  const lineSubtotal = qtyValid
    ? Math.round(line.unit_price_minor * qtyNum)
    : line.unit_price_minor;

  function computeDiscountMinor(): number {
    if (discountMode === "pct") {
      const bp = Math.round(parseFloat(discountValue || "0") * 100);
      if (isNaN(bp) || bp <= 0) return 0;
      return Math.round(lineSubtotal * Math.min(bp, 10000) / 10000);
    }
    // flat discount
    return Math.min(parseMoney(discountValue, exp), lineSubtotal);
  }

  const discountPreview = computeDiscountMinor();
  const reasonTrimmed = discountReason.trim();
  const discountNeedsReason = discountPreview > 0 && reasonTrimmed.length === 0;

  function handleApply() {
    if (!qtyValid || discountNeedsReason) return;
    if (qty !== line.quantity) {
      onUpdateQty(line.cart_line_id, qty);
    }
    if (discountPreview !== line.line_discount_minor) {
      onApplyLineDiscount(line.cart_line_id, discountPreview, reasonTrimmed);
    }
    const noteVal = note.trim() || null;
    if (noteVal !== (line.note ?? null)) {
      onSetLineNote(line.cart_line_id, noteVal);
    }
    onClose();
  }

  function handleRemove() {
    onRemove(line.cart_line_id);
    onClose();
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        ref={modalRef}
        className="modal line-edit-modal"
        role="dialog"
        aria-modal="true"
        aria-label={`Edit ${line.product_name}`}
        onClick={e => e.stopPropagation()}
      >
        <h2 className="modal-title">{line.product_name}</h2>
        <p className="line-edit-unit-price">Unit price: {fmt(line.unit_price_minor)}</p>

        {/* ── Quantity ── */}
        <label className="line-edit-label" htmlFor="line-edit-qty">{t("quantity")}</label>
        <div className="line-edit-qty-row">
          <button
            className="qty-btn"
            onClick={() => {
              const q = Math.max(1, parseFloat(qty) - 1);
              setQty(String(q));
            }}
          >−</button>
          <input
            id="line-edit-qty"
            className="line-edit-qty-input"
            type="number"
            inputMode="decimal"
            min="0.001"
            step="1"
            value={qty}
            onChange={e => setQty(e.target.value)}
          />
          <button
            className="qty-btn"
            onClick={() => setQty(String(parseFloat(qty) + 1))}
          >+</button>
        </div>

        {/* ── Line discount ── */}
        <label className="line-edit-label" htmlFor="line-edit-discount">{t("lineDiscount")}</label>
        <div className="discount-mode-tabs">
          <button
            className={`discount-mode-tab ${discountMode === "pct" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setDiscountMode("pct"); setDiscountValue(""); }}
          >%</button>
          <button
            className={`discount-mode-tab ${discountMode === "flat" ? "discount-mode-tab-active" : ""}`}
            onClick={() => { setDiscountMode("flat"); setDiscountValue(""); }}
          >{t("flatAmount")}</button>
        </div>
        <div className="discount-input-row">
          {discountMode === "pct" ? (
            <>
              <input
                id="line-edit-discount"
                className="discount-input"
                type="number"
                inputMode="numeric"
                min="0" max="100" step="1"
                placeholder="0"
                value={discountValue}
                onChange={e => setDiscountValue(e.target.value)}
              />
              <span className="discount-input-suffix">%</span>
            </>
          ) : (
            <>
              <span className="discount-input-prefix">{cur}</span>
              <input
                className="discount-input"
                type="number"
                inputMode="decimal"
                min="0"
                step={Math.pow(10, -exp).toFixed(exp)}
                placeholder={`0.${"0".repeat(exp)}`}
                value={discountValue}
                onChange={e => setDiscountValue(e.target.value)}
              />
            </>
          )}
        </div>
        {discountPreview > 0 && (
          <p className="discount-current-note">
            Discount: − {fmt(discountPreview)}
            &nbsp;<span className="icon-directional" aria-hidden="true">→</span> Line total:{" "}
            <span className="numeric-ltr">{fmt(Math.max(0, lineSubtotal - discountPreview))}</span>
          </p>
        )}

        {discountPreview > 0 && (
            <>
              <label className="line-edit-label" htmlFor="line-edit-discount-reason">{t("reasonRequired")}</label>
              <input
                id="line-edit-discount-reason"
              className={`line-edit-note-input${discountNeedsReason ? " input-error" : ""}`}
              type="text"
              maxLength={120}
              placeholder={dt("managerDiscountExample")}
              value={discountReason}
              onChange={e => setDiscountReason(e.target.value)}
            />
          </>
        )}

        {/* ── Note ── */}
        <label className="line-edit-label" htmlFor="line-edit-note">{t("noteOptional")}</label>
        <input
          id="line-edit-note"
          className="line-edit-note-input"
          type="text"
          maxLength={120}
          placeholder={dt("lineNoteExample")}
          value={note}
          onChange={e => setNote(e.target.value)}
        />

        <div className="modal-actions line-edit-actions">
          <button className="btn-danger" onClick={handleRemove}>{t("remove")}</button>
          <button className="btn-secondary" onClick={onClose}>{t("cancel")}</button>
          <button className="btn-primary" onClick={handleApply} disabled={!qtyValid || discountNeedsReason}>
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
