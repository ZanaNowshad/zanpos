import { useCallback, useEffect, useState } from "react";
import type { HubStatus } from "../../types";
import { hubStatus, hubEnable, hubRegenerateToken, hubTestConnection, hubConnectExisting, hubSetUrl } from "../../tauri/commands";

interface Props { sessionUserId: string; }

export default function HubTab({ sessionUserId }: Props) {
  const [status, setStatus] = useState<HubStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);
  const [showToken, setShowToken] = useState(false);

  const load = useCallback(async () => {
    try {
      const s = await hubStatus(sessionUserId);
      setStatus(s);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionUserId]);

  useEffect(() => { load(); }, [load]);

  const doAction = async (fn: () => Promise<unknown>) => {
    setActionLoading(true);
    setError(null);
    try { await fn(); await load(); }
    catch (e: unknown) { setError(String(e)); }
    finally { setActionLoading(false); }
  };

  // ─── Enable hub ──────────────────────────────────────────────────────────────
  const [port, setPort] = useState("8923");

  const handleEnableHub = () => doAction(async () => {
    await hubEnable(sessionUserId, parseInt(port, 10) || 8923);
  });

  const handleRegenerateToken = () => {
    if (!window.confirm("Regenerate the store token? ALL terminals will need to re-enter it.")) return;
    doAction(async () => { await hubRegenerateToken(sessionUserId); });
  };

  // ─── Connect to existing hub ─────────────────────────────────────────────────
  const [connectUrl, setConnectUrl] = useState("");
  const [connectToken, setConnectToken] = useState("");

  const handleTest = async () => {
    setActionLoading(true);
    setError(null);
    try {
      const r = await hubTestConnection(connectUrl, connectToken);
      if (!r.ok) { setError(r.error ?? "Connection failed"); }
      else { setError(null); alert(`✓ Found store: ${r.store_name}`); }
    } catch (e: unknown) { setError(String(e)); }
    finally { setActionLoading(false); }
  };

  const handleConnect = () => doAction(async () => {
    await hubConnectExisting(sessionUserId, connectUrl, connectToken);
    setConnectUrl(""); setConnectToken("");
  });

  // ─── Change hub URL ──────────────────────────────────────────────────────────
  const [newUrl, setNewUrl] = useState("");

  const handleSetUrl = () => doAction(async () => {
    await hubSetUrl(sessionUserId, newUrl);
    setNewUrl("");
  });

  // ─── Copy to clipboard helper ────────────────────────────────────────────────
  const copy = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
  };

  if (loading) return <div className="bo-loading">Loading…</div>;
  if (!status) return <div className="bo-error">Could not load hub status</div>;

  const { mode, running, lan_ips, token, hub_url, last_error: hubError, terminals } = status;

  // ─── Hub mode ────────────────────────────────────────────────────────────────
  if (mode === "hub") {
    const masked = token
      ? token.slice(0, 8) + "…" + token.slice(-8)
      : null;
    return (
      <div className="bo-tab-content">
        <h3 className="bo-section-title">Hub Server</h3>

        <div className={`hub-status-card ${running ? "hub-online" : "hub-offline"}`}>
          <div className="hub-status-row">
            <span>Status:</span>
            <strong style={{ color: running ? "var(--success)" : "var(--danger)" }}>
              {running ? "Running on port " + status.port : "Not running"}
            </strong>
          </div>
          {hubError && (
            <div className="hub-status-row">
              <span>Error:</span>
              <strong style={{ color: "var(--danger)" }}>{hubError}</strong>
            </div>
          )}
        </div>

        <h4>LAN Addresses</h4>
        <div className="hub-ip-list">
          {lan_ips.length === 0 && <p className="setup-hint">No LAN IP detected — check your network connection.</p>}
          {lan_ips.map(ip => (
            <div key={ip} className="hub-ip-row">
              <code>{ip}:{status.port}</code>
              <button className="btn-sm" onClick={() => copy(`${ip}:${status.port}`)}>Copy</button>
            </div>
          ))}
        </div>

        <h4>Store Token</h4>
        <div className="hub-token-row">
          <code style={{ wordBreak: "break-all" }}>
            {showToken ? token : (masked ?? "Not set")}
          </code>
          <button className="btn-sm" onClick={() => setShowToken(v => !v)}>
            {showToken ? "Hide" : "Reveal"}
          </button>
          {token && <button className="btn-sm" onClick={() => copy(token)}>Copy</button>}
        </div>
        <p className="setup-hint">
          Share the IP address + store token with other terminals. They'll enter it
          in Back Office → Settings → Hub → "Connect to existing Hub".
        </p>

        <button className="btn-secondary" onClick={handleRegenerateToken} disabled={actionLoading}>
          Regenerate Token
        </button>

        {terminals.length > 0 && (
          <>
            <h4>Connected Terminals</h4>
            <table className="bo-table">
              <thead><tr><th>Device ID</th><th>IP</th><th>Last Seen</th></tr></thead>
              <tbody>
                {terminals.map(t => (
                  <tr key={t.device_id}>
                    <td><code>{t.device_id}</code></td>
                    <td>{t.ip}</td>
                    <td>{new Date(t.last_seen).toLocaleString()}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}

        {error && <div className="modal-error">{error}</div>}
      </div>
    );
  }

  // ─── Terminal mode ───────────────────────────────────────────────────────────
  if (mode === "terminal") {
    return (
      <div className="bo-tab-content">
        <h3 className="bo-section-title">Connected to Hub</h3>
        <div className="hub-status-card hub-online">
          <div className="hub-status-row">
            <span>Hub Address:</span>
            <strong><code>{hub_url}</code></strong>
          </div>
        </div>

        <h4>Change Hub Address</h4>
        <div className="hub-form-row">
          <input
            className="field-input"
            placeholder="192.168.1.50 or http://192.168.1.50:8923"
            value={newUrl}
            onChange={e => setNewUrl(e.target.value)}
          />
          <button className="btn-primary" onClick={handleSetUrl} disabled={actionLoading || !newUrl.trim()}>
            Update
          </button>
        </div>

        <h4>Re-enter Store Token</h4>
        <div className="hub-form-row">
          <input
            className="field-input"
            type="password"
            placeholder="Paste the store token"
            value={connectToken}
            onChange={e => setConnectToken(e.target.value)}
          />
          <button
            className="btn-primary"
            onClick={() => doAction(async () => {
              await hubConnectExisting(sessionUserId, hub_url ?? "", connectToken);
            })}
            disabled={actionLoading || !connectToken.trim()}
          >
            Update Token
          </button>
        </div>

        {error && <div className="modal-error">{error}</div>}
      </div>
    );
  }

  // ─── Standalone mode ─────────────────────────────────────────────────────────
  return (
    <div className="bo-tab-content">
      <h3 className="bo-section-title">Multi-Terminal Hub</h3>
      <p className="setup-body">
        Connect this device to other tills over your shop WiFi for real-time stock
        and sales sync — no internet required.
      </p>

      <div className="hub-card-group">
        <div className="hub-card">
          <h4>Become Hub</h4>
          <p>Make THIS device the store hub. Other tills connect to it.</p>
          <label className="field-label">Port</label>
          <input
            className="field-input"
            type="number"
            value={port}
            onChange={e => setPort(e.target.value)}
            style={{ width: 120 }}
          />
          <button className="btn-primary" onClick={handleEnableHub} disabled={actionLoading}>
            {actionLoading ? "Starting…" : "Become Hub"}
          </button>
        </div>

        <div className="hub-card">
          <h4>Connect to Existing Hub</h4>
          <p>This device becomes a terminal of another ZANPOS machine on the same WiFi.</p>
          <label className="field-label">Hub Address</label>
          <input
            className="field-input"
            placeholder="192.168.1.50"
            value={connectUrl}
            onChange={e => setConnectUrl(e.target.value)}
          />
          <label className="field-label">Store Token</label>
          <input
            className="field-input"
            type="password"
            placeholder="Paste the token from the hub"
            value={connectToken}
            onChange={e => setConnectToken(e.target.value)}
          />
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn-secondary" onClick={handleTest} disabled={actionLoading || !connectUrl.trim() || !connectToken.trim()}>
              Test
            </button>
            <button className="btn-primary" onClick={handleConnect} disabled={actionLoading || !connectUrl.trim() || !connectToken.trim()}>
              Connect
            </button>
          </div>
        </div>
      </div>

      {error && <div className="modal-error">{error}</div>}
    </div>
  );
}
