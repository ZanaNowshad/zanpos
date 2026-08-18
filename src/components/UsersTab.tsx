import { useEffect, useState } from "react";
import { useAutoFocus } from "../hooks/useAutoFocus";
import type { AdminUserRow, RoleRow } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { operationsTranslator, roleText } from "../i18n/operationsStrings";
import { PageTemplate, EmptyState, LoadingSkeleton } from "./templates";
import { Users } from "lucide-react";

const EMPTY_FORM = { display_name: "", username: "", pin: "", confirm_pin: "", role_id: "", is_active: true };

interface Props {
  sessionUserId: string;
  /** The actor's own role. Only an owner may grant the owner role — the
   *  backend enforces it; this keeps the form from offering a choice that
   *  would be refused on save. */
  sessionRole: string;
}

export default function UsersTab({ sessionUserId, sessionRole }: Props) {
  const { language } = useLanguage();
  const t = operationsTranslator(language);
  const [users, setUsers]       = useState<AdminUserRow[]>([]);
  const [roles, setRoles]       = useState<RoleRow[]>([]);
  const [loading, setLoading]   = useState(true);
  const [selected, setSelected] = useState<AdminUserRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm]         = useState(EMPTY_FORM);
  const [saving, setSaving]     = useState(false);
  const [error, setError]       = useState<string | null>(null);
  const [showPin, setShowPin]   = useState(false);
  const nameRef = useAutoFocus<HTMLInputElement>();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    Promise.all([cmd.adminListUsersAll(sessionUserId), cmd.adminListRoles(sessionUserId)])
      .then(([u, r]) => { if (!cancelled) { setUsers(u); setRoles(r); setLoading(false); } });
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
  function setField(key: string, val: unknown) { setForm(f => ({ ...f, [key]: val })); }

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
    <PageTemplate
      header={{
        title: t("staffUsers"),
        icon: <Users size={18} strokeWidth={1.7} aria-hidden="true" />,
        primaryAction: { label: t("newAction"), onClick: startCreate },
      }}
    >
      {loading ? (
        <LoadingSkeleton variant="table" count={5} />
      ) : users.length === 0 ? (
        <EmptyState
          icon={<Users size={36} strokeWidth={1.5} />}
          title={t("noUsers")}
          description="Add staff members to grant them system access."
          actions={[{ label: t("newAction"), onClick: startCreate, primary: true }]}
        />
      ) : (
        <div className="bo-tab-layout">
          <div className="bo-list-pane">
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
            </div>
          </div>

          {showingForm && (
            <div className="bo-form-pane">
              <h3 className="bo-form-title">{t(creating ? "newUser" : "editUser")}</h3>
              {error && <div className="bo-form-error">{error}</div>}

              <label htmlFor="a11y-input-1" className="bo-label">{t("displayName")} *</label>
              <input id="a11y-input-1" className="bo-input" value={form.display_name} onChange={e => setField("display_name", e.target.value)}
                placeholder={t("fullName")} ref={nameRef} />

              {creating && (
                <>
                  <label htmlFor="a11y-input-2" className="bo-label">{t("username")} *</label>
                  <input id="a11y-input-2" className="bo-input" value={form.username} onChange={e => setField("username", e.target.value)}
                    placeholder={t("loginUsername")} autoComplete="off" />
                </>
              )}

              <label htmlFor="a11y-input-3" className="bo-label">{t("role")} *</label>
              <select id="a11y-input-3" className="bo-select" value={form.role_id} onChange={e => setField("role_id", e.target.value)}>
                <option value="">{t("select")}</option>
                {roles
                  // `authorize_user_admin` refuses this for a manager, so the
                  // option is not offered rather than offered and rejected.
                  .filter(r => r.name !== "owner" || sessionRole === "owner")
                  .map(r => <option key={r.role_id} value={r.role_id}>{roleText(language, r.name)}</option>)}
              </select>

              {!creating && (
                <div className="bo-change-pin-toggle">
                  <button className="btn-secondary bo-pin-toggle-btn" onClick={() => setShowPin(p => !p)}>
                    {t(showPin ? "cancelPinChange" : "changePin")}
                  </button>
                </div>
              )}

              {(creating || showPin) && (
                <>
                  <label htmlFor="a11y-input-4" className="bo-label">{creating ? `${t("pin")} *` : t("newPin")} ({t("minFourDigits")})</label>
                  <input id="a11y-input-4" className="bo-input" type="password" inputMode="numeric" maxLength={8}
                    value={form.pin} onChange={e => setField("pin", e.target.value)} placeholder="••••" autoComplete="new-password" />

                  <label htmlFor="a11y-input-5" className="bo-label">{t("confirmPin")}</label>
                  <input id="a11y-input-5" className="bo-input" type="password" inputMode="numeric" maxLength={8}
                    value={form.confirm_pin} onChange={e => setField("confirm_pin", e.target.value)}
                    placeholder="••••" autoComplete="new-password" />
                </>
              )}

              {!creating && (
                <div className="bo-checkboxes" style={{ marginTop: 12 }}>
                  <label htmlFor="a11y-input-6" className="bo-checkbox-label">
                    <input id="a11y-input-6" type="checkbox" checked={form.is_active} onChange={e => setField("is_active", e.target.checked)} />
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
      )}
    </PageTemplate>
  );
}
