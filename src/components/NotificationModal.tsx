import { useCallback, useEffect, useRef, useState } from "react";
import { AlertTriangle, Bell, CheckCircle2, Image, MessageCircle, UsersRound } from "lucide-react";
import type { AiHandoff, CatalogImportProposal, GhostBarcode, PaymentConfirmation, ProductPrefill, WaMessage } from "../types";
import { DEVICE } from "../types";
import {
  ghostList, ghostResolve, ghostDismiss, ghostPrefill,
  whatsappListMessages, whatsappMarkRead, whatsappMarkAllRead, whatsappGetMedia,
  whatsappClearMessages,
  paymentConfirmationsList, paymentConfirmationsMarkAllSeen, paymentConfirmationOverride,
  catalogImportExtract,
} from "../tauri/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator } from "../i18n/operationsStrings";
import { modalText } from "../i18n/modalStrings";
import { detailText } from "../i18n/detailStrings";
import CatalogImportModal from "./CatalogImportModal";
import { canImportCatalogFromMessage, relativeMessageTime } from "./notificationPresentation";
export { canImportCatalogFromMessage } from "./notificationPresentation";
interface Props {
  sessionUserId: string;
  sessionToken: string;
  onClose: () => void;
  /** Hand a resolved barcode off to OfficeAI's product create form. */
  onCreateProduct: (prefill: ProductPrefill) => void;
  /** Send a WhatsApp message (and its image, if any) to the OfficeAI assistant. */
  onSendToAi: (handoff: AiHandoff) => void;
  /** Called whenever the notification count changes (resolve/dismiss/create). */
  onCountChange: () => void;
}

