import { useEffect, useState } from "react";
import type { DeviceRow } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";

interface Props { sessionUserId: string; }

export default function DevicesTab({ sessionUserId }: Props) {
  const [devices, setDevices]   = useState<DeviceRow[]>([]);
  const [loading, setLoading]   = useState(true);
  const [error, setError]       = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [code, setCode]         = useState("");
  const [name, setName]         = useState("");
  const [saving, setSaving]     = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const rows = await cmd.deviceList();
      setDevices(rows);
    } catch {
      setError("Failed to load devices");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, []); // load is stable — no deps needed

  async function handleAdd() {
    if (!code.trim() || !name.trim()) {
      setError("Device code and name are required"); return;
    }
    setSaving(true); setError(null);
    try {
      const created = await cmd.deviceCreate(sessionUserId, { device_code: code.trim(), device_name: name.trim() });
      setDevices(prev => [...prev, created]);
      setShowForm(false); setCode(""); setName("");
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to create device");
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
      setError(typeof e === "string" ? e : "Failed to update device");
    }
  }

  if (loading) return <div className="bo-empty">Loading devices…</div>;

  return (
    <div className="devices-layout">
      <div className="devices-header">
        <h2 className="settings-title">POS Terminals</h2>
        <button className="btn-primary" onClick={() => { setShowForm(s => !s); setError(null); }}>
          {showForm ? "Cancel" : "+ Register Device"}
        </button>
      </div>

      {showForm && (
        <div className="devices-form-card">
          <h3>New Device</h3>
          {error && <div className="bo-form-error">{error}</div>}
          <div className="bo-row-two">
            <div>
              <label className="bo-label">Device Code *</label>
              <input className="bo-input" value={code} onChange={e => setCode(e.target.value)} placeholder="POS02" autoFocus />
            </div>
            <div>
              <label className="bo-label">Device Name *</label>
              <input className="bo-input" value={name} onChange={e => setName(e.target.value)} placeholder="Counter 2" />
            </div>
          </div>
          <div className="bo-form-actions">
            <button className="btn-primary" onClick={handleAdd} disabled={saving}>
              {saving ? "Saving…" : "Register"}
            </button>
          </div>
        </div>
      )}

      {!showForm && error && <div className="bo-form-error">{error}</div>}

      <table className="devices-table">
        <thead>
          <tr>
            <th>Code</th>
            <th>Name</th>
            <th>Status</th>
            <th>This Device</th>
            <th>Action</th>
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
                    {d.is_active ? "Active" : "Inactive"}
                  </span>
                </td>
                <td>{isCurrent ? <span className="devices-current-chip">This terminal</span> : ""}</td>
                <td>
                  {!isCurrent && (
                    <button
                      className={d.is_active ? "btn-secondary" : "btn-primary"}
                      onClick={() => handleToggle(d)}
                    >
                      {d.is_active ? "Deactivate" : "Activate"}
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
          {devices.length === 0 && (
            <tr><td colSpan={5} className="bo-empty">No devices registered.</td></tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
