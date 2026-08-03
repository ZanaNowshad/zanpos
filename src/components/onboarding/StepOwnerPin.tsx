import { useState } from "react";

export interface OwnerDraft {
  ownerName: string;
  ownerUsername: string;
  ownerPin: string;
}

interface Props {
  storeName: string;
  submitting: boolean;
  error: string | null;
  onSubmit: (draft: OwnerDraft) => void;
  onBack: () => void;
}

/** Step 2 — create the owner account. Submitting this step is what actually
 * writes the store + owner to the database (one atomic call owned by the
 * orchestrator), so PIN validation happens here before that call ever fires. */
export default function StepOwnerPin({ storeName, submitting, error, onSubmit, onBack }: Props) {
  const [ownerName, setOwnerName] = useState("");
  const [ownerUsername, setOwnerUsername] = useState("admin");
  const [ownerPin, setOwnerPin] = useState("");
  const [ownerPinConfirm, setOwnerPinConfirm] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);

  const handleSubmit = () => {
    if (!ownerName.trim()) { setLocalError("Owner name is required"); return; }
    if (!ownerUsername.trim()) { setLocalError("Username is required"); return; }
    if (ownerPin.length < 4 || ownerPin.length > 6) { setLocalError("PIN must be 4–6 digits"); return; }
    if (ownerPin !== ownerPinConfirm) { setLocalError("PINs do not match"); return; }
    setLocalError(null);
    onSubmit({ ownerName: ownerName.trim(), ownerUsername: ownerUsername.trim(), ownerPin });
  };

  return (
    <div className="setup-content">
      <h2 className="setup-title">Owner Account</h2>
      <p className="setup-body">
        Creating the owner account for <strong>{storeName || "your store"}</strong>. This account
        has full access — keep the PIN secure.
      </p>

      <label htmlFor="a11y-input-1" className="field-label">Full Name *</label>
      <input id="a11y-input-1"
        className="field-input"
        type="text"
        placeholder="e.g. Mohammed Al-Farsi"
        value={ownerName}
        onChange={e => { setOwnerName(e.target.value); setLocalError(null); }}

      />

      <label htmlFor="a11y-input-2" className="field-label">Username *</label>
      <input id="a11y-input-2"
        className="field-input"
        type="text"
        placeholder="admin"
        value={ownerUsername}
        onChange={e => { setOwnerUsername(e.target.value.toLowerCase().replace(/\s/g, "")); setLocalError(null); }}
      />

      <label htmlFor="a11y-input-3" className="field-label">PIN (4–6 digits) *</label>
      <input id="a11y-input-3"
        className="field-input setup-pin-input"
        type="password" inputMode="numeric" pattern="[0-9]*" maxLength={6}
        placeholder="Enter PIN"
        value={ownerPin}
        onChange={e => { setOwnerPin(e.target.value.replace(/\D/g, "")); setLocalError(null); }}
      />

      <label htmlFor="a11y-input-4" className="field-label">Confirm PIN *</label>
      <input id="a11y-input-4"
        className="field-input setup-pin-input"
        type="password" inputMode="numeric" pattern="[0-9]*" maxLength={6}
        placeholder="Re-enter PIN"
        value={ownerPinConfirm}
        onChange={e => { setOwnerPinConfirm(e.target.value.replace(/\D/g, "")); setLocalError(null); }}
      />

      {(localError || error) && <div className="modal-error">{localError || error}</div>}
      <div className="setup-actions">
        <button className="setup-btn-secondary" onClick={onBack} disabled={submitting}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
        <button className="setup-btn-primary" onClick={handleSubmit} disabled={submitting}>
          {submitting ? "Creating store…" : <>Create Store & Continue <span className="icon-directional" aria-hidden="true">→</span></>}
        </button>
      </div>
    </div>
  );
}
