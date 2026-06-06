import { useEffect, useState } from "react";
import type { AdminUserRow, RoleRow } from "../types";
import * as cmd from "../tauri/commands";

const EMPTY_FORM = { display_name: "", username: "", pin: "", confirm_pin: "", role_id: "", is_active: true };

interface Props { sessionUserId: string; }

export default function UsersTab({ sessionUserId }: Props) {
  const [users, setUsers]       = useState<AdminUserRow[]>([]);
  const [roles, setRoles]       = useState<RoleRow[]>([]);
  const [selected, setSelected] = useState<AdminUserRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm]         = useState(EMPTY_FORM);
  const [saving, setSaving]     = useState(false);
  const [error, setError]       = useState<string | null>(null);
  const [showPin, setShowPin]   = useState(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([cmd.adminListUsersAll(sessionUserId), cmd.adminListRoles(sessionUserId)])
      .then(([u, r]) => { if (!cancelled) { setUsers(u); setRoles(r); } });
    return () => { cancelled = true; };
  }, []);

  function startCreate() {
    setSelected(null); setCreating(true);
    setForm({ ...EMPTY_FORM, role_id: roles.find(r => r.name === "cashier")?.role_id ?? roles[0]?.role_id ?? "" });
    setError(null); setShowPin(false);
  }

  function startEdit(u: AdminUserRow) {
    setCreating(false); setSelected(u);
    setForm({ display_name: u.display_name, username: u.username, pin: "", confirm_pin: "", role_id: u.role_id, is_active: u.is_active });
    setError(null); setShowPin(false);
  }

  function cancelEdit() { setSelected(null); setCreating(false); setError(null); }
  function set(key: string, val: unknown) { setForm(f => ({ ...f, [key]: val })); }

  async function save() {
    if (!form.display_name.trim() || !form.role_id) {
      setError("Name and role are required."); return;
    }
    if (creating && !form.username.trim()) {
      setError("Username is required for new users."); return;
    }
    if (creating || form.pin) {
      if (form.pin.length < 4) { setError("PIN must be at least 4 characters."); return; }
      if (form.pin !== form.confirm_pin) { setError("PINs do not match."); return; }
    }

    setSaving(true); setError(null);
    try {
      if (creating) {
        const created = await cmd.adminCreateUser({
          display_name: form.display_name.trim(),
          username: form.username.trim(),
          pin: form.pin,
          role_id: form.role_id,
          actor_user_id: sessionUserId,
        });
        setUsers(prev => [created, ...prev]);
      } else if (selected) {
        const updated = await cmd.adminUpdateUser({
          user_id: selected.user_id,
          display_name: form.display_name.trim(),
          pin: form.pin || undefined,
          role_id: form.role_id,
          is_active: form.is_active,
          actor_user_id: sessionUserId,
        });
        setUsers(prev => prev.map(u => u.user_id === updated.user_id ? updated : u));
      }
      cancelEdit();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  const showingForm = creating || selected !== null;

  return (
    <div className="bo-tab-layout">
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <span className="bo-list-title">Staff Users</span>
          <button className="btn-primary bo-add-btn" onClick={startCreate}>+ New</button>
        </div>
        <div className="bo-list">
          {users.map(u => (
            <button
              key={u.user_id}
              className={`bo-list-row ${selected?.user_id === u.user_id ? "bo-list-row-active" : ""} ${!u.is_active ? "bo-list-row-inactive" : ""}`}
              onClick={() => startEdit(u)}
            >
              <div className="bo-list-row-main">
                <span className="bo-list-row-name">{u.display_name}</span>
                <span className="bo-list-row-sub">{u.username} · {u.role_name}</span>
              </div>
              {!u.is_active && <span className="bo-badge-inactive">Inactive</span>}
            </button>
          ))}
          {users.length === 0 && <div className="bo-empty">No users.</div>}
        </div>
      </div>

      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{creating ? "New User" : "Edit User"}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">Display Name *</label>
          <input className="bo-input" value={form.display_name} onChange={e => set("display_name", e.target.value)}
            placeholder="Full name" autoFocus />

          {creating && (
            <>
              <label className="bo-label">Username *</label>
              <input className="bo-input" value={form.username} onChange={e => set("username", e.target.value)}
                placeholder="Login username" autoComplete="off" />
            </>
          )}

          <label className="bo-label">Role *</label>
          <select className="bo-select" value={form.role_id} onChange={e => set("role_id", e.target.value)}>
            <option value="">— select —</option>
            {roles.map(r => <option key={r.role_id} value={r.role_id}>{r.name}</option>)}
          </select>

          {/* PIN section */}
          {!creating && (
            <div className="bo-change-pin-toggle">
              <button className="btn-secondary bo-pin-toggle-btn" onClick={() => setShowPin(p => !p)}>
                {showPin ? "Cancel PIN change" : "Change PIN"}
              </button>
            </div>
          )}

          {(creating || showPin) && (
            <>
              <label className="bo-label">{creating ? "PIN *" : "New PIN"} (min 4 digits)</label>
              <input className="bo-input" type="password" inputMode="numeric" maxLength={8}
                value={form.pin} onChange={e => set("pin", e.target.value)} placeholder="••••" autoComplete="new-password" />

              <label className="bo-label">Confirm PIN</label>
              <input className="bo-input" type="password" inputMode="numeric" maxLength={8}
                value={form.confirm_pin} onChange={e => set("confirm_pin", e.target.value)}
                placeholder="••••" autoComplete="new-password" />
            </>
          )}

          {!creating && (
            <div className="bo-checkboxes" style={{ marginTop: 12 }}>
              <label className="bo-checkbox-label">
                <input type="checkbox" checked={form.is_active} onChange={e => set("is_active", e.target.checked)} />
                Active
              </label>
            </div>
          )}

          <div className="bo-form-actions">
            <button className="btn-secondary" onClick={cancelEdit}>Cancel</button>
            <button className="btn-primary" onClick={save} disabled={saving}>
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
