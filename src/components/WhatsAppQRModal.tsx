import { useEffect, useState, useCallback } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus } from "../tauri/commands";

interface Props {
  onClose: () => void;
  onConnected?: () => void;
}

export default function WhatsAppQRModal({ onClose, onConnected }: Props) {
  const [status, setStatus] = useState<WhatsAppStatus>({ connected: false });
  const [loading, setLoading] = useState(true);

  const poll = useCallback(async () => {
    try {
      const s = await whatsappStatus();
      setStatus(s);
      setLoading(false);
      if (s.connected) {
        onConnected?.();
        onClose();
      }
    } catch {
      setLoading(false);
    }
  }, [onClose, onConnected]);

  useEffect(() => {
    poll();
    const id = setInterval(poll, 20_000);
    return () => clearInterval(id);
  }, [poll]);

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onClose()}>
      <div className="modal wa-qr-modal">
        <div className="modal-header">
          <span className="modal-title">📱 Connect WhatsApp</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="wa-qr-body">
          {loading && <div className="wa-qr-hint">Connecting to sidecar…</div>}

          {!loading && !status.connected && !status.qr && (
            <div className="wa-qr-hint wa-qr-hint-warn">
              WhatsApp sidecar is not running.<br />
              Restart the app to reconnect.
            </div>
          )}

          {!loading && !status.connected && status.qr && (
            <>
              <p className="wa-qr-instruction">
                Open WhatsApp on your phone → <strong>Linked Devices</strong> → <strong>Link a Device</strong> and scan this QR code.
              </p>
              <div className="wa-qr-img-wrap">
                <img src={status.qr} alt="WhatsApp QR Code" className="wa-qr-img" />
              </div>
              <p className="wa-qr-hint">QR refreshes automatically every 20 seconds.</p>
            </>
          )}

          {!loading && status.connected && (
            <div className="wa-qr-hint wa-qr-connected">✅ WhatsApp connected!</div>
          )}
        </div>

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
