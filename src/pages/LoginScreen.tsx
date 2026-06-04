import { useEffect, useRef, useState } from "react";
import type { SessionUser, UserSummary } from "../types";
import { authListUsers, authLoginPin } from "../tauri/commands";
import ZanposBootLogo from "../components/ZanposBootLogo";

interface Props {
  onLogin: (user: SessionUser) => void;
}

export default function LoginScreen({ onLogin }: Props) {
  const [users, setUsers] = useState<UserSummary[]>([]);
  const [selected, setSelected] = useState<UserSummary | null>(null);
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Refs to avoid stale closures in the keydown listener
  const selectedRef   = useRef(selected);
  const pinRef         = useRef(pin);
  const loadingRef     = useRef(loading);
  const submitRef      = useRef<(() => Promise<void>) | null>(null);
  useEffect(() => { selectedRef.current = selected; }, [selected]);
  useEffect(() => { pinRef.current = pin; }, [pin]);
  useEffect(() => { loadingRef.current = loading; }, [loading]);

  // ── Physical keyboard support for PIN entry ───────────────────────────────
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!selectedRef.current || loadingRef.current) return;
      if (e.key >= "0" && e.key <= "9") {
        e.preventDefault();
        setError(null);
        setPin(p => p.length < 6 ? p + e.key : p);
      } else if (e.key === "Backspace") {
        e.preventDefault();
        setError(null);
        setPin(p => p.slice(0, -1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        // Trigger submit using current ref values
        const cur = selectedRef.current;
        const curPin = pinRef.current;
        if (cur && curPin.length > 0) {
          // Simulate click on OK by calling handleSubmit indirectly via the
          // same async path — we trigger it via a microtask so React state settles
          setTimeout(() => submitRef.current?.(), 0);
        }
      } else if (e.key === "Escape") {
        e.preventDefault();
        setSelected(null);
        setPin("");
        setError(null);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      authListUsers()
        .then(list => {
          if (cancelled) return;
          setUsers(list);
        })
        .catch((e: unknown) => {
          if (cancelled) return;
          // F-MED-12: Backend now returns "rate_limit" error instead of empty list.
          // Retry after the 3-second window; all other errors are genuine failures.
          const msg = typeof e === "string" ? e : String(e);
          if (msg.includes("rate_limit")) {
            setTimeout(() => { if (!cancelled) load(); }, 3200);
          }
          // else: genuine error — leave users empty, login screen shows "no users" state
        });
    };
    load();
    return () => { cancelled = true; };
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
    } else if (key === "⌫") {
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
  useEffect(() => { submitRef.current = handleSubmit; });

  const PAD = [
    ["1", "2", "3"],
    ["4", "5", "6"],
    ["7", "8", "9"],
    ["⌫", "0", "OK"],
  ];

  return (
    <div className="login-screen">
      {/* Geometric diagonal border shapes */}
      <div className="login-geo-1" />
      <div className="login-geo-2" />
      <div className="login-geo-glow" />

      {/* Left brand panel */}
      <div className="login-brand">
        <ZanposBootLogo />
      </div>

      {/* Right — floating card */}
      <div className="login-right">
        <div className="login-panel">
          {!selected ? (
            <>
              <div className="login-panel-icon">👥</div>
              <h2 className="login-heading">Select Cashier</h2>
              <p className="login-subheading">Choose your profile to continue</p>
              <div className="user-grid">
                {users.map(u => (
                  <button key={u.user_id} className="user-card" onClick={() => handleUserSelect(u)}>
                    <div className="user-avatar">{u.display_name.charAt(0).toUpperCase()}</div>
                    <div className="user-name">{u.display_name}</div>
                    <div className="user-role">{u.role_name}</div>
                    <div className="user-arrow">→</div>
                  </button>
                ))}
              </div>
            </>
          ) : (
            <>
              {/* User badge at top */}
              <div className="login-user-badge">
                <div className="user-avatar">{selected.display_name.charAt(0).toUpperCase()}</div>
                <div className="user-name">{selected.display_name}</div>
                <div className="user-role">{selected.role_name}</div>
              </div>

              {/* PIN heading */}
              <div className="pin-enter-label">
                <h3>Enter PIN</h3>
                <p>Enter your 6-digit PIN to continue</p>
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
                        className={`pin-key ${key === "OK" ? "pin-key-ok" : key === "⌫" ? "pin-key-clear" : ""}`}
                        onClick={() => handleKey(key)}
                        disabled={loading}
                      >
                        {key === "OK" ? <>🔒 OK</> : key}
                      </button>
                    ))}
                  </div>
                ))}
              </div>

              <p className="pin-keyboard-hint">⌨ Type digits on your keyboard · Enter to confirm · Esc to go back</p>

              <button className="login-back" onClick={() => { setSelected(null); setPin(""); setError(null); }}>
                ← Back to cashier selection
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
