import { useEffect, useState } from "react";
import { useAutoFocus } from "../hooks/useAutoFocus";
import type { AdminUserRow, RoleRow } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator, roleText } from "../i18n/operationsStrings";

const EMPTY_FORM = { display_name: "", username: "", pin: "", confirm_pin: "", role_id: "", is_active: true };

interface Props { sessionUserId: string; }

export default function UsersTab({ sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const [users, setUsers]       = useState<AdminUserRow[]>([]);
  const [roles, setRoles]       = useState<RoleRow[]>([]);
  const [selected, setSelected] = useState<AdminUserRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm]         = useState(EMPTY_FORM);
  const [saving, setSaving]     = useState(false);
  const [error, setError]       = useState<string | null>(null);
  const [showPin, setShowPin]   = useState(false);
  const nameRef = useAutoFocus<HTMLInputElement>();

  useEffect(() => {
    let cancelled = false;
    Promise.all([cmd.adminListUsersAll(sessionUserId), cmd.adminListRoles(sessionUserId)])
      .then(([u, r]) => { if (!cancelled) { setUsers(u); setRoles(r); } });
    return () => { cancelled = true; };
  }, [sessionUserId]);

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
      setError(t("nameAndRoleRequired")); return;
    }
    if (creating && !form.username.trim()) {
      setError(t("usernameRequired")); return;
    }
    if (creating || form.pin) {
      if (form.pin.length < 4) { setError(t("pinMinimum")); return; }
      if (form.pin !== form.confirm_pin) { setError(t("pinsMismatch")); return; }
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
      setError(typeof e === "string" ? e : t("saveFailed"));
    } finally {
      setSaving(false);
    }
  }

  const showingForm = creating || selected !== null;

  return (
    <div className="bo-tab-layout">
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <span className="bo-list-title">{t("staffUsers")}</span>
          <button className="btn-primary bo-add-btn" onClick={startCreate}>{t("newAction")}</button>
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
                <span className="bo-list-row-sub">{u.username} · {roleText(language, u.role_name)}</span>
              </div>
              {!u.is_active && <span className="bo-badge-inactive">{t("inactive")}</span>}
            </button>
          ))}
          {users.length === 0 && <div className="bo-empty">{t("noUsers")}</div>}
        </div>
      </div>

      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{t(creating ? "newUser" : "editUser")}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">{t("displayName")} *</label>
          <input className="bo-input" value={form.display_name} onChange={e => set("display_name", e.target.value)}
            placeholder={t("fullName")} ref={nameRef} />

          {creating && (
            <>
              <label className="bo-label">{t("username")} *</label>
              <input className="bo-input" value={form.username} onChange={e => set("username", e.target.value)}
                placeholder={t("loginUsername")} autoComplete="off" />
            </>
          )}

          <label className="bo-label">{t("role")} *</label>
          <select className="bo-select" value={form.role_id} onChange={e => set("role_id", e.target.value)}>
            <option value="">{t("select")}</option>
            {roles.map(r => <option key={r.role_id} value={r.role_id}>{roleText(language, r.name)}</option>)}
          </select>

          {/* PIN section */}
          {!creating && (
            <div className="bo-change-pin-toggle">
              <button className="btn-secondary bo-pin-toggle-btn" onClick={() => setShowPin(p => !p)}>
                {t(showPin ? "cancelPinChange" : "changePin")}
              </button>
            </div>
          )}

          {(creating || showPin) && (
            <>
              <label className="bo-label">{creating ? `${t("pin")} *` : t("newPin")} ({t("minFourDigits")})</label>
              <input className="bo-input" type="password" inputMode="numeric" maxLength={8}
                value={form.pin} onChange={e => set("pin", e.target.value)} placeholder="••••" autoComplete="new-password" />

              <label className="bo-label">{t("confirmPin")}</label>
              <input className="bo-input" type="password" inputMode="numeric" maxLength={8}
                value={form.confirm_pin} onChange={e => set("confirm_pin", e.target.value)}
                placeholder="••••" autoComplete="new-password" />
            </>
          )}

          {!creating && (
            <div className="bo-checkboxes" style={{ marginTop: 12 }}>
              <label className="bo-checkbox-label">
                <input type="checkbox" checked={form.is_active} onChange={e => set("is_active", e.target.checked)} />
                {t("active")}
              </label>
            </div>
          )}

          <div className="bo-form-actions">
            <button className="btn-secondary" onClick={cancelEdit}>{t("cancel")}</button>
            <button className="btn-primary" onClick={save} disabled={saving}>
              {t(saving ? "saving" : "save")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
