import { useCallback, useEffect, useState } from "react";
import { UserRound } from "lucide-react";
import type { CustomerRow } from "../types";
import * as cmd from "../tauri/commands";

const EMPTY_FORM = { name: "", phone: "", email: "", notes: "" };

interface Props { sessionUserId: string; }

export default function CustomersTab({ sessionUserId }: Props) {
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
      setError("Failed to load customers");
    }
  }, [sessionUserId]);

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
    if (!form.name.trim()) { setError("Name is required"); return; }
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
      setError(typeof e === "string" ? e : "Save failed");
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
            placeholder="Search by name or phone…"
            value={search}
            onChange={e => setSearch(e.target.value)}
          />
          <button className="btn-primary bo-add-btn" onClick={startCreate}>+ New</button>
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
                <span className="cust-loyalty">{c.loyalty_points} pts</span>
              </div>
            </button>
          ))}
          {customers.length === 0 && (
            <div className="bo-empty">
              <div className="bo-empty-icon"><UserRound size={40} strokeWidth={1.5} /></div>
              <p className="bo-empty-title">No customers found</p>
              <p className="bo-empty-hint">Try a different search term, or add a new customer.</p>
            </div>
          )}
        </div>
      </div>

      {/* ── Form pane ── */}
      {showingForm && (
        <div className="bo-form-pane">
          <h3 className="bo-form-title">{creating ? "New Customer" : "Edit Customer"}</h3>
          {error && <div className="bo-form-error">{error}</div>}

          <label className="bo-label">Name *</label>
          <input className="bo-input" value={form.name} onChange={e => set("name", e.target.value)} autoFocus placeholder="Full name" />

          <label className="bo-label">Phone</label>
          <input className="bo-input" value={form.phone} onChange={e => set("phone", e.target.value)} placeholder="+973 1234 5678" />

          <label className="bo-label">Email</label>
          <input className="bo-input" type="email" value={form.email} onChange={e => set("email", e.target.value)} placeholder="customer@example.com" />

          <label className="bo-label">Notes</label>
          <textarea className="bo-input" rows={3} value={form.notes} onChange={e => set("notes", e.target.value)} placeholder="Any notes about this customer" />

          {selected && (
            <div className="cust-loyalty-info">
              Loyalty points: <strong>{selected.loyalty_points}</strong>
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
