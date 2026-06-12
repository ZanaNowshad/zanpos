import { useEffect, useState } from "react";
import type { Cart, HeldCartSummary } from "../types";
import { DEVICE } from "../types";
import { heldCartSave, heldCartList, heldCartResume, heldCartDelete } from "../tauri/commands";
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

  const handleHold = async () => {
    if (lineCount === 0) return;
    setSaving(true);
    setError(null);
    try {
      await heldCartSave(actorUserId, cart, note || undefined);
      onHeld();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to hold cart");
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
      setError(typeof e === "string" ? e : "Failed to resume cart");
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
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal hold-modal" role="dialog" aria-modal="true" aria-labelledby="hold-dialog-title" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title" id="hold-dialog-title">Hold Order</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {lineCount > 0 && (
          <div className="hold-current">
            <div className="hold-current-info">
              <span>{lineCount} item{lineCount !== 1 ? "s" : ""}</span>
              <span>{DEVICE.currency} {formatMoney(netTotal, DEVICE.currency_exponent)}</span>
            </div>
            {/* T20: label the held order with a customer name so cashiers can quickly identify it */}
            <input
              className="field-input"
              placeholder="Customer name (optional)…"
              value={note}
              maxLength={60}
              autoFocus
              onChange={e => setNote(e.target.value)}
            />
            {error && <div className="modal-error">{error}</div>}
            <button className="modal-btn-primary" onClick={handleHold} disabled={saving}>
              {saving ? "Holding…" : "Hold This Order"}
            </button>
          </div>
        )}

        {heldCarts.length > 0 && (
          <>
            <div className="hold-divider">Held Orders</div>
            <div className="held-list">
              {heldCarts.map(h => (
                <div key={h.held_cart_id} className="held-item">
                  <div className="held-item-info">
                    {/* T20: show customer name prominently; fall back to "Unnamed order" */}
                    <div className="held-item-note">{h.note || <span className="held-item-unnamed">Unnamed order</span>}</div>
                    <div className="held-item-meta">
                      {h.line_count} items · {DEVICE.currency} {formatMoney(h.estimated_total_minor, DEVICE.currency_exponent)}
                    </div>
                    <div className="held-item-time">{new Date(h.held_at).toLocaleTimeString()}</div>
                  </div>
                  <div className="held-item-actions">
                    <button
                      className="held-btn-resume"
                      onClick={() => handleResume(h.held_cart_id)}
                      disabled={resumingIds.has(h.held_cart_id)}
                    >
                      {resumingIds.has(h.held_cart_id) ? "Resuming…" : "Resume"}
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
          <p className="hold-empty">No orders on hold.</p>
        )}
      </div>
    </div>
  );
}