export default function NotificationModal({
  sessionUserId,
  sessionToken,
  onClose,
  onCreateProduct,
  onSendToAi,
  onCountChange,
}: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const modalRef = useRef<HTMLDivElement>(null);

  const [items, setItems]         = useState<GhostBarcode[]>([]);
  const [waMsgs, setWaMsgs]       = useState<WaMessage[]>([]);
  const [payConfs, setPayConfs]   = useState<PaymentConfirmation[]>([]);
  const [loading, setLoading]     = useState(true);
  const [resolving, setResolving] = useState(false);
  const [error, setError]         = useState<string | null>(null);

  const [viewing, setViewing]         = useState<WaMessage | null>(null);
  const [mediaSrc, setMediaSrc]       = useState<string | null>(null);
  const [mediaLoading, setMediaLoading] = useState(false);
  const [mediaError, setMediaError]   = useState<string | null>(null);

  const [catalogProposal, setCatalogProposal] = useState<CatalogImportProposal | null>(null);
  const [ciLoadingId, setCiLoadingId]         = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [ghosts, msgs, confs] = await Promise.all([
        ghostList(sessionUserId),
        whatsappListMessages(sessionUserId).catch(() => [] as WaMessage[]),
        paymentConfirmationsList(sessionUserId).catch(() => [] as PaymentConfirmation[]),
      ]);
      setItems(ghosts);
      setWaMsgs(msgs);
      setPayConfs(confs);
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedNotifications"));
    } finally {
      setLoading(false);
    }
  }, [language, sessionUserId]);

  useEffect(() => { load(); }, [load]);

  // Closing the popup marks the WhatsApp messages as read (they've been seen),
  // which clears the bell badge. Ghost barcodes still require explicit action.
  const handleClose = useCallback(() => {
    if (waMsgs.some(m => !m.read)) {
      whatsappMarkAllRead(sessionUserId).then(onCountChange).catch(() => {});
    }
    if (payConfs.some(c => !c.seen)) {
      paymentConfirmationsMarkAllSeen(sessionUserId).then(onCountChange).catch(() => {});
    }
    onClose();
  }, [waMsgs, payConfs, sessionUserId, onCountChange, onClose]);

  const overridePay = async (id: string, confirm: boolean) => {
    try {
      await paymentConfirmationOverride(id, confirm, sessionUserId);
      setPayConfs(prev => prev.map(c => (c.id === id
        ? { ...c, status: confirm ? "confirmed" : "failed", seen: true } : c)));
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedUpdatePayment"));
    }
  };

  useFocusTrap(modalRef, handleClose);

  const markWaRead = async (id: string) => {
    try {
      await whatsappMarkRead(id, sessionUserId);
      setWaMsgs(prev => prev.map(m => (m.id === id ? { ...m, read: true } : m)));
      onCountChange();
    } catch { /* non-fatal */ }
  };

  const clearWa = async () => {
    try {
      await whatsappClearMessages(sessionUserId);
      setWaMsgs([]);
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedClear"));
    }
  };

  // Open the full-message view; fetches the decrypted image for photo messages.
  const openView = async (m: WaMessage) => {
    setViewing(m);
    setMediaSrc(null);
    setMediaError(null);
    if (!m.read) markWaRead(m.id);
    if (m.media_type === "image") {
      setMediaLoading(true);
      try {
        const r = await whatsappGetMedia(m.id, sessionUserId);
        if (r.ok && r.base64) {
          setMediaSrc(`data:${r.mimetype || "image/jpeg"};base64,${r.base64}`);
        } else {
          setMediaError(detailText(language, "photoUnavailable"));
        }
      } catch {
        setMediaError(detailText(language, "photoLoadFailed"));
      } finally {
        setMediaLoading(false);
      }
    }
  };

  const sendToAi = async (m: WaMessage) => {
    if (!m.read) markWaRead(m.id);
    const who = m.is_group
      ? `${m.chat_name || detailText(language, "storeGroup")}${m.sender_name ? ` (${m.sender_name})` : ""}`
      : (m.chat_name || detailText(language, "owner"));
    const text = `WhatsApp message from ${who}:\n\n"${m.body}"`;
    // Attach the photo so the assistant can read prices/products off the image.
    if (m.media_type === "image") {
      try {
        const r = await whatsappGetMedia(m.id, sessionUserId);
        if (r.ok && r.base64) {
          onSendToAi({ text, imageBase64: r.base64, imageMediaType: r.mimetype || "image/jpeg" });
          return;
        }
        // Image file gone (sidecar restarted, media expired) — tell the user instead of
        // sending a text-only message that makes the AI say "I can't view images."
        setError(detailText(language, "photoCacheLost"));
        return;
      } catch {
        setError(detailText(language, "whatsappReconnectPhoto"));
        return;
      }
    }
    onSendToAi({ text });
  };

  const openCatalogImport = async (m: WaMessage) => {
    setCiLoadingId(m.id);
    setError(null);
    try {
      const proposal = await catalogImportExtract(m.id, DEVICE.currency_exponent, sessionToken);
      if (proposal.lines.length === 0) {
        setError(t("noProductLines"));
        return;
      }
      setCatalogProposal(proposal);
    } catch (e) {
      setError(typeof e === "string" ? e : t("priceListReadFailed"));
    } finally {
      setCiLoadingId(null);
    }
  };

  const pending = items.filter(i => i.status === "pending").length;

  const handleResolve = async () => {
    setResolving(true);
    setError(null);
    try {
      await ghostResolve(sessionUserId);
      await load();
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedLookup"));
    } finally {
      setResolving(false);
    }
  };

  const handleDismiss = async (id: string) => {
    try {
      await ghostDismiss(id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== id));
      onCountChange();
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedDismiss"));
    }
  };

  const handleCreate = async (item: GhostBarcode) => {
    try {
      // 'found' items use the resolved lookup data; for pending/not_found we still
      // let the admin add the product manually, pre-filling just the barcode.
      const prefill: ProductPrefill = item.status === "found"
        ? await ghostPrefill(item.id, sessionUserId)
        : {
            name: item.product_name ?? "",
            barcode: item.barcode,
            brand: item.brand,
            category: item.category,
            image_url: item.image_url,
          };
      await ghostDismiss(item.id, sessionUserId);
      setItems(prev => prev.filter(i => i.id !== item.id));
      onCountChange();
      onCreateProduct(prefill);
    } catch (e) {
      setError(typeof e === "string" ? e : detailText(language, "failedOpenProductForm"));
    }
  };

  return (
    <>
    <button className="modal-overlay" type="button" onClick={handleClose}>
      <div
        ref={modalRef}
        className="modal ghost-modal notif-modal"
        role="dialog"
        aria-modal="true"
        aria-label={modalText(language, "notifications")}
        onClick={e => e.stopPropagation()}
      >
        <div className="ghost-modal-header">
          <div className="ghost-modal-title">
            <h2>{modalText(language, "notifications")}</h2>
            <p className="ghost-modal-sub">
              {detailText(language, "notificationsDescription")}
            </p>
          </div>
          <button className="bo-form-modal-close" onClick={handleClose} aria-label={modalText(language, "close")}>✕</button>
        </div>

        <div className="ghost-modal-body">
          {error && <div className="ghost-error">{error}</div>}

          {payConfs.length > 0 && (
            <div className="wa-msg-section">
              <div className="notif-section-head">
                <span className="notif-section-label">{modalText(language, "payments")}</span>
              </div>
              <div className="ghost-list">
                {payConfs.map(c => {
                  const amount = (c.expected_amount_minor / 10 ** c.currency_exponent).toFixed(c.currency_exponent);
                  const who = c.customer_name || c.customer_jid.split("@")[0];
                  const tsSec = Math.floor(new Date(c.resolved_at || c.created_at).getTime() / 1000);
                  return (
                    <div key={c.id} className={`ghost-item wa-msg${c.seen ? "" : " wa-msg-unread"}`}>
                      <div className="ghost-item-left">
                        <span className="wa-msg-avatar" aria-hidden="true">
                          {c.status === "confirmed" ? <CheckCircle2 size={17} /> : <AlertTriangle size={17} />}
                        </span>
                        <div className="ghost-item-info">
                          <div className="ghost-item-name">
                            {!c.seen && <span className="wa-msg-dot" aria-hidden="true" />}
                            {c.status === "confirmed"
                              ? `${detailText(language, "zanAiConfirmedPayment")} — ${who}`
                              : `${detailText(language, "couldNotVerifyPayment")} — ${who}`}
                          </div>
                          <div className="wa-msg-body">
                            {c.status === "confirmed"
                              ? `BHD ${amount} ${detailText(language, "forReceipt")} #${c.receipt_number}`
                              : `${modalText(language, "receipt")} #${c.receipt_number} · ${detailText(language, "expectedAmount")} BHD ${amount}${c.reason ? ` — ${c.reason}` : ""}`}
                          </div>
                          <div className="ghost-item-barcode">{relativeMessageTime(tsSec)}</div>
                        </div>
                      </div>
                      {c.status === "failed" && (
                        <div className="ghost-item-right wa-msg-actions">
                          <button className="ghost-btn ghost-btn-ai" onClick={() => overridePay(c.id, true)}>{modalText(language, "confirm")}</button>
                          <button className="ghost-btn ghost-btn-dismiss" onClick={() => overridePay(c.id, false)}>{modalText(language, "reject")}</button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {waMsgs.length > 0 && (
            <div className="wa-msg-section">
              <div className="notif-section-head">
                <span className="notif-section-label">WhatsApp</span>
                <button className="notif-clear-btn" onClick={clearWa}>{modalText(language, "clearAll")}</button>
              </div>
              {waMsgs.some(m => m.media_type === "image") && (
                <p className="ghost-modal-hint notif-ai-hint">
                  {detailText(language, "visionModelHint")}
                </p>
              )}
              <div className="ghost-list">
                {waMsgs.map(m => (
                  <div key={m.id} className={`ghost-item wa-msg${m.read ? "" : " wa-msg-unread"}`}>
                    <div className="ghost-item-left">
                      <span className="wa-msg-avatar" aria-hidden="true">
                        {m.is_group ? <UsersRound size={17} /> : <MessageCircle size={17} />}
                      </span>
                      <div className="ghost-item-info">
                        <div className="ghost-item-name">
                          {!m.read && <span className="wa-msg-dot" aria-hidden="true" />}
                          {m.chat_name || (m.is_group ? detailText(language, "storeGroup") : detailText(language, "owner"))}
                          {m.is_group && m.sender_name && (
                            <span className="wa-msg-sender"> · {m.sender_name}</span>
                          )}
                        </div>
                        <div className="wa-msg-body">
                          {m.media_type === "image" && <span className="wa-msg-tag"><Image size={12} /> {modalText(language, "photo")}</span>}
                          {m.body}
                        </div>
                        <div className="ghost-item-barcode">{relativeMessageTime(m.ts)}</div>
                      </div>
                    </div>
                    <div className="ghost-item-right wa-msg-actions">
                      <button className="ghost-btn ghost-btn-view" onClick={() => openView(m)}>
                        {detailText(language, "view")}
                      </button>
                      <button className="ghost-btn ghost-btn-ai" onClick={() => sendToAi(m)}>
                        {detailText(language, "sendToAi")}
                      </button>
                      {canImportCatalogFromMessage(m) && (
                        <button className="ghost-btn ghost-btn-create" onClick={() => openCatalogImport(m)} disabled={ciLoadingId === m.id}>
                          {t(ciLoadingId === m.id ? "reading" : "priceList")}
                        </button>
                      )}
                      {!m.read && (
                        <button className="ghost-btn ghost-btn-dismiss" onClick={() => markWaRead(m.id)}>
                          {detailText(language, "markRead")}
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {(waMsgs.length > 0 && (items.length > 0 || pending > 0)) && (
            <div className="notif-section-label">{modalText(language, "barcodes")}</div>
          )}

          {pending > 0 && (
            <div className="ghost-actions-row">
              <button className="ghost-lookup-btn" onClick={handleResolve} disabled={resolving}>
                {resolving
                  ? detailText(language, "lookingUp")
                  : `${detailText(language, "lookUpBarcodes")} (${pending})`}
              </button>
              <span className="ghost-modal-hint">{modalText(language, "barcodeSearchHint")}</span>
            </div>
          )}

          {loading && <div className="ghost-loading">{modalText(language, "loading")}</div>}

          {!loading && items.length > 0 && (
            <div className="ghost-list">
              {items.map(item => (
                <div key={item.id} className={`ghost-item ghost-item-${item.status}`}>
                  <div className="ghost-item-left">
                    {item.image_url ? (
                      <img
                        src={item.image_url}
                        alt={item.product_name ?? ""}
                        className="ghost-item-img"
                        onError={e => { (e.target as HTMLImageElement).style.display = "none"; }}
                      />
                    ) : <span className="notif-item-dot" aria-hidden="true" />}
                    <div className="ghost-item-info">
                      {item.status === "found" ? (
                        <>
                          <div className="ghost-item-name">{item.product_name}</div>
                          {item.brand    && <div className="ghost-item-meta">{item.brand}</div>}
                          {item.category && <div className="ghost-item-meta">{item.category}</div>}
                        </>
                      ) : item.status === "not_found" ? (
                        <div className="ghost-item-name ghost-not-found">
                          {detailText(language, "unknownBarcodeNotFound")}
                        </div>
                      ) : (
                        <div className="ghost-item-name ghost-pending">
                          {detailText(language, "unknownBarcodeScanned")}
                        </div>
                      )}
                      <div className="ghost-item-barcode">
                        #{item.barcode}
                        <span className="ghost-item-count">· {detailText(language, "scanned")} {item.scan_count}×</span>
                      </div>
                    </div>
                  </div>

                  <div className="ghost-item-right">
                    <button
                      className="ghost-btn ghost-btn-create"
                      onClick={() => handleCreate(item)}
                    >
                      {item.status === "found" ? `+ ${detailText(language, "addProduct")}` : `+ ${detailText(language, "addManually")}`}
                    </button>
                    <button
                      className="ghost-btn ghost-btn-dismiss"
                      onClick={() => handleDismiss(item.id)}
                    >
                      {modalText(language, "dismiss")}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {!loading && items.length === 0 && waMsgs.length === 0 && payConfs.length === 0 && (
            <div className="ghost-empty">
              <div className="notif-empty-icon"><Bell size={28} /></div>
              {detailText(language, "allCaughtUp")}
            </div>
          )}
        </div>
      </div>
    </button>

    {viewing && (
      <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="modal-overlay wa-view-overlay" onClick={() => setViewing(null)}>
        <div
          className="modal wa-view-modal"
          role="dialog"
          aria-modal="true"
          aria-label={detailText(language, "whatsappMessage")}
          onClick={e => e.stopPropagation()}
        >
          <div className="wa-view-header">
            <div className="wa-view-title">
              <span className="wa-msg-avatar" aria-hidden="true">
                {viewing.is_group ? <UsersRound size={17} /> : <MessageCircle size={17} />}
              </span>
              <span>
                {viewing.chat_name || (viewing.is_group ? detailText(language, "storeGroup") : detailText(language, "owner"))}
                {viewing.is_group && viewing.sender_name && (
                  <span className="wa-msg-sender"> · {viewing.sender_name}</span>
                )}
              </span>
            </div>
            <button className="bo-form-modal-close" onClick={() => setViewing(null)} aria-label={modalText(language, "close")}>✕</button>
          </div>
          <div className="wa-view-body">
            {viewing.media_type === "image" && (
              mediaLoading ? (
                <div className="wa-view-loading">{modalText(language, "loading")} {modalText(language, "photo")}</div>
              ) : mediaSrc ? (
                <img className="wa-view-img" src={mediaSrc} alt="" />
              ) : mediaError ? (
                <div className="wa-view-error">{mediaError}</div>
              ) : null
            )}
            {viewing.body && <p className="wa-view-text">{viewing.body}</p>}
          </div>
          <div className="wa-view-footer">
            <button className="btn-secondary" onClick={() => { const m = viewing; setViewing(null); sendToAi(m); }}>
              {detailText(language, "sendToAi")}
            </button>
            <button className="btn-primary" onClick={() => setViewing(null)}>{modalText(language, "done")}</button>
          </div>
        </div>
      </div>
    )}

    {catalogProposal && (
      <CatalogImportModal
        proposal={catalogProposal}
        sessionToken={sessionToken}
        onClose={() => setCatalogProposal(null)}
        onApplied={onCountChange}
      />
    )}
    </>
  );
}
