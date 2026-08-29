import { useCallback, useEffect, useMemo, useState } from "react";
import type { HubStatus, HubTruthCompareResult, SyncConflictRow } from "../../types";
import {
  hubStatus,
  hubEnable,
  hubRegenerateToken,
  hubTestConnection,
  hubConnectExisting,
  hubSetUrl,
  hubTruthCompare,
  hubTruthPull,
  syncConflictsList,
} from "../../tauri/commands";
import AppConfirmModal from "../AppConfirmModal";
import TerminalRosterPanel from "./TerminalRosterPanel";
import { useLanguage } from "../../hooks/useLanguage";
import { hubTruthStatusText, operationsTranslator } from "../../i18n/operationsStrings";

interface Props { sessionUserId: string; }

export default function HubTab({ sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);
  const [status, setStatus] = useState<HubStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);
  const [showToken, setShowToken] = useState(false);
  const [confirmRegenerate, setConfirmRegenerate] = useState(false);
  const [truth, setTruth] = useState<HubTruthCompareResult | null>(null);
  const [conflicts, setConflicts] = useState<SyncConflictRow[]>([]);
  const [truthLoading, setTruthLoading] = useState(false);

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

  const loadTruth = useCallback(async () => {
    setTruthLoading(true);
    setError(null);
    try {
      const [compare, inbox] = await Promise.all([
        hubTruthCompare(sessionUserId),
        syncConflictsList(sessionUserId).catch(() => []),
      ]);
      setTruth(compare);
      setConflicts(inbox);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setTruthLoading(false);
    }
  }, [sessionUserId]);

  const pullTruth = useCallback(async () => {
    setTruthLoading(true);
    setError(null);
    try {
      const compare = await hubTruthPull(sessionUserId);
      const inbox = await syncConflictsList(sessionUserId).catch(() => []);
      setTruth(compare);
      setConflicts(inbox);
      setMessage(t(compare.ok ? "hubTruthVerified" : "hubTruthReview"));
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setTruthLoading(false);
    }
  }, [sessionUserId, t]);

  const doAction = async (fn: () => Promise<unknown>) => {
    setActionLoading(true);
    setError(null);
    setMessage(null);
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
    setConfirmRegenerate(true);
  };

  // ─── Connect to existing hub ─────────────────────────────────────────────────
  const [connectUrl, setConnectUrl] = useState("");
  const [connectToken, setConnectToken] = useState("");

  const handleTest = async () => {
    setActionLoading(true);
    setError(null);
    setMessage(null);
    try {
      const r = await hubTestConnection(connectUrl, connectToken);
      if (!r.ok) { setError(r.error ?? t("connectionFailed")); }
      else { setError(null); setMessage(`${t("foundStore")}: ${r.store_name}`); }
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

  const truthPanel = (
    <section className="hub-status-card">
      <div className="hub-status-row">
        <span>{t("hubTruthAudit")}</span>
        <strong style={{ color: truth?.ok ? "var(--success)" : truth ? "var(--warning)" : "var(--text)" }}>
          {truth ? `${truth.score}% ${t("consistent")}` : t("notChecked")}
        </strong>
      </div>
      {truth && (
        <>
          <p className="setup-hint">{truth.message}</p>
          <div className="hub-status-row">
            <span>{t("schema")}</span>
            <strong>{truth.schema_match ? t("match") : `${t("local")} ${truth.local_schema_version} / ${t("hub")} ${truth.hub_schema_version}`}</strong>
          </div>
          {truth.tables.filter(t => t.status !== "match").length > 0 && (
            <table className="bo-table">
              <thead><tr><th>{t("table")}</th><th>{t("local")}</th><th>{t("hub")}</th><th>{t("status")}</th></tr></thead>
              <tbody>
                {truth.tables.filter(t => t.status !== "match").map(t => (
                  <tr key={t.table}>
                    <td>{t.table}</td>
                    <td>{t.local_count}</td>
                    <td>{t.hub_count}</td>
                    <td>{hubTruthStatusText(language, t.status)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <button className="btn-secondary" onClick={loadTruth} disabled={truthLoading}>
          {truthLoading ? t("checking") : t("compareTerminalHub")}
        </button>
        {truth && !truth.ok && (
          <button className="btn-primary" onClick={pullTruth} disabled={truthLoading}>
            {t("pullHubTruth")}
          </button>
        )}
      </div>
      {conflicts.length > 0 && (
        <>
          <h4>{t("conflictInbox")}</h4>
          <div className="hub-conflict-list">
            {conflicts.slice(0, 6).map(c => (
              <div key={c.conflict_id} className="hub-conflict-row">
                <strong>{c.title}</strong>
                <span>{c.table_name}{c.entity_id ? ` · ${c.entity_id}` : ""}</span>
                <small>{c.detail}</small>
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );

  if (loading) return <div className="bo-loading">{t("loading")}</div>;
  if (!status) return <div className="bo-error">{t("hubStatusLoadFailed")}</div>;

  const { mode, running, lan_ips, token, hub_url, last_error: hubError } = status;

  // ─── Hub mode ────────────────────────────────────────────────────────────────
  if (mode === "hub") {
    const masked = token
      ? token.slice(0, 8) + "…" + token.slice(-8)
      : null;
    return (
      <div className="bo-tab-content">
        {confirmRegenerate && (
          <AppConfirmModal
            title={t("regenerateStoreToken")}
            description={t("regenerateTokenDescription")}
            confirmLabel={t("regenerate")}
            danger
            onCancel={() => setConfirmRegenerate(false)}
            onConfirm={() => {
              setConfirmRegenerate(false);
              void doAction(async () => { await hubRegenerateToken(sessionUserId); });
            }}
          />
        )}
        <h3 className="bo-section-title">{t("hubServer")}</h3>
        {message && <div className="settings-action-msg settings-action-ok">{message}</div>}

        {truthPanel}

        <div className={`hub-status-card ${running ? "hub-online" : "hub-offline"}`}>
          <div className="hub-status-row">
            <span>{t("status")}:</span>
            <strong style={{ color: running ? "var(--success)" : "var(--danger)" }}>
              {running ? `${t("runningOnPort")} ${status.port}` : t("notRunning")}
            </strong>
          </div>
          {hubError && (
            <div className="hub-status-row">
              <span>{t("error")}:</span>
              <strong style={{ color: "var(--danger)" }}>{hubError}</strong>
            </div>
          )}
        </div>

        <h4>{t("lanAddresses")}</h4>
        <div className="hub-ip-list">
          {lan_ips.length === 0 && <p className="setup-hint">{t("noLanIp")}</p>}
          {lan_ips.map(ip => (
            <div key={ip} className="hub-ip-row">
              <code>{ip}:{status.port}</code>
              <button className="btn-sm" onClick={() => copy(`${ip}:${status.port}`)}>{t("copy")}</button>
            </div>
          ))}
        </div>

        <h4>{t("storeToken")}</h4>
        <div className="hub-token-row">
          <code style={{ wordBreak: "break-all" }}>
            {showToken ? token : (masked ?? t("notSet"))}
          </code>
          <button className="btn-sm" onClick={() => setShowToken(v => !v)}>
            {t(showToken ? "hide" : "reveal")}
          </button>
          {token && <button className="btn-sm" onClick={() => copy(token)}>{t("copy")}</button>}
        </div>
        <p className="setup-hint">
          {t("shareHubCredentials")}
        </p>

        <button className="btn-secondary" onClick={handleRegenerateToken} disabled={actionLoading}>
          {t("regenerateStoreToken")}
        </button>

        {/* The roster, not the socket list. What was here showed devices
            currently connected to the hub, which answered "online" for a
            terminal that had never once contacted it — every state below is
            derived from heartbeat evidence instead. Two terminal lists giving
            different answers is worse than one. */}
        <TerminalRosterPanel actorUserId={sessionUserId} canRepair />

        {error && <div className="modal-error">{error}</div>}
      </div>
    );
  }

  // ─── Terminal mode ───────────────────────────────────────────────────────────
  if (mode === "terminal") {
    return (
      <div className="bo-tab-content">
        <h3 className="bo-section-title">{t("connectedToHub")}</h3>
        {message && <div className="settings-action-msg settings-action-ok">{message}</div>}
        {truthPanel}
        <div className="hub-status-card hub-online">
          <div className="hub-status-row">
            <span>{t("hubAddress")}:</span>
            <strong><code>{hub_url}</code></strong>
          </div>
        </div>

        <h4>{t("changeHubAddress")}</h4>
        <div className="hub-form-row">
          <input
            className="field-input"
            placeholder="192.168.1.50 or http://192.168.1.50:8923"
            value={newUrl}
            onChange={e => setNewUrl(e.target.value)}
          />
          <button className="btn-primary" onClick={handleSetUrl} disabled={actionLoading || !newUrl.trim()}>
            {t("updateAddress")}
          </button>
        </div>

        <h4>{t("reenterStoreToken")}</h4>
        <div className="hub-form-row">
          <input
            className="field-input"
            type="password"
            placeholder={t("pasteStoreToken")}
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
            {t("updateAddress")}
          </button>
        </div>

        {/* A till needs to know about its siblings too — a terminal that
            stopped syncing on Tuesday is the usual reason two screens disagree. */}
        <TerminalRosterPanel actorUserId={sessionUserId} canRepair />

        {error && <div className="modal-error">{error}</div>}
      </div>
    );
  }

  // ─── Standalone mode ─────────────────────────────────────────────────────────
  return (
    <div className="bo-tab-content">
      <h3 className="bo-section-title">{t("multiTerminalHub")}</h3>
      {message && <div className="settings-action-msg settings-action-ok">{message}</div>}
      {truthPanel}
      <p className="setup-body">
        {t("multiTerminalDescription")}
      </p>

      <div className="hub-card-group">
        <div className="hub-card">
          <h4>{t("becomeHub")}</h4>
          <p>{t("becomeHubDescription")}</p>
          <label htmlFor="a11y-input-1" className="field-label">{t("port")}</label>
          <input id="a11y-input-1"
            className="field-input"
            type="number"
            value={port}
            onChange={e => setPort(e.target.value)}
            style={{ width: 120 }}
          />
          <button className="btn-primary" onClick={handleEnableHub} disabled={actionLoading}>
            {t(actionLoading ? "starting" : "becomeHub")}
          </button>
        </div>

        <div className="hub-card">
          <h4>{t("connectExistingHub")}</h4>
          <p>{t("connectExistingDescription")}</p>
          <label htmlFor="a11y-input-2" className="field-label">{t("hubAddress")}</label>
          <input id="a11y-input-2"
            className="field-input"
            placeholder="192.168.1.50"
            value={connectUrl}
            onChange={e => setConnectUrl(e.target.value)}
          />
          <label htmlFor="a11y-input-3" className="field-label">{t("storeToken")}</label>
          <input id="a11y-input-3"
            className="field-input"
            type="password"
            placeholder={t("pasteHubToken")}
            value={connectToken}
            onChange={e => setConnectToken(e.target.value)}
          />
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn-secondary" onClick={handleTest} disabled={actionLoading || !connectUrl.trim() || !connectToken.trim()}>
              {t(actionLoading ? "testing" : "testConnection")}
            </button>
            <button className="btn-primary" onClick={handleConnect} disabled={actionLoading || !connectUrl.trim() || !connectToken.trim()}>
              {t(actionLoading ? "connecting" : "connect")}
            </button>
          </div>
        </div>
      </div>

      {error && <div className="modal-error">{error}</div>}
    </div>
  );
}
