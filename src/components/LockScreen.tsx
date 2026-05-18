import { useState } from "react";
import type { SessionUser } from "../types";
import { authLoginPin } from "../tauri/commands";

interface Props {
  user: SessionUser;
  onUnlock: () => void;
  onLogout: () => void;
}

export default function LockScreen({ user, onUnlock, onLogout }: Props) {
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleKey = (key: string) => {
    setError(null);
    if (key === "←") {
      setPin(p => p.slice(0, -1));
    } else if (key === "OK") {
      handleSubmit();
    } else if (pin.length < 6) {
      setPin(p => p + key);
    }
  };

  const handleSubmit = async () => {
    if (pin.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      await authLoginPin(user.username, pin);
      onUnlock();
    } catch {
      setError("Incorrect PIN");
      setPin("");
    } finally {
      setLoading(false);
    }
  };

  const PAD = [
    ["1", "2", "3"],
    ["4", "5", "6"],
    ["7", "8", "9"],
    ["←", "0", "OK"],
  ];

  return (
    <div className="lock-overlay">
      <div className="lock-panel">
        <div className="lock-icon">🔒</div>
        <h2 className="lock-title">Session Locked</h2>
        <p className="lock-subtitle">
          Re-enter your PIN to continue as <strong>{user.display_name}</strong>
        </p>

        <div className="pin-display">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className={`pin-dot ${i < pin.length ? "pin-dot-filled" : ""}`} />
          ))}
        </div>

        {error && <div className="login-error">{error}</div>}

        <div className="pin-pad">
          {PAD.map((row, ri) => (
            <div key={ri} className="pin-row">
              {row.map(key => (
                <button
                  key={key}
                  className={`pin-key ${key === "OK" ? "pin-key-ok" : key === "←" ? "pin-key-clear" : ""}`}
                  onClick={() => handleKey(key)}
                  disabled={loading}
                >
                  {key}
                </button>
              ))}
            </div>
          ))}
        </div>

        <button className="lock-logout-btn" onClick={onLogout}>
          Switch User
        </button>
      </div>
    </div>
  );
}
