import type { DeliveryInput } from "../types";

interface Props {
  value: Partial<DeliveryInput>;
  onChange: (value: Partial<DeliveryInput>) => void;
  expectedPaymentMethod: string;
}

/** Normalize a Bahrain phone number to E.164: +97333050666. */
export function normalizePhone(raw: string): string | null {
  let value = raw.replace(/[\s\-()]/g, "").replace(/^0+/, "");
  if (value.startsWith("+973")) value = value.slice(4);
  else if (value.startsWith("973")) value = value.slice(3);
  if (!/^\d{8}$/.test(value)) return null;
  return `+973${value}`;
}

export default function DeliveryForm({ value, onChange, expectedPaymentMethod }: Props) {
  const set = (field: keyof DeliveryInput, next: string) => onChange({ ...value, [field]: next });
  const methodLabel = expectedPaymentMethod === "wallet"
    ? "BenefitPay"
    : expectedPaymentMethod.charAt(0).toUpperCase() + expectedPaymentMethod.slice(1);

  return (
    <div className="delivery-form pm-address-form">
      {/* Only the house number is required. Flat and road were both mandatory,
          which blocked the sale for a villa with no flat and for addresses a
          customer gives as a landmark — the rider still finds them. */}
      <div className="pm-address-heading">
        <span>Delivery address</span>
        <small>House number required · flat and road optional</small>
      </div>
      <div className="pm-address-grid">
        <div className="delivery-field">
          <label htmlFor="payment-house" className="delivery-label">House number <span className="delivery-required">*</span></label>
          <input id="payment-house" className="delivery-input" inputMode="text" enterKeyHint="next" autoComplete="address-line1" placeholder="12" value={value.house_number ?? ""} onChange={event => set("house_number", event.target.value)} />
        </div>
        <div className="delivery-field">
          <label htmlFor="payment-flat" className="delivery-label">Flat</label>
          <input id="payment-flat" className="delivery-input" inputMode="text" enterKeyHint="next" autoComplete="address-line2" placeholder="4B (optional)" value={value.area ?? ""} onChange={event => set("area", event.target.value)} />
        </div>
        <div className="delivery-field pm-road-field">
          <label htmlFor="payment-road" className="delivery-label">Road</label>
          <input id="payment-road" className="delivery-input" inputMode="text" enterKeyHint="done" autoComplete="address-line3" placeholder="Road or block number (optional)" value={value.address_text ?? ""} onChange={event => set("address_text", event.target.value)} />
        </div>
      </div>
      <div className="delivery-method-note">Expected payment: <strong>{methodLabel}</strong></div>
    </div>
  );
}
