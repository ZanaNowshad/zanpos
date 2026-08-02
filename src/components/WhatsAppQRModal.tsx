import { useEffect, useState, useCallback, useMemo } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus } from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  onClose: () => void;
  onConnected?: () => void;
  sessionUserId?: string;
}

export default function WhatsAppQRModal({ onClose, onConnected, sessionUserId = "" }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [status, setStatus]     = useState<WhatsAppStatus>({ connected: false });
  const [loading, setLoading]   = useState(true);
  const [unreachable, setUnreachable] = useState(false);
  const [, setPollCount]     = useState(0);

  const poll = useCallback(async () => {
    try {
      const s = await whatsappStatus(sessionUserId);
      setStatus(s);
      setUnreachable(false);
      setLoading(false);
      setPollCount(n => n + 1);
      if (s.connected) {
        onConnected?.();
        onClose();
      }
    } catch {
      setUnreachable(true);
      setLoading(false);
    }
  }, [onClose, onConnected, sessionUserId]);

  useEffect(() => {
    poll();
    const id = setInterval(poll, 3_000);
    return () => clearInterval(id);
  }, [poll]);

  // While we have a response but no QR yet, the sidecar is warming up — keep polling
  const waitingForQr = !loading && !unreachable && !status.connected && !status.qr;

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="modal wa-qr-modal">
        <div className="modal-header">
          <span className="modal-title">📱 Connect WhatsApp</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="wa-qr-body">
          {/* Sidecar unreachable — genuine error */}
          {unreachable && (
            <div className="wa-qr-hint wa-qr-hint-warn">
              ⚠️ WhatsApp sidecar is not responding.<br />
              Restart the app and try again.
            </div>
          )}

          {/* Sidecar reachable but QR not ready yet (warming up) */}
          {(loading || waitingForQr) && !unreachable && (
            <div className="wa-qr-hint wa-qr-warming">
              <span className="wa-qr-spinner">⟳</span>
              {loading ? dt("connectingSidecar") : dt("generatingQr")}
            </div>
          )}

          {/* QR ready — show it */}
          {!loading && !unreachable && !status.connected && status.qr && (
            <>
              <p className="wa-qr-instruction">
                {t("whatsappLinkInstructions")}
              </p>
              <div className="wa-qr-img-wrap">
                <img src={status.qr} alt={dt("whatsappQrAlt")} className="wa-qr-img" />
              </div>
              <p className="wa-qr-hint">{t("qrRefreshHint")}</p>
            </>
          )}

          {/* Already connected */}
          {!loading && status.connected && (
            <div className="wa-qr-hint wa-qr-connected">✅ WhatsApp connected!</div>
          )}
        </div>

        <div className="modal-actions">
          {/* Retry button shown only when unreachable */}
          {unreachable && (
            <button className="btn-primary" onClick={poll}>{t("retry")}</button>
          )}
          <button className="btn-secondary" onClick={onClose}>{t("close")}</button>
        </div>
      </div>
    </div>
  );
}
