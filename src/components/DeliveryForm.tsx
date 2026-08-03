import { useState, useEffect } from "react";
import type { CustomerRow, DeliveryInput } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  value: Partial<DeliveryInput>;
  onChange: (v: Partial<DeliveryInput>) => void;
  selectedCustomer: CustomerRow | null;
  expectedPaymentMethod: string;
  actorUserId: string;
  // Phone is controlled by parent so the dialpad can write to it
  phoneRaw: string;
  phoneError: string | null;
  onPhoneChange: (raw: string, normalized: string | null, error: string | null) => void;
  onPhoneFocus: () => void;
}

/** Normalize a Bahrain phone number to E.164: +97333050666 */
export function normalizePhone(raw: string): string | null {
  let s = raw.replace(/[\s\-()]/g, "").replace(/^0+/, "");
  if (s.startsWith("+973")) s = s.slice(4);
  else if (s.startsWith("973")) s = s.slice(3);
  if (!/^\d{8}$/.test(s)) return null;
  return `+973${s}`;
}

export default function DeliveryForm({
  value, onChange, selectedCustomer, expectedPaymentMethod,
  actorUserId, phoneRaw, phoneError, onPhoneChange, onPhoneFocus,
}: Props) {
  const { language } = useLanguage();
  const t = detailTranslator(language);
  const [riderSuggestions, setRiderSuggestions] = useState<string[]>([]);
  const [showRiderDrop, setShowRiderDrop] = useState(false);

  // Pre-fill from selected customer
  useEffect(() => {
    if (selectedCustomer?.phone && !value.contact_number) {
      // FIX: robust strip handles +973, 973, 00973, and bare 8-digit numbers
      const raw = selectedCustomer.phone.replace(/\D/g, "").replace(/^(00)?973/, "").slice(0, 8);
      const normalized = normalizePhone(raw);
      if (normalized) onPhoneChange(raw, normalized, null);
    }
  }, [selectedCustomer]);

  // Load rider suggestions
  useEffect(() => {
    let cancelled = false;
    cmd.deliveryRiderSuggestions(DEVICE.branch_id, actorUserId)
      .then(data => { if (!cancelled) setRiderSuggestions(data); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [actorUserId]);

  // Keep parent deliveryData phone in sync when phoneRaw changes via dialpad
  useEffect(() => {
    if (!phoneRaw) return;
    const normalized = normalizePhone(phoneRaw);
    // FIX: don't overwrite a valid contact_number with "" when normalization fails
    // (partial entry of < 8 digits). Only update when we have a valid E.164 number.
    if (normalized) {
      onChange({ ...value, contact_number: normalized });
    }
    // We don't call onPhoneChange here to avoid a loop — parent already set phoneRaw
  }, [phoneRaw]);

  const set = (field: keyof DeliveryInput, val: string) =>
    onChange({ ...value, [field]: val });

  const methodLabel =
    expectedPaymentMethod === "wallet" ? "BenefitPay" :
    expectedPaymentMethod === "cash" ? t("cash") :
    expectedPaymentMethod === "card" ? t("card") :
    expectedPaymentMethod;

  return (
    <div className="delivery-form">

      {/* Contact number — DIALPAD-CONNECTED */}
      <div className="delivery-field">
        <label className="delivery-label">
          {t("contactNumber")} <span className="delivery-required">*</span>
          <span className="delivery-label-hint"> — {t("tapUseDialpad")}</span>
        </label>
        <div className="delivery-phone-row">
          <span className="delivery-phone-prefix">+973</span>
          <input
            className={`delivery-input delivery-phone-input${phoneError ? " delivery-input-error" : ""}`}
            placeholder="33050666"
            value={phoneRaw}
            onChange={e => {
              const raw = e.target.value.replace(/\D/g, "").slice(0, 8);
              const normalized = normalizePhone(raw);
              const err = raw && !normalized ? t("invalidBahrainNumber") : null;
              onPhoneChange(raw, normalized, err);
            }}
            onFocus={onPhoneFocus}
            maxLength={8}
            inputMode="numeric"
          />
        </div>
        {phoneError && <span className="delivery-error-hint">{phoneError}</span>}
      </div>

      {/* Flat No + Bldg/House in one row */}
      <div className="delivery-field-row">
        <div className="delivery-field" style={{ flex: 1 }}>
          <label htmlFor="a11y-input-1" className="delivery-label">{t("flatNumber")}</label>
          <input id="a11y-input-1"
            className="delivery-input"
            placeholder={t("apartmentFlat")}
            value={value.area ?? ""}
            onChange={e => set("area", e.target.value)}
          />
        </div>
        <div className="delivery-field" style={{ flex: 1 }}>
          <label className="delivery-label">
            {t("buildingHouse")} <span className="delivery-required">*</span>
          </label>
          <input
            className="delivery-input"
            placeholder="12"
            value={value.house_number ?? ""}
            onChange={e => set("house_number", e.target.value)}
          />
        </div>
      </div>

      {/* Road Number */}
      <div className="delivery-field">
        <label className="delivery-label">
          {t("roadNumber")} <span className="delivery-required">*</span>
        </label>
        <input
          className="delivery-input"
          placeholder={t("roadBlockNumber")}
          value={value.address_text ?? ""}
          onChange={e => set("address_text", e.target.value)}
        />
      </div>

      {/* Rider */}
      <div className="delivery-field" style={{ position: "relative" }}>
        <label htmlFor="a11y-input-2" className="delivery-label">{t("rider")}</label>
        <input id="a11y-input-2"
          className="delivery-input"
          placeholder={t("staffName")}
          value={value.delivery_staff_name ?? ""}
          onChange={e => { set("delivery_staff_name", e.target.value); setShowRiderDrop(true); }}
          onFocus={() => setShowRiderDrop(true)}
          onBlur={() => setTimeout(() => setShowRiderDrop(false), 150)}
        />
        {showRiderDrop && riderSuggestions.filter(r =>
          !value.delivery_staff_name ||
          r.toLowerCase().includes((value.delivery_staff_name ?? "").toLowerCase())
        ).length > 0 && (
          <div className="delivery-rider-dropdown">
            {riderSuggestions
              .filter(r => !value.delivery_staff_name ||
                r.toLowerCase().includes((value.delivery_staff_name ?? "").toLowerCase()))
              .map(r => (
                <button key={r} className="delivery-rider-item"
                  onMouseDown={() => { set("delivery_staff_name", r); setShowRiderDrop(false); }}>
                  {r}
                </button>
              ))}
          </div>
        )}
      </div>

      <div className="delivery-method-note">
        {t("expectedPayment")}: <strong>{methodLabel}</strong>
      </div>
    </div>
  );
}
