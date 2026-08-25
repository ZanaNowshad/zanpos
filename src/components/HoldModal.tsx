import { useEffect, useState } from "react";
import type { Cart, HeldCartSummary } from "../types";
import { DEVICE } from "../types";
import { heldCartSave, heldCartList, heldCartResume, heldCartDelete } from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { countText, operationsTranslator } from "../i18n/operationsStrings";
import { formatMoney } from "../money";

interface Props {
  cart: Cart;
  lineCount: number;
  netTotal: number;
  onHeld: () => void;
  onResume: (cart: Cart) => void;
  onClose: () => void;
  actorUserId: string;
}

export default function HoldModal({ cart, lineCount, netTotal, onHeld, onResume, onClose, actorUserId }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const [note, setNote] = useState("");
  const [heldCarts, setHeldCarts] = useState<HeldCartSummary[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // BUG-POS-9: prevent double-resume — track which held_cart_ids are in-flight
  const [resumingIds, setResumingIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    heldCartList(actorUserId, DEVICE.device_id).then(data => { if (!cancelled) setHeldCarts(data); }).catch(() => {});
    return () => { cancelled = true; };
  }, [actorUserId]);

  /* Escape closes the dialog. Capture phase and propagation stopped, because
     the till binds Escape to "put the caret back in the barcode field" — a
     bubbling handler here would fire second and the cashier would be typing
     barcodes into a dialog that never went away. */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const handleHold = async () => {
    if (lineCount === 0) return;
    setSaving(true);
    setError(null);
    try {
      await heldCartSave(actorUserId, cart, note || undefined);
      onHeld();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("holdCartFailed"));
    } finally {
      setSaving(false);
    }
  };

  const handleResume = async (held_cart_id: string) => {
    // BUG-POS-9: guard against double-tap/double-click resuming the same cart twice
    if (resumingIds.has(held_cart_id)) return;
    setResumingIds(prev => new Set(prev).add(held_cart_id));
    try {
      const resumed = await heldCartResume(actorUserId, held_cart_id, cart.shift_id);
      // Await delete — fire-and-forget left the cart resumable again on failure
      await heldCartDelete(actorUserId, held_cart_id).catch(() => {});
      setHeldCarts(prev => prev.filter(h => h.held_cart_id !== held_cart_id));
      onResume(resumed);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("resumeCartFailed"));
      setResumingIds(prev => { const s = new Set(prev); s.delete(held_cart_id); return s; });
    }
  };

  const handleDelete = async (held_cart_id: string) => {
    try {
      await heldCartDelete(actorUserId, held_cart_id);
      setHeldCarts(prev => prev.filter(h => h.held_cart_id !== held_cart_id));
    } catch {
      // ignore
    }
  };

  return (
    /*
     * A div, not a button. The overlay used to be a `<button>` wrapping the
     * whole dialog, which made every control inside it a nested interactive
     * element: pressing Space anywhere the caret was not closed the dialog and
     * lost the note. The inner `onKeyDown` that went with it turned Enter and
     * Space into a synthetic click on whatever had focus, so a space could not
     * be typed into the customer-name field at all — "Abu Ali" came out
     * "AbuAli". Both are gone; the dismiss target is a sibling behind the
     * dialog, as elsewhere in the app.
     */
    <div className="modal-overlay" role="presentation">
      <button
        type="button"
        className="modal-overlay-dismiss"
        onClick={onClose}
        tabIndex={-1}
        aria-hidden="true"
      />
      <div className="modal hold-modal" role="dialog" aria-modal="true" aria-labelledby="hold-dialog-title">
        <div className="modal-header">
          <h2 className="modal-title" id="hold-dialog-title">{t("holdOrder")}</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {lineCount > 0 && (
          <div className="hold-current">
            <div className="hold-current-info">
              <span>{countText(language, "items", lineCount)}</span>
              <span>{DEVICE.currency} {formatMoney(netTotal, DEVICE.currency_exponent)}</span>
            </div>
            {/* T20: label the held order with a customer name so cashiers can quickly identify it */}
            <input
              className="field-input"
              placeholder={t("customerNameOptional")}
              value={note}
              maxLength={60}

              onChange={e => setNote(e.target.value)}
            />
            {error && <div className="modal-error">{error}</div>}
            <button className="modal-btn-primary" onClick={handleHold} disabled={saving}>
              {t(saving ? "holding" : "holdThisOrder")}
            </button>
          </div>
        )}

        {heldCarts.length > 0 && (
          <>
            <div className="hold-divider">{t("heldOrders")}</div>
            <div className="held-list">
              {heldCarts.map(h => (
                <div key={h.held_cart_id} className="held-item">
                  <div className="held-item-info">
                    {/* T20: show customer name prominently; fall back to "Unnamed order" */}
                    <div className="held-item-note">{h.note || <span className="held-item-unnamed">{t("unnamedOrder")}</span>}</div>
                    <div className="held-item-meta">
                      {countText(language, "items", h.line_count)} · {DEVICE.currency} {formatMoney(h.estimated_total_minor, DEVICE.currency_exponent)}
                    </div>
                    <div className="held-item-time">{new Date(h.held_at).toLocaleTimeString()}</div>
                  </div>
                  <div className="held-item-actions">
                    <button
                      className="held-btn-resume"
                      onClick={() => handleResume(h.held_cart_id)}
                      disabled={resumingIds.has(h.held_cart_id)}
                    >
                      {t(resumingIds.has(h.held_cart_id) ? "resuming" : "resume")}
                    </button>
                    <button className="held-btn-delete" onClick={() => handleDelete(h.held_cart_id)}>
                      ✕
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        {heldCarts.length === 0 && lineCount === 0 && (
          <p className="hold-empty">{t("noHeldOrders")}</p>
        )}
      </div>
    </div>
  );
}
