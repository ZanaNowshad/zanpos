import { useEffect, useState } from "react";
import type { SessionUser, UserSummary } from "../types";
import { authListUsers, authLoginPin } from "../tauri/commands";

interface Props {
  onLogin: (user: SessionUser) => void;
}

export default function LoginScreen({ onLogin }: Props) {
  const [users, setUsers] = useState<UserSummary[]>([]);
  const [selected, setSelected] = useState<UserSummary | null>(null);
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    authListUsers().then(setUsers).catch(() => {});
  }, []);

  const handleUserSelect = (u: UserSummary) => {
    setSelected(u);
    setPin("");
    setError(null);
  };

  const handleKey = (key: string) => {
    setError(null);
    if (key === "C") {
      setPin("");
    } else if (key === "←") {
      setPin(p => p.slice(0, -1));
    } else if (key === "OK") {
      handleSubmit();
    } else if (pin.length < 6) {
      setPin(p => p + key);
    }
  };

  const handleSubmit = async () => {
    if (!selected || pin.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const user = await authLoginPin(selected.username, pin);
      onLogin(user);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Invalid PIN");
      setPin("");
    } finally {
      setLoading(false);
    }
  };

  const PAD = [
    ["1", "2", "3"],
    ["4", "5", "6"],
    ["7", "8", "9"],
    ["C", "0", "OK"],
  ];

  return (
    <div className="login-screen">
      <div className="login-brand">
        <div className="login-logo">ZAN<span>POS</span></div>
        <div className="login-tagline">Local-first Point of Sale</div>
      </div>

      <div className="login-panel">
        {!selected ? (
          <>
            <h2 className="login-heading">Select Cashier</h2>
            <div className="user-grid">
              {users.map(u => (
                <button key={u.user_id} className="user-card" onClick={() => handleUserSelect(u)}>
                  <div className="user-avatar">{u.display_name.charAt(0).toUpperCase()}</div>
                  <div className="user-name">{u.display_name}</div>
                  <div className="user-role">{u.role_name}</div>
                </button>
              ))}
            </div>
          </>
        ) : (
          <>
            <button className="login-back" onClick={() => { setSelected(null); setPin(""); setError(null); }}>
              ← Back
            </button>
            <div className="login-user-badge">
              <div className="user-avatar">{selected.display_name.charAt(0).toUpperCase()}</div>
              <div>
                <div className="user-name">{selected.display_name}</div>
                <div className="user-role">{selected.role_name}</div>
              </div>
            </div>

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
                      className={`pin-key ${key === "OK" ? "pin-key-ok" : key === "C" ? "pin-key-clear" : ""}`}
                      onClick={() => handleKey(key)}
                      disabled={loading}
                    >
                      {key}
                    </button>
                  ))}
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
