import { useEffect, useRef, useState } from "react";
import { authValidateManagerPin } from "../tauri/commands";

interface Props {
  /** What the manager is being asked to approve, shown above the PIN field. */
  action: string;
  /** Receives a single-use override token proving a manager's PIN was entered. */
  onApproved: (overrideToken: string) => Promise<void>;
  onCancel: () => void;
}

/**
 * Ask a manager to approve an action at the till.
 *
 * The PIN is exchanged for a short-lived single-use token and only the token
 * travels onward. Nothing here reports success on its own: `onApproved` is
 * awaited, so a backend refusal surfaces as an error in this dialog rather than
 * closing over a failure. Naming a manager used to be enough on its own, which
 * is what this replaces.
 */
export default function ManagerApprovalModal({ action, onApproved, onCancel }: Props) {
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pinRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const t = setTimeout(() => pinRef.current?.focus(), 50);
    return () => clearTimeout(t);
  }, []);

  const submit = async () => {
    if (!pin.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const token = await authValidateManagerPin(pin);
      setPin("");
      // Awaited, and the token is passed directly rather than through state —
      // a state update is not visible in this closure until the next render.
      await onApproved(token);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? "Approval failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <button
      className="modal-overlay"
      type="button"
      onClick={e => e.target === e.currentTarget && !busy && onCancel()}
    >
      <div className="modal">
        <h2 className="modal-title">Manager approval</h2>
        <div className="pi-hint">{action}</div>

        <input
          ref={pinRef}
          className="ce-amount-input"
          type="password"
          inputMode="numeric"
          autoComplete="off"
          value={pin}
          placeholder="Manager PIN"
          onChange={e => {
            setPin(e.target.value);
            setError(null);
          }}
          onKeyDown={e => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            } else if (e.key === "Escape") {
              e.preventDefault();
              if (!busy) onCancel();
            }
          }}
        />

        {error && <div className="modal-error">{error}</div>}

        <div className="dialpad-actions">
          <button
            className="dialpad-confirm-btn"
            onClick={submit}
            disabled={!pin.trim() || busy}
          >
            {busy ? "Checking…" : "✓ Approve"}
          </button>
          <button className="dialpad-cancel-btn" onClick={onCancel} disabled={busy}>
            ✕ Cancel
          </button>
        </div>
      </div>
    </button>
  );
}
