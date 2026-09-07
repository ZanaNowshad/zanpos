import { useEffect, useState } from "react";
import type { WhatsAppStatus } from "../../types";
import { whatsappStatus } from "../../tauri/commands";
import WhatsAppQRModal from "../WhatsAppQRModal";
import type { SessionToken } from "../../types";

interface Props {
  sessionToken: SessionToken;
  onDone: () => void;
}

/** Step 3 — pair WhatsApp. Reuses WhatsAppQRModal verbatim; skippable, with an
 * explicit note that pairing can happen later in Settings. */
export default function StepWhatsApp({ sessionToken, onDone }: Props) {
  const [status, setStatus] = useState<WhatsAppStatus>({ connected: false });
  const [checked, setChecked] = useState(false);
  const [showQr, setShowQr] = useState(false);

  useEffect(() => {
    let cancelled = false;
    whatsappStatus(sessionToken)
      .then(s => { if (!cancelled) setStatus(s); })
      .catch(() => { /* sidecar not up yet — connect button still available */ })
      .finally(() => { if (!cancelled) setChecked(true); });
    return () => { cancelled = true; };
  }, [sessionToken]);

  return (
    <div className="setup-content">
      <h2 className="setup-title">Pair WhatsApp</h2>
      <p className="setup-body">
        ZANPOS can send order confirmations and receipts to customers over WhatsApp.
        You can skip this and pair later in Settings.
      </p>

      {status.connected ? (
        <div className="setup-wa-section">
          <div className="setup-wa-title">✅ WhatsApp connected</div>
        </div>
      ) : (
        <div className="setup-wa-section">
          <div className="setup-wa-title">📱 Connect WhatsApp</div>
          <p className="setup-field-hint">
            {checked ? "Not connected yet." : "Checking status…"}
          </p>
          <button className="setup-btn-secondary" type="button" onClick={() => setShowQr(true)}>
            Connect WhatsApp (Scan QR)
          </button>
        </div>
      )}

      <div className="setup-actions">
        {status.connected ? (
          <button className="setup-btn-primary" onClick={onDone}>Continue <span className="icon-directional" aria-hidden="true">→</span></button>
        ) : (
          <button className="setup-btn-secondary" onClick={onDone}>
            Skip — you can pair later in Settings
          </button>
        )}
      </div>

      {showQr && (
        <WhatsAppQRModal
          sessionToken={sessionToken}
          onConnected={() => setStatus(s => ({ ...s, connected: true }))}
          onClose={() => setShowQr(false)}
        />
      )}
    </div>
  );
}
