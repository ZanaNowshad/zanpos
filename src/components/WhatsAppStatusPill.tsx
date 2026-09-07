import { memo, useEffect, useState, useCallback } from "react";
import type { WhatsAppStatus } from "../types";
import { whatsappStatus } from "../tauri/commands";
import type { SessionToken } from "../types";

interface Props {
  sessionRole: string;
  sessionToken: SessionToken;
  onOpenQR: () => void;
}

const WhatsAppStatusPill = memo(function WhatsAppStatusPill({ sessionRole, sessionToken, onOpenQR }: Props) {
  const [status, setStatus] = useState<WhatsAppStatus>({ connected: false });

  const poll = useCallback(async () => {
    try {
      const s = await whatsappStatus(sessionToken);
      setStatus(s);
    } catch { /* sidecar not running */ }
  }, [sessionToken]);

  useEffect(() => {
    poll();
    const id = setInterval(poll, 30_000);
    return () => clearInterval(id);
  }, [poll]);

  const isManager = sessionRole === "owner" || sessionRole === "manager";
  const isPending = !status.connected && !!status.qr;

  const label = isPending ? "⟳ WA" : "WA";
  const cls   = `wa-pill ${
    status.connected ? "wa-pill-on" : isPending ? "wa-pill-pending" : "wa-pill-off"
  }`;

  return (
    <button
      className={cls}
      onClick={() => { if (isManager && !status.connected) onOpenQR(); }}
      title={
        status.connected ? "WhatsApp connected"
        : isPending      ? "WhatsApp — scan QR to connect"
        :                  isManager ? "WhatsApp disconnected — click to connect" : "WhatsApp disconnected"
      }
      disabled={!isManager || status.connected}
      style={{ cursor: isManager && !status.connected ? "pointer" : "default" }}
    >
      <span className="wa-pill-dot" />
      {label}
    </button>
  );
});

export default WhatsAppStatusPill;
