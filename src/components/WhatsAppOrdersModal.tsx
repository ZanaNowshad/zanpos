import { useCallback, useEffect, useMemo, useState } from "react";
import { X } from "lucide-react";
import type { WaOrder, WaMatchedLine } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator, ownedModalLabel } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  sessionUserId: string;
  /** Called with matched order lines the cashier chose to fulfil. Returns the
   *  count actually added so the modal can report partial fulfilment. */
  onAddToCart: (lines: WaMatchedLine[]) => Promise<number>;
  onClose: () => void;
}

const STATUSES = ["new", "reviewed", "fulfilled", "cancelled"] as const;

function fmtPrice(minor: number | null, currency: string | null): string {
  if (minor == null) return "—";
  return `${currency ?? "BHD"} ${(minor / 1000).toFixed(3)}`;
}

/** POS-side WhatsApp Orders command center: review, message the customer,
 *  send payment reminders, and fulfil an order into the POS cart. */
export default function WhatsAppOrdersModal({ sessionUserId, onAddToCart, onClose }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [filter, setFilter] = useState<string>("new");
  const [orders, setOrders] = useState<WaOrder[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msgFor, setMsgFor] = useState<WaOrder | null>(null);
  const [msgText, setMsgText] = useState("");
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      setOrders(await cmd.whatsappOrderList(sessionUserId, filter || undefined));
    } catch (e) { setError(String(e)); } finally { setLoading(false); }
  }, [sessionUserId, filter]);

  useEffect(() => { void load(); }, [load]);

  async function setStatus(o: WaOrder, status: string) {
    setBusy(o.order_id); setError(null);
    try { await cmd.whatsappOrderUpdateStatus(sessionUserId, o.order_id, status); await load(); }
    catch (e) { setError(String(e)); } finally { setBusy(null); }
  }

  async function fulfil(o: WaOrder) {
    setBusy(o.order_id); setError(null); setNote(null);
    try {
      const match = await cmd.whatsappOrderMatch(sessionUserId, o.order_id);
      if (match.matched.length === 0) {
        setNote(dt("noOrderItemsMatched"));
        return;
      }
      const added = await onAddToCart(match.matched);
      const missing = match.unmatched.length;
      await cmd.whatsappOrderUpdateStatus(sessionUserId, o.order_id, "reviewed");
      setNote(
        `${added} ${dt("addedToCart")}` +
        (missing > 0 ? ` · ${missing} ${dt("unmatchedItems")}` : "") +
        `. ${dt("completeThenFulfil")}`
      );
      await load();
    } catch (e) { setError(String(e)); } finally { setBusy(null); }
  }

  async function sendMessage() {
    if (!msgFor || !msgText.trim()) return;
    setBusy(msgFor.order_id); setError(null);
    try {
      const ok = await cmd.whatsappOrderMessage(sessionUserId, msgFor.order_id, msgText.trim());
      if (!ok) setError("Message not sent — is WhatsApp connected?");
      else setNote(dt("messageSent"));
      setMsgFor(null); setMsgText("");
    } catch (e) { setError(String(e)); } finally { setBusy(null); }
  }

  return (
    <div className="modal-overlay" onMouseDown={onClose}>
      <div className="modal bo-form-modal" style={{ maxWidth: 640, width: "92%", maxHeight: "88vh", display: "flex", flexDirection: "column" }} onMouseDown={e => e.stopPropagation()}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3 style={{ margin: 0 }}>{t("whatsappOrders")}</h3>
          <button className="bo-form-modal-close" onClick={onClose} aria-label={t("close")}><X size={18} /></button>
        </div>

        <div style={{ display: "flex", gap: 6, margin: "10px 0" }}>
          {["", ...STATUSES].map(s => (
            <button key={s || "all"}
              className={filter === s ? "btn-primary" : "btn-secondary"}
              style={{ fontSize: "0.78rem" }}
              onClick={() => setFilter(s)}
            >{s === "" ? t("all") : ownedModalLabel(language, s)}</button>
          ))}
        </div>

        {error && <div style={{ color: "var(--danger, #c0392b)", marginBottom: 8 }}>{error}</div>}
        {note && <div style={{ color: "var(--success, #2e7d32)", marginBottom: 8 }}>{note}</div>}

        <div style={{ overflowY: "auto", flex: 1 }}>
          {orders.map(o => (
            <div key={o.order_id} style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 12, marginBottom: 10 }}>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <strong>{o.customer_name ?? o.customer_jid}</strong>
                <span style={{ fontSize: "0.78rem", color: "var(--text-dim)" }}>{ownedModalLabel(language, o.status)} · {new Date(o.created_at).toLocaleString()}</span>
              </div>
              <div style={{ fontSize: "0.85rem", margin: "6px 0" }}>
                {o.products.map((l, i) => (
                  <div key={i}>{l.quantity}× {l.name} — {fmtPrice(l.price, l.currency)}</div>
                ))}
              </div>
              <div style={{ fontWeight: 600 }}>{t("total")}: {fmtPrice(o.total_minor, o.currency)}</div>
              <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
                <button className="btn-primary" style={{ fontSize: "0.78rem" }} disabled={busy === o.order_id} onClick={() => fulfil(o)}>
                  {t("fulfilToCart")}
                </button>
                <button className="btn-secondary" style={{ fontSize: "0.78rem" }} onClick={() => { setMsgFor(o); setMsgText(""); }}>{t("message")}</button>
                {o.status === "new" && <button className="btn-secondary" style={{ fontSize: "0.78rem" }} disabled={busy === o.order_id} onClick={() => setStatus(o, "reviewed")}>{t("reviewed")}</button>}
                {o.status !== "fulfilled" && o.status !== "cancelled" && (
                  <>
                    <button className="btn-secondary" style={{ fontSize: "0.78rem" }} disabled={busy === o.order_id} onClick={() => setStatus(o, "fulfilled")}>{t("fulfilled")}</button>
                    <button className="btn-secondary" style={{ fontSize: "0.78rem" }} disabled={busy === o.order_id} onClick={() => setStatus(o, "cancelled")}>{t("cancel")}</button>
                  </>
                )}
              </div>
            </div>
          ))}
          {orders.length === 0 && !loading && <div style={{ color: "var(--text-dim)" }}>{t("noOrders")}</div>}
        </div>

        {msgFor && (
          <div style={{ borderTop: "1px solid var(--border)", paddingTop: 10, marginTop: 8 }}>
            <div style={{ fontSize: "0.85rem", marginBottom: 4 }}>{t("messageTo")} {msgFor.customer_name ?? msgFor.customer_jid}</div>
            <textarea className="bo-input" rows={2} value={msgText} onChange={e => setMsgText(e.target.value)} placeholder={t("message")} style={{ width: "100%" }} />
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 6 }}>
              <button className="btn-secondary" onClick={() => setMsgFor(null)}>{t("cancel")}</button>
              <button className="btn-primary" disabled={!msgText.trim()} onClick={sendMessage}>{t("send")}</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
