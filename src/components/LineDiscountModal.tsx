import { useMemo, useState, useRef } from "react";
import type { CartLine } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import { useAutoFocus } from "../hooks/useAutoFocus";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  line: CartLine;
  onApply: (discount_minor: number, reason: string) => void;
  onCancel: () => void;
}

export default function LineDiscountModal({ line, onApply, onCancel }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const modalRef = useRef<HTMLDivElement>(null);
  useFocusTrap(modalRef, onCancel);
  const inputRef = useAutoFocus<HTMLInputElement>();

  const exp = DEVICE.currency_exponent;
  const cur = DEVICE.currency;
  const fmt = (n: number) => `${cur} ${formatMoney(n, exp)}`;

  const total = Math.round(line.unit_price_minor * (parseFloat(line.quantity) || 1));
  const [value, setValue] = useState("");
  const [reason, setReason] = useState("");

  const discountMinor = parseMoney(value, exp);
  const valid = discountMinor > 0 && discountMinor <= total && reason.trim().length > 0;

  return (
    <button className="modal-overlay" type="button" onClick={onCancel}>
      <div ref={modalRef} className="modal" style={{ maxWidth: 380 }} role="dialog" aria-modal="true" aria-label={t("lineDiscount")} onClick={e => e.stopPropagation()} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
        <h2 className="modal-title">Discount — {line.product_name}</h2>
        <p style={{ fontSize: "0.82rem", color: "var(--text-dim)", marginBottom: 10 }}>
          Qty: {line.quantity} · Unit: {fmt(line.unit_price_minor)} · Line: {fmt(total)}
        </p>

        <label htmlFor="a11y-wrap-LineDiscountModal" className="line-edit-label">{t("discountAmount")}</label>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
          <span style={{ fontWeight: 700 }}>{cur}</span>
          <input id="a11y-wrap-LineDiscountModal"
            className="line-edit-qty-input"
            style={{ flex: 1 }}
            type="number"
            inputMode="decimal"
            min="0"
            step={Math.pow(10, -exp).toFixed(exp)}
            placeholder={`0.${"0".repeat(exp)}`}
            value={value}
            onChange={e => setValue(e.target.value)}
            ref={inputRef}
          />
        </div>

        <label htmlFor="a11y-input-1" className="line-edit-label">{t("reasonRequired")}</label>
        <input id="a11y-input-1"
          className="line-edit-note-input"
          type="text"
          maxLength={120}
          placeholder={dt("damagedDiscountExample")}
          value={reason}
          onChange={e => setReason(e.target.value)}
          style={{ marginBottom: 12 }}
        />

        <div className="modal-actions line-edit-actions">
          <button className="btn-secondary" onClick={onCancel}>{t("cancel")}</button>
          <button className="btn-primary" onClick={() => onApply(discountMinor, reason.trim())} disabled={!valid}>
            {t("applyDiscount")} −{fmt(discountMinor)}
          </button>
        </div>
      </div>
    </button>
  );
}
