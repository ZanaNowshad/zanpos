import { useState, useEffect } from "react";
import type { CustomerRow, DeliveryInput } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";

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
  const [riderSuggestions, setRiderSuggestions] = useState<string[]>([]);
  const [showRiderDrop, setShowRiderDrop] = useState(false);

  // Pre-fill from selected customer
  useEffect(() => {
    if (selectedCustomer?.phone && !value.contact_number) {
      const raw = selectedCustomer.phone.replace(/^\+?973/, "");
      const normalized = normalizePhone(raw);
      if (normalized) onPhoneChange(raw, normalized, null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    onChange({ ...value, contact_number: normalized ?? "" });
    // We don't call onPhoneChange here to avoid a loop — parent already set phoneRaw
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phoneRaw]);

  const set = (field: keyof DeliveryInput, val: string) =>
    onChange({ ...value, [field]: val });

  const methodLabel =
    expectedPaymentMethod === "wallet" ? "BenefitPay" :
    expectedPaymentMethod.charAt(0).toUpperCase() + expectedPaymentMethod.slice(1);

  return (
    <div className="delivery-form">

      {/* Contact number — DIALPAD-CONNECTED */}
      <div className="delivery-field">
        <label className="delivery-label">
          Contact Number <span className="delivery-required">*</span>
          <span className="delivery-label-hint"> — tap to use dialpad</span>
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
              const err = raw && !normalized ? "Enter 8-digit Bahrain number (e.g. 33050666)" : null;
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
          <label className="delivery-label">Flat No</label>
          <input
            className="delivery-input"
            placeholder="Apt / Flat"
            value={value.area ?? ""}
            onChange={e => set("area", e.target.value)}
          />
        </div>
        <div className="delivery-field" style={{ flex: 1 }}>
          <label className="delivery-label">
            Bldg / House <span className="delivery-required">*</span>
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
          Road No <span className="delivery-required">*</span>
        </label>
        <input
          className="delivery-input"
          placeholder="Road / Block number"
          value={value.address_text ?? ""}
          onChange={e => set("address_text", e.target.value)}
        />
      </div>

      {/* Rider */}
      <div className="delivery-field" style={{ position: "relative" }}>
        <label className="delivery-label">Rider</label>
        <input
          className="delivery-input"
          placeholder="Staff name"
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
        Expected payment: <strong>{methodLabel}</strong>
      </div>
    </div>
  );
}
