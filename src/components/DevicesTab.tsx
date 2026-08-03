import { useCallback, useEffect, useMemo, useState } from "react";
import { useAutoFocus } from "../hooks/useAutoFocus";
import type { DeviceRow } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { deviceStatusText, operationsTranslator } from "../i18n/operationsStrings";

interface Props { sessionUserId: string; }

export default function DevicesTab({ sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);
  const [devices, setDevices]   = useState<DeviceRow[]>([]);
  const [loading, setLoading]   = useState(true);
  const [error, setError]       = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [code, setCode]         = useState("");
  const [name, setName]         = useState("");
  const [saving, setSaving]     = useState(false);
  const codeRef = useAutoFocus<HTMLInputElement>();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const rows = await cmd.deviceList(sessionUserId);
      setDevices(rows);
    } catch {
      setError(t("devicesLoadFailed"));
    } finally {
      setLoading(false);
    }
  }, [sessionUserId, t]);

  useEffect(() => { load(); }, [load]);

  async function handleAdd() {
    if (!code.trim() || !name.trim()) {
      setError(t("deviceCodeNameRequired")); return;
    }
    setSaving(true); setError(null);
    try {
      const created = await cmd.deviceCreate(sessionUserId, { device_code: code.trim(), device_name: name.trim() });
      setDevices(prev => [...prev, created]);
      setShowForm(false); setCode(""); setName("");
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceCreateFailed"));
    } finally {
      setSaving(false);
    }
  }

  async function handleToggle(device: DeviceRow) {
    try {
      await cmd.deviceToggleActive(sessionUserId, device.device_id, !device.is_active);
      setDevices(prev => prev.map(d =>
        d.device_id === device.device_id ? { ...d, is_active: !d.is_active } : d
      ));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceUpdateFailed"));
    }
  }

  if (loading) return <div className="bo-empty">{t("loadingDevices")}</div>;

  return (
    <div className="devices-layout">
      <div className="devices-header">
        <h2 className="settings-title">{t("posTerminals")}</h2>
        <button className="btn-primary" onClick={() => { setShowForm(s => !s); setError(null); }}>
          {t(showForm ? "cancel" : "registerDevice")}
        </button>
      </div>

      {showForm && (
        <div className="devices-form-card">
          <h3>{t("newDevice")}</h3>
          {error && <div className="bo-form-error">{error}</div>}
          <div className="bo-row-two">
            <div>
              <label htmlFor="a11y-input-1" className="bo-label">{t("deviceCode")} *</label>
              <input id="a11y-input-1" className="bo-input" value={code} onChange={e => setCode(e.target.value)} placeholder="POS02" ref={codeRef} />
            </div>
            <div>
              <label htmlFor="a11y-input-2" className="bo-label">{t("deviceName")} *</label>
              <input id="a11y-input-2" className="bo-input" value={name} onChange={e => setName(e.target.value)} placeholder={t("counterTwo")} />
            </div>
          </div>
          <div className="bo-form-actions">
            <button className="btn-primary" onClick={handleAdd} disabled={saving}>
              {t(saving ? "saving" : "register")}
            </button>
          </div>
        </div>
      )}

      {!showForm && error && <div className="bo-form-error">{error}</div>}

      <table className="devices-table">
        <thead>
          <tr>
            <th>{t("code")}</th>
            <th>{t("name")}</th>
            <th>{t("status")}</th>
            <th>{t("thisDevice")}</th>
            <th>{t("action")}</th>
          </tr>
        </thead>
        <tbody>
          {devices.map(d => {
            const isCurrent = d.device_id === DEVICE.device_id;
            return (
              <tr key={d.device_id} className={isCurrent ? "devices-row-current" : ""}>
                <td><code>{d.device_code}</code></td>
                <td>{d.device_name}</td>
                <td>
                  <span className={`devices-badge ${d.is_active ? "devices-badge-active" : "devices-badge-inactive"}`}>
                    {deviceStatusText(language, d.is_active)}
                  </span>
                </td>
                <td>{isCurrent ? <span className="devices-current-chip">{t("thisTerminal")}</span> : ""}</td>
                <td>
                  {!isCurrent && (
                    <button
                      className={d.is_active ? "btn-secondary" : "btn-primary"}
                      onClick={() => handleToggle(d)}
                    >
                      {t(d.is_active ? "deactivate" : "activate")}
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
          {devices.length === 0 && (
            <tr><td colSpan={5} className="bo-empty">{t("noDevicesRegistered")}</td></tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
