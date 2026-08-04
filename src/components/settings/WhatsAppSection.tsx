import { useCallback, useEffect, useMemo, useState } from "react";
import type { ImportContactsResult, WaContact, WaGroup, WhatsAppStatus } from "../../types";
import {
  appConfigLoad,
  whatsappDisconnect,
  whatsappGetTargets,
  whatsappImportContacts,
  whatsappListContacts,
  whatsappListGroups,
  whatsappSaveConfig,
  whatsappSetTargets,
  whatsappStatus,
} from "../../tauri/commands";
import WhatsAppQRModal from "../WhatsAppQRModal";

export default function WhatsAppSection({
  sessionUserId,
  sessionRole,
  registerTimer,
}: {
  sessionUserId: string;
  sessionRole: string;
  registerTimer: (id: ReturnType<typeof setTimeout>) => void;
}) {
  const [status, setStatus]               = useState<WhatsAppStatus>({ connected: false });
  const [showQR, setShowQR]               = useState(false);
  // One-time post-upgrade notice: Baileys v7 invalidates the old session, so a
  // previously-linked user must re-scan. Dismissible; suppressed once acknowledged.
  const [showUpgradeNotice, setShowUpgradeNotice] = useState(() => {
    try { return localStorage.getItem("wa_v7_repair_ack") !== "1"; } catch { return true; }
  });
  const [benefitNum, setBenefitNum]       = useState("");
  const [saving, setSaving]               = useState(false);
  const [saved, setSaved]                 = useState(false);
  const [saveError, setSaveError]         = useState<string | null>(null);
  const [disconnecting, setDisconnecting] = useState(false);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const [importing, setImporting]         = useState(false);
  const [importResult, setImportResult]   = useState<ImportContactsResult | null>(null);
  const [importError, setImportError]     = useState<string | null>(null);

  // ── POS alerts: owner contact + store group ──────────────────────────────────
  const [waContacts, setWaContacts] = useState<WaContact[]>([]);
  const [waGroups, setWaGroups]     = useState<WaGroup[]>([]);
  const [ownerJid, setOwnerJid]     = useState("");
  const [ownerName, setOwnerName]   = useState("");
  const [groupJid, setGroupJid]     = useState("");
  const [groupName, setGroupName]   = useState("");
  const [contactFilter, setContactFilter] = useState("");
  const [loadingChats, setLoadingChats]   = useState(false);
  const [chatsError, setChatsError]       = useState<string | null>(null);
  const [savingTargets, setSavingTargets] = useState(false);
  const [targetsSaved, setTargetsSaved]   = useState(false);

  const isManager = sessionRole === "owner" || sessionRole === "manager";

  const refresh = useCallback(async () => {
    // R-17: log polling errors instead of silently swallowing — a persistently
    // unreachable sidecar should leave a diagnostic trail.
    try { setStatus(await whatsappStatus(sessionUserId)); }
    catch (e: unknown) { console.warn("WhatsApp status poll failed:", e); }
  }, [sessionUserId]);

  useEffect(() => {
    refresh();
    appConfigLoad().then(cfg => {
      setBenefitNum(cfg.whatsapp_benefit_number ?? "");
    }).catch(() => {});
    const id = setInterval(refresh, 30_000);
    return () => clearInterval(id);
  }, [refresh]);

  // BUG-WA-PHONE-VALIDATION: validate phone number before calling the Tauri command.
  const validateBenefitNum = (val: string): string | null => {
    const v = val.trim();
    if (v === "") return null; // empty = clear setting, allowed
    if (!v.startsWith("+"))
      return "Phone number must start with '+' followed by the country code (e.g. +97333050666)";
    const afterPlus = v.slice(1);
    if (afterPlus.length === 0 || !/^\d+$/.test(afterPlus))
      return "Phone number must contain only digits after '+'";
    if (v.length < 8 || v.length > 16)
      return "Phone number must be 8–16 characters including the '+' prefix";
    return null;
  };

  const handleSave = async () => {
    setSaveError(null);
    const validationError = validateBenefitNum(benefitNum);
    if (validationError) {
      setSaveError(validationError);
      return;
    }
    setSaving(true);
    try {
      await whatsappSaveConfig(benefitNum.trim(), sessionUserId);
      setSaved(true);
      registerTimer(setTimeout(() => setSaved(false), 2000));
    } catch (e: unknown) {
      setSaveError(typeof e === "string" ? e : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleDisconnect = async () => {
    setConfirmDisconnect(false);
    setDisconnecting(true);
    try {
      await whatsappDisconnect(sessionUserId);
      await refresh();
    } catch (e: unknown) {
      setSaveError(typeof e === "string" ? e : "Failed to disconnect");
    } finally {
      setDisconnecting(false);
    }
  };

  const handleImportContacts = useCallback(async () => {
    setImporting(true);
    setImportResult(null);
    setImportError(null);
    try {
      const result = await whatsappImportContacts(sessionUserId);
      setImportResult(result);
      registerTimer(setTimeout(() => setImportResult(null), 8_000));
    } catch (e: unknown) {
      setImportError(typeof e === "string" ? e : "Failed to import contacts");
      registerTimer(setTimeout(() => setImportError(null), 6_000));
    } finally {
      setImporting(false);
    }
  }, [sessionUserId, registerTimer]);

  const handleConnected = useCallback(async () => {
    await refresh();
    // Baileys fires contacts.set (the full contact list) several seconds AFTER the
    // connection.update "open" event.  12 s gives WhatsApp time to finish the
    // initial contact-sync before we pull from the sidecar.
    const t = setTimeout(handleImportContacts, 12_000);
    registerTimer(t);
  }, [refresh, handleImportContacts, registerTimer]);

  // Load the saved owner/group + the contact & group lists once connected.
  const loadChats = useCallback(async () => {
    setLoadingChats(true);
    setChatsError(null);
    try {
      const [t, contacts, groups] = await Promise.all([
        whatsappGetTargets(sessionUserId),
        whatsappListContacts(sessionUserId),
        whatsappListGroups(sessionUserId),
      ]);
      setOwnerJid(t.owner_jid); setOwnerName(t.owner_name);
      setGroupJid(t.group_jid); setGroupName(t.group_name);
      setWaContacts(contacts.sort((a, b) => a.name.localeCompare(b.name)));
      setWaGroups(groups.sort((a, b) => a.name.localeCompare(b.name)));
    } catch (e) {
      setChatsError(typeof e === "string" ? e : "Could not load WhatsApp chats");
    } finally {
      setLoadingChats(false);
    }
  }, [sessionUserId]);

  useEffect(() => {
    if (status.connected && isManager) loadChats();
  }, [status.connected, isManager, loadChats]);

  const saveTargets = async () => {
    const manualGroup = groupJid.trim();
    if (manualGroup && !manualGroup.toLowerCase().endsWith("@g.us")) {
      setChatsError("Store group JID must end with @g.us");
      return;
    }
    setSavingTargets(true);
    setChatsError(null);
    try {
      await whatsappSetTargets(sessionUserId, {
        owner_jid: ownerJid, owner_name: ownerName,
        group_jid: manualGroup, group_name: groupName || manualGroup,
      });
      setTargetsSaved(true);
      registerTimer(setTimeout(() => setTargetsSaved(false), 2000));
    } catch (e) {
      setChatsError(typeof e === "string" ? e : "Failed to save");
    } finally {
      setSavingTargets(false);
    }
  };

  // Contacts filtered by the search box; always include the saved owner so the
  // current selection stays visible even if it's not in the freshly-synced list.
  const filteredContacts = useMemo(() => {
    const q = contactFilter.trim().toLowerCase();
    let list = q
      ? waContacts.filter(c => c.name.toLowerCase().includes(q) || c.id.includes(q))
      : waContacts;
    if (ownerJid && !waContacts.some(c => c.id === ownerJid)) {
      list = [{ id: ownerJid, name: ownerName || ownerJid }, ...list];
    }
    return list.slice(0, 300);
  }, [waContacts, contactFilter, ownerJid, ownerName]);

  const groupOptions = useMemo(() => {
    if (groupJid && !waGroups.some(g => g.id === groupJid)) {
      return [{ id: groupJid, name: groupName || groupJid }, ...waGroups];
    }
    return waGroups;
  }, [waGroups, groupJid, groupName]);

  return (
    <>
      <div className="wa-settings-status-row">
        <span className={`wa-settings-badge ${status.connected ? "wa-badge-on" : "wa-badge-off"}`}>
          {status.connected ? "● Connected" : "● Disconnected"}
        </span>
        {!status.connected && isManager && (
          <button className="btn-primary btn-sm" onClick={() => setShowQR(true)}>
            Connect (Scan QR)
          </button>
        )}
        {status.connected && isManager && (
          <>
            <button className="btn-secondary btn-sm" onClick={() => setConfirmDisconnect(true)} disabled={disconnecting}>
              {disconnecting ? "Disconnecting…" : "Disconnect"}
            </button>
            {confirmDisconnect && (
              <div className="settings-confirm-overlay">
                <div className="settings-confirm-box">
                  <div className="settings-confirm-text">Disconnect WhatsApp? You will need to scan the QR code again.</div>
                  <div className="settings-confirm-actions">
                    <button className="btn-secondary btn-sm" onClick={() => setConfirmDisconnect(false)}>Cancel</button>
                    <button className="btn-danger btn-sm" onClick={handleDisconnect}>Disconnect</button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {!status.connected && isManager && showUpgradeNotice && (
        <div className="wa-upgrade-notice" role="status">
          <span>⚠️ WhatsApp was upgraded (Baileys v7). If it was linked before, the old session can't carry over — please <strong>re-scan the QR</strong> to reconnect.</span>
          <button
            className="btn-secondary btn-sm"
            onClick={() => { setShowUpgradeNotice(false); try { localStorage.setItem("wa_v7_repair_ack", "1"); } catch { /* ignore */ } }}
          >
            Dismiss
          </button>
        </div>
      )}

      {isManager && (
        <div className="wa-import-row">
          <div className="wa-import-info">
            <span className="wa-import-label">Contact Import</span>
            <span className="wa-import-hint">
              Save all WhatsApp contacts into the POS customer list.
            </span>
          </div>
          <button className="btn-secondary btn-sm" onClick={handleImportContacts} disabled={importing}>
            {importing ? "Importing…" : "Import Contacts"}
          </button>
        </div>
      )}
      {importResult !== null && (
        <div className="wa-import-result wa-import-result-ok" role="status" aria-live="polite">
          ✅ {importResult.imported} contact{importResult.imported !== 1 ? "s" : ""} imported
          {importResult.skipped > 0 ? ` — ${importResult.skipped} already existed` : ""}
        </div>
      )}
      {importError !== null && (
        <div className="wa-import-result wa-import-result-err" role="alert">{importError}</div>
      )}
      {saveError && (
        <div className="wa-import-result wa-import-result-err" role="alert">{saveError}</div>
      )}

      <label htmlFor="a11y-wrap-WhatsAppSection" className="bo-label" style={{ marginTop: "16px" }}>BenefitPay Number</label>
      <p className="settings-hint">Sent in delivery messages so customers can pay you.</p>
      <div className="wa-benefit-row">
        <input id="a11y-wrap-WhatsAppSection"
          className="bo-input"
          placeholder="e.g. +97333050666"
          value={benefitNum}
          onChange={e => { setBenefitNum(e.target.value); setSaved(false); }}
          maxLength={20}
          disabled={!isManager}
          aria-label="BenefitPay Number"
        />
        {isManager && (
          <button className="btn-primary btn-sm" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : saved ? "✓ Saved" : "Save"}
          </button>
        )}
      </div>

      {status.connected && isManager && (
        <div className="wa-alerts-block">
          <label className="bo-label" style={{ marginTop: "20px" }}>POS Alerts — Owner & Store Group</label>
          <p className="settings-hint">
            Messages from these two chats pop up as POS notifications at the till — so you never
            miss the owner or the store group while serving customers.
          </p>
          {chatsError && (
            <div className="wa-import-result wa-import-result-err" role="alert">{chatsError}</div>
          )}
          {loadingChats ? (
            <div className="settings-hint">Loading your WhatsApp chats…</div>
          ) : (
            <>
              <div className="wa-alert-field">
                <span className="wa-alert-label">Business owner's WhatsApp</span>
                <input
                  className="bo-input"
                  placeholder="Search contacts…"
                  value={contactFilter}
                  onChange={e => setContactFilter(e.target.value)}
                  aria-label="Search owner contact"
                />
                <select
                  className="bo-select"
                  value={ownerJid}
                  onChange={e => {
                    const c = filteredContacts.find(x => x.id === e.target.value);
                    setOwnerJid(e.target.value);
                    setOwnerName(c?.name ?? "");
                  }}
                  aria-label="Owner contact"
                >
                  <option value="">— none —</option>
                  {filteredContacts.map(c => (
                    <option key={c.id} value={c.id}>{c.name}</option>
                  ))}
                </select>
              </div>

              <div className="wa-alert-field">
                <span className="wa-alert-label">Store WhatsApp group</span>
                <select
                  className="bo-select"
                  value={groupJid}
                  onChange={e => {
                    const g = groupOptions.find(x => x.id === e.target.value);
                    setGroupJid(e.target.value);
                    setGroupName(g?.name ?? "");
                  }}
                  aria-label="Store group"
                >
                  <option value="">— none —</option>
                  {groupOptions.map(g => (
                    <option key={g.id} value={g.id}>{g.name}</option>
                  ))}
                </select>
                <input
                  className="bo-input"
                  placeholder="Manual group JID, e.g. 120363...@g.us"
                  value={groupJid}
                  onChange={e => {
                    const value = e.target.value.trim();
                    const g = groupOptions.find(x => x.id === value);
                    setGroupJid(value);
                    setGroupName(g?.name ?? value);
                  }}
                  aria-label="Manual store group JID"
                />
                {waGroups.length === 0 && (
                  <div className="settings-hint">
                    No groups returned yet. Refresh chats after WhatsApp finishes syncing, or paste the group JID manually.
                  </div>
                )}
              </div>

              <div className="wa-alert-actions">
                <button className="btn-secondary btn-sm" onClick={loadChats} disabled={loadingChats}>
                  Refresh chats
                </button>
                <button className="btn-primary btn-sm" onClick={saveTargets} disabled={savingTargets}>
                  {savingTargets ? "Saving…" : targetsSaved ? "✓ Saved" : "Save"}
                </button>
              </div>
            </>
          )}
        </div>
      )}

      {showQR && (
        <WhatsAppQRModal onClose={() => setShowQR(false)} onConnected={handleConnected} sessionUserId={sessionUserId} />
      )}
    </>
  );
}
