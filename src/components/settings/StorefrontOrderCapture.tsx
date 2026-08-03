import { useEffect, useState } from "react";
import { LoaderCircle, MessageCircle } from "lucide-react";
import * as cmd from "../../tauri/commands";

interface Props { sessionUserId: string; }

/** Master ON/OFF for WhatsApp order capture, shown as the closing step of the
 *  storefront journey: customers browse the shop, order over WhatsApp, and the
 *  order lands on the POS Orders page. When OFF, the POS Orders page is hidden,
 *  no order capture runs, and every order command refuses. */
export default function StorefrontOrderCapture({ sessionUserId }: Props) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    cmd.whatsappCommerceGetEnabled(sessionUserId)
      .then(value => { if (!cancelled) setEnabled(value); })
      .catch(e => { if (!cancelled) { setError(String(e)); setEnabled(false); } });
    return () => { cancelled = true; };
  }, [sessionUserId]);

  async function toggle(next: boolean) {
    setSaving(true);
    setError(null);
    try {
      await cmd.whatsappCommerceSetEnabled(sessionUserId, next);
      setEnabled(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="sf-section" aria-labelledby="sf-capture-title">
      <div className="sf-section-heading">
        <div><span>03 · Order capture</span><h3 id="sf-capture-title">Orders arriving on WhatsApp</h3></div>
        {enabled === null ? (
          <LoaderCircle className="sf-capture-spinner" aria-label="Loading order capture" />
        ) : (
          <label htmlFor="a11y-input-1" className="sf-capture-toggle">
            <span>{enabled ? "Capturing orders" : "Not capturing"}</span>
            <input id="a11y-input-1"
              type="checkbox"
              checked={enabled}
              disabled={saving}
              onChange={event => void toggle(event.target.checked)}
            />
            <span className="sf-toggle-track" aria-hidden="true" />
          </label>
        )}
      </div>
      <p className="sf-capture-note">
        <MessageCircle aria-hidden="true" />
        Customers order from your shop over WhatsApp, the message lands in your inbox, and the
        order appears on the POS Orders page ready to fulfil. Keep your WhatsApp number paired —
        if it drops, the Orders page goes quiet.
      </p>
      {error && <p className="sf-capture-error" role="alert">{error}</p>}
    </section>
  );
}
