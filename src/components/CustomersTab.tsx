import { useCallback, useEffect, useMemo, useState } from "react";
import { UserRound } from "lucide-react";
import type { CustomerRow } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { countText, operationsTranslator } from "../i18n/operationsStrings";

const EMPTY_FORM = { name: "", phone: "", email: "", notes: "" };

interface Props { sessionUserId: string; }

export default function CustomersTab({ sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);
  const [customers, setCustomers]   = useState<CustomerRow[]>([]);
  const [selected, setSelected]     = useState<CustomerRow | null>(null);
  const [creating, setCreating]     = useState(false);
  const [form, setForm]             = useState(EMPTY_FORM);
  const [saving, setSaving]         = useState(false);
  const [error, setError]           = useState<string | null>(null);
  const [search, setSearch]         = useState("");

  const load = useCallback(async (q: string) => {
    try {
      const rows = await cmd.customerList(sessionUserId, q);
      setCustomers(rows);
    } catch {
      setError(t("customersLoadFailed"));
    }
  }, [sessionUserId, t]);

  useEffect(() => { load(""); }, [load]);

  // Debounced search — avoid firing a backend query on every keystroke
  useEffect(() => {
    const t = setTimeout(() => load(search), 300);
    return () => clearTimeout(t);
  }, [search, load]);

  function startCreate() {
    setSelected(null); setCreating(true); setForm(EMPTY_FORM); setError(null);
  }

  function startEdit(c: CustomerRow) {
    setCreating(false);
    setSelected(c);
    setForm({
      name:  c.name,
      phone: c.phone  ?? "",
      email: c.email  ?? "",
      notes: c.notes  ?? "",
    });
    setError(null);
  }

  function cancelEdit() { setSelected(null); setCreating(false); setError(null); }

  function set(key: string, val: string) { setForm(f => ({ ...f, [key]: val })); }

  async function save() {
    if (!form.name.trim()) { setError(t("nameRequired")); return; }
    setSaving(true); setError(null);
    try {
      if (creating) {
        const created = await cmd.customerCreate({
          name:  form.name.trim(),
          phone: form.phone.trim()  || undefined,
          email: form.email.trim()  || undefined,
          notes: form.notes.trim()  || undefined,
          actor_user_id: sessionUserId,
        });
        setCustomers(prev => [created, ...prev]);
      } else if (selected) {
        const updated = await cmd.customerUpdate({
          customer_id: selected.customer_id,
          name:  form.name.trim(),
          phone: form.phone.trim()  || undefined,
          email: form.email.trim()  || undefined,
          notes: form.notes.trim()  || undefined,
          actor_user_id: sessionUserId,
        });
        setCustomers(prev => prev.map(c => c.customer_id === updated.customer_id ? updated : c));
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
      {/* ── List pane ── */}
      <div className="bo-list-pane">
        <div className="bo-list-header">
          <input
            className="bo-search"
            placeholder={t("searchCustomer")}
            value={search}
            onChange={e => setSearch(e.target.value)}
          />
          <button className="btn-primary bo-add-btn" onClick={startCreate}>{t("newAction")}</button>
        </div>
        <div className="bo-list">
          {customers.map(c => (
            <button
              key={c.customer_id}
              className={`bo-list-row ${selected?.customer_id === c.customer_id ? "bo-list-row-active" : ""}`}
              onClick={() => startEdit(c)}
            >
              <div className="bo-list-row-main">
                <span className="bo-list-row-name">{c.name}</span>
                <span className="bo-list-row-sub">
                  {c.phone ?? ""}
                  {c.email ? ` · ${c.email}` : ""}
                </span>
              </div>
              <div className="bo-list-row-right">
                <span className="cust-loyalty numeric-ltr">{countText(language, "points", c.loyalty_points)}</span>
              </div>
            </button>
          ))}
          {customers.length === 0 && (
            <div className="bo-empty">
              <div className="bo-empty-icon"><UserRound size={40} strokeWidth={1.5} /></div>
              <p className="bo-empty-title">{t("noCustomersFound")}</p>
              <p className="bo-empty-hint">{t("customersEmptyHint")}</p>
            </div>
          )}
        </div>
      </div>

      {/* ── Form pane ── */}
      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{t(creating ? "newCustomer" : "editCustomer")}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">{t("name")} *</label>
          <input className="bo-input" value={form.name} onChange={e => set("name", e.target.value)} autoFocus placeholder={t("fullName")} />

          <label className="bo-label">{t("phone")}</label>
          <input className="bo-input" value={form.phone} onChange={e => set("phone", e.target.value)} placeholder="+973 1234 5678" />

          <label className="bo-label">{t("email")}</label>
          <input className="bo-input" type="email" value={form.email} onChange={e => set("email", e.target.value)} placeholder="customer@example.com" />

          <label className="bo-label">{t("notes")}</label>
          <textarea className="bo-input" rows={3} value={form.notes} onChange={e => set("notes", e.target.value)} placeholder={t("customerNotesPlaceholder")} />

          {selected && (
            <div className="cust-loyalty-info">
              {t("loyaltyPoints")}: <strong className="numeric-ltr">{selected.loyalty_points}</strong>
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
