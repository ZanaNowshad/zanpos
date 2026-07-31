import { useState, useEffect, useCallback, useRef, Fragment } from "react";
import type {
  DeliveryRow,
  DeliveryListFilter,
  SessionUser,
  ConfirmDeliveryPaymentInput,
  AdminUserRow,
  RevertPaymentInput,
} from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";
import { openUrl } from "@tauri-apps/plugin-opener";

interface Props {
  sessionUser: SessionUser;
}

type FilterPreset = "unpaid" | "paid" | "today" | "all";

// ── Helpers ───────────────────────────────────────────────────────────────────

function paymentBadge(status: string) {
  const cls: Record<string, string> = {
    unpaid: "dlv-badge dlv-badge-unpaid",
    paid: "dlv-badge dlv-badge-paid",
    cancelled: "dlv-badge dlv-badge-cancelled",
  };
  return <span className={cls[status] ?? "dlv-badge"}>{status}</span>;
}

function deliveryBadge(status: string) {
  // FIX: standardize on "dispatched" — backend uses this status string for WhatsApp trigger
  const cls: Record<string, string> = {
    pending: "dlv-badge dlv-badge-pending",
    dispatched: "dlv-badge dlv-badge-out",
    out_for_delivery: "dlv-badge dlv-badge-out",  // legacy alias — still display correctly
    delivered: "dlv-badge dlv-badge-delivered",
    cancelled: "dlv-badge dlv-badge-cancelled",
  };
  const labels: Record<string, string> = {
    pending: "Pending",
    dispatched: "Out for Delivery",
    out_for_delivery: "Out for Delivery",
    delivered: "Delivered",
    cancelled: "Cancelled",
  };
  return <span className={cls[status] ?? "dlv-badge"}>{labels[status] ?? status}</span>;
}

function methodLabel(m: string) {
  if (m === "wallet") return "BenefitPay";
  if (m === "card") return "Card";
  if (m === "cash") return "Cash";
  return m.charAt(0).toUpperCase() + m.slice(1);
}

export function deliveryStatusActionLabel(status: string) {
  return status === "dispatched" || status === "out_for_delivery"
    ? "Out for delivery"
    : "Delivered";
}

function fmtDateTime(iso: string) {
  const d = new Date(iso);
  return d.toLocaleDateString([], { day: "2-digit", month: "short", year: "numeric" }) +
    " " + d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function fmtDate(iso: string) {
  return new Date(iso).toLocaleDateString([], { day: "2-digit", month: "short", year: "numeric" });
}

function fmtTime(iso: string) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// ── Time-window guards for WhatsApp buttons ───────────────────────────────────

function isArrivalExpired(row: DeliveryRow): boolean {
  return Date.now() - new Date(row.created_at).getTime() > 60 * 60 * 1000;
}

function isReminderExpired(row: DeliveryRow): boolean {
  const offset = 3 * 60 * 60 * 1000; // Bahrain UTC+3
  const createdDay = new Date(new Date(row.created_at).getTime() + offset).toISOString().slice(0, 10);
  const todayDay = new Date(Date.now() + offset).toISOString().slice(0, 10);
  return createdDay !== todayDay;
}

// ── Main component ────────────────────────────────────────────────────────────

export default function DeliveriesTab({ sessionUser }: Props) {
  const isManager = sessionUser.role_name === "owner" || sessionUser.role_name === "manager";
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, DEVICE.currency_exponent)}`;

  // ── State ──
  const [rows, setRows] = useState<DeliveryRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preset, setPreset] = useState<FilterPreset>("unpaid");
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  // Advanced filters
  const [showFilters, setShowFilters] = useState(false);
  const [filterMethod, setFilterMethod] = useState<string>("");
  const [filterRider, setFilterRider] = useState<string>("");
  const [filterDateFrom, setFilterDateFrom] = useState<string>("");
  const [filterDateTo, setFilterDateTo] = useState<string>("");

  // Rider autocomplete
  const [riderSuggestions, setRiderSuggestions] = useState<string[]>([]);
  const [showRiderSuggestions, setShowRiderSuggestions] = useState(false);
  const riderRef = useRef<HTMLInputElement>(null);

  // User map for display names
  const [userMap, setUserMap] = useState<Map<string, string>>(new Map());

  // Confirm payment form
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  const [confirmRef, setConfirmRef] = useState("");
  const [confirmNote, setConfirmNote] = useState("");
  const [confirmLoading, setConfirmLoading] = useState(false);
  const [cancelConfirm, setCancelConfirm] = useState<DeliveryRow | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // WhatsApp contact actions (keyed by delivery_id)
  const [waLoading, setWaLoading] = useState<Record<string, "arrival" | "reminder">>({});
  const [waSent, setWaSent]       = useState<Record<string, "arrival" | "reminder">>({});

  // ── Load user map on mount (admins only) ──
  useEffect(() => {
    if (!isManager) return;
    cmd.adminListUsersAll(sessionUser.user_id).then((users: AdminUserRow[]) => {
      const map = new Map<string, string>();
      users.forEach(u => map.set(u.user_id, u.display_name));
      setUserMap(map);
    }).catch(() => { /* non-critical */ });
  }, [isManager, sessionUser.user_id]);

  // ── Load rider suggestions ──
  useEffect(() => {
    let cancelled = false;
    cmd.deliveryRiderSuggestions(DEVICE.branch_id, sessionUser.user_id)
      .then(data => { if (!cancelled) setRiderSuggestions(data); })
      .catch(() => { /* non-critical */ });
    return () => { cancelled = true; };
  }, [sessionUser.user_id]);

  // ── Build filter ──
  const buildFilter = useCallback((): DeliveryListFilter => {
    const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
    const filter: DeliveryListFilter = { branch_id: DEVICE.branch_id, limit: 200, offset: 0 };

    // Preset filters
    if (preset === "unpaid") filter.payment_status = "unpaid";
    if (preset === "paid")   filter.payment_status = "paid";
    if (preset === "today") {
      filter.date_from = `${today}T00:00:00Z`;
      filter.date_to   = `${today}T23:59:59Z`;
    }

    // Advanced filters (date range overrides today preset)
    if (filterRider.trim()) filter.staff_name = filterRider.trim();
    if (filterDateFrom) filter.date_from = `${filterDateFrom}T00:00:00Z`;
    if (filterDateTo)   filter.date_to   = `${filterDateTo}T23:59:59Z`;
    if (search.trim())  filter.contact_search = search.trim();

    // filterMethod is client-side only (no backend field), handled in load()

    return filter;
  }, [preset, search, filterRider, filterDateFrom, filterDateTo]);

  // ── Load rows ──
  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      let data = await cmd.deliveryList(buildFilter(), sessionUser.user_id);
      // Client-side method filter (backend doesn't have it)
      if (filterMethod) {
        data = data.filter(r => r.expected_payment_method === filterMethod);
      }
      setRows(data);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to load deliveries");
    } finally {
      setLoading(false);
    }
  }, [buildFilter, sessionUser.user_id, filterMethod]);

  useEffect(() => { load(); }, [load]);

  // ── Actions ──
  const handleStatusChange = async (row: DeliveryRow, newStatus: string) => {
    try {
      setActionError(null);
      const updated = await cmd.deliveryUpdateStatus({
        delivery_id: row.delivery_id,
        delivery_status: newStatus,
        actor_user_id: sessionUser.user_id,
      });
      setRows(prev => prev.map(r => r.delivery_id === updated.delivery_id ? updated : r));
    } catch (e: unknown) {
      setActionError(typeof e === "string" ? e : "Failed to update status");
    }
  };

  const handleConfirmPayment = async (deliveryId: string) => {
    setConfirmLoading(true);
    setActionError(null);
    try {
      const input: ConfirmDeliveryPaymentInput = {
        delivery_id: deliveryId,
        confirmed_by_user_id: sessionUser.user_id,
        payment_reference: confirmRef || undefined,
        payment_note: confirmNote || undefined,
      };
      const updated = await cmd.deliveryConfirmPayment(input);
      setRows(prev => prev.map(r => r.delivery_id === updated.delivery_id ? updated : r));
      setConfirmingId(null);
      setConfirmRef("");
      setConfirmNote("");
    } catch (e: unknown) {
      setActionError(typeof e === "string" ? e : "Failed to confirm payment");
    } finally {
      setConfirmLoading(false);
    }
  };

  const handleCancel = async (row: DeliveryRow) => {
    setCancelConfirm(row);
  };

  const executeCancel = async () => {
    const row = cancelConfirm;
    if (!row) return;
    setCancelConfirm(null);
    setActionError(null);
    try {
      const updated = await cmd.deliveryCancel({
        delivery_id: row.delivery_id,
        actor_user_id: sessionUser.user_id,
      });
      setRows(prev => prev.map(r => r.delivery_id === updated.delivery_id ? updated : r));
    } catch (e: unknown) {
      setActionError(typeof e === "string" ? e : "Failed to cancel delivery");
    }
  };

  // ── Mark paid → unpaid (manager/owner only) ──────────────────────────────
  const handleRevertPayment = async (row: DeliveryRow) => {
    setActionError(null);
    try {
      const input: RevertPaymentInput = {
        delivery_id: row.delivery_id,
        actor_user_id: sessionUser.user_id,
      };
      const updated = await cmd.deliveryRevertPayment(input);
      setRows(prev => prev.map(r => r.delivery_id === updated.delivery_id ? updated : r));
    } catch (e: unknown) {
      setActionError(typeof e === "string" ? e : "Failed to revert payment");
    }
  };

  // ── WhatsApp: notify customer delivery is outside + open phone dialer ──────
  const handleNotifyArrival = async (row: DeliveryRow) => {
    if (!row.contact_number) return;
    setWaLoading(p => ({ ...p, [row.delivery_id]: "arrival" }));
    try {
      // Open OS phone dialer (fire-and-forget — Tauri opener, non-blocking)
      openUrl(`tel:${row.contact_number}`).catch(() => {});
      // Send WhatsApp "delivery is here" message
      await cmd.whatsappNotifyArrival(sessionUser.user_id, row.contact_number, row.receipt_number, row.delivery_id);
      setWaSent(p => ({ ...p, [row.delivery_id]: "arrival" }));
      setTimeout(() => setWaSent(p => { const n = { ...p }; delete n[row.delivery_id]; return n; }), 4000);
    } catch {
      // non-fatal — sidecar may not be connected
    } finally {
      setWaLoading(p => { const n = { ...p }; delete n[row.delivery_id]; return n; });
    }
  };

  // ── WhatsApp: payment reminder with BenefitPay number ──────────────────────
  const handlePaymentReminder = async (row: DeliveryRow) => {
    if (!row.contact_number) return;
    setWaLoading(p => ({ ...p, [row.delivery_id]: "reminder" }));
    try {
      await cmd.whatsappPaymentReminder(
        sessionUser.user_id,
        row.contact_number,
        row.receipt_number,
        row.amount_minor,
        DEVICE.currency_exponent,
        DEVICE.currency,
        row.delivery_id,
      );
      setWaSent(p => ({ ...p, [row.delivery_id]: "reminder" }));
      setTimeout(() => setWaSent(p => { const n = { ...p }; delete n[row.delivery_id]; return n; }), 4000);
    } catch {
      // non-fatal
    } finally {
      setWaLoading(p => { const n = { ...p }; delete n[row.delivery_id]; return n; });
    }
  };

  const statusOptions = (row: DeliveryRow): string[] => {
    if (row.delivery_status === "cancelled" || row.delivery_status === "delivered") return [];
    // FIX: use "dispatched" — this is what the backend's WhatsApp trigger checks for.
    // "out_for_delivery" was never triggering the WhatsApp dispatch notification.
    if (row.delivery_status === "pending") return ["dispatched", "delivered"];
    if (row.delivery_status === "dispatched" || row.delivery_status === "out_for_delivery") return ["delivered"];
    return [];
  };

  const userName = (id?: string) =>
    id ? (userMap.get(id) ?? id.slice(0, 8) + "…") : "—";

  const hasAdvancedFilters = !!(filterMethod || filterRider || filterDateFrom || filterDateTo);

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="dlv-tab">
      <header className="dlv-command-header">
        <div>
          <span>Operations queue</span>
          <h2>Delivery queue</h2>
          <p>Handle payment exceptions and move each order to its next stage.</p>
        </div>
        <strong>{rows.length}<small>{preset === "all" ? " shown" : ` ${preset}`}</small></strong>
      </header>

      {/* ── Top bar ── */}
      <div className="dlv-topbar">
        <div className="dlv-presets">
          {([
            ["unpaid", "Unpaid"],
            ["paid", "Paid"],
            ["today", "Today"],
            ["all", "All"],
          ] as [FilterPreset, string][]).map(([p, label]) => (
            <button
              key={p}
              className={`dlv-preset-btn${preset === p ? " active" : ""}`}
              onClick={() => setPreset(p)}
            >
              {label}
            </button>
          ))}
        </div>

        <input
          className="dlv-search"
          placeholder="Search contact / name…"
          value={search}
          onChange={e => setSearch(e.target.value)}
          onKeyDown={e => e.key === "Enter" && load()}
        />

        <button
          className={`dlv-filter-toggle${hasAdvancedFilters ? " dlv-filter-toggle-active" : ""}`}
          onClick={() => setShowFilters(f => !f)}
          title="Advanced filters"
        >
          ⊞ Filters{hasAdvancedFilters ? " ●" : ""}
        </button>

        <button className="dlv-refresh-btn" onClick={load} title="Refresh">↺</button>
      </div>

      {/* ── Advanced filters panel ── */}
      {showFilters && (
        <div className="dlv-adv-filters">
          <label className="dlv-filter-label">
            Method
            <select
              className="dlv-filter-select"
              value={filterMethod}
              onChange={e => setFilterMethod(e.target.value)}
            >
              <option value="">Any</option>
              <option value="cash">Cash</option>
              <option value="card">Card</option>
              <option value="wallet">BenefitPay</option>
              <option value="other">Other</option>
            </select>
          </label>

          <label className="dlv-filter-label">
            Rider
            <div className="dlv-rider-wrap">
              <input
                ref={riderRef}
                className="dlv-filter-input"
                placeholder="Rider name…"
                value={filterRider}
                onChange={e => { setFilterRider(e.target.value); setShowRiderSuggestions(true); }}
                onFocus={() => setShowRiderSuggestions(true)}
                onBlur={() => setTimeout(() => setShowRiderSuggestions(false), 150)}
              />
              {showRiderSuggestions && riderSuggestions.filter(s =>
                filterRider ? s.toLowerCase().includes(filterRider.toLowerCase()) : true
              ).length > 0 && (
                <div className="dlv-rider-suggestions">
                  {riderSuggestions
                    .filter(s => filterRider ? s.toLowerCase().includes(filterRider.toLowerCase()) : true)
                    .map(s => (
                      <div
                        key={s}
                        className="dlv-rider-suggestion"
                        onMouseDown={() => { setFilterRider(s); setShowRiderSuggestions(false); }}
                      >
                        {s}
                      </div>
                    ))}
                </div>
              )}
            </div>
          </label>

          <label className="dlv-filter-label">
            From
            <input
              type="date"
              className="dlv-filter-input dlv-filter-date"
              value={filterDateFrom}
              onChange={e => setFilterDateFrom(e.target.value)}
            />
          </label>

          <label className="dlv-filter-label">
            To
            <input
              type="date"
              className="dlv-filter-input dlv-filter-date"
              value={filterDateTo}
              onChange={e => setFilterDateTo(e.target.value)}
            />
          </label>

          {hasAdvancedFilters && (
            <button
              className="dlv-filter-clear"
              onClick={() => {
                setFilterMethod("");
                setFilterRider("");
                setFilterDateFrom("");
                setFilterDateTo("");
              }}
            >
              ✕ Clear
            </button>
          )}
        </div>
      )}

      {/* ── Status ── */}
      {error && <div className="dlv-error" role="alert">{error}</div>}
      {actionError && <div className="dlv-error" role="alert">⚠ {actionError}<button className="dlv-error-dismiss" onClick={() => setActionError(null)}>✕</button></div>}
      {loading && <div className="dlv-loading">Loading…</div>}

      {/* ── Empty state ── */}
      {!loading && rows.length === 0 && (
        <div className="dlv-empty">
          <span aria-hidden="true">✓</span>
          <strong>No deliveries need attention</strong>
          <p>{hasAdvancedFilters || search ? "Clear filters to see the full queue." : "New delivery orders will appear here automatically."}</p>
        </div>
      )}

      {/* ── Register book table ── */}
      {rows.length > 0 && (
        <div className="dlv-table-wrap">
          <table className="dlv-table">
            <thead>
              <tr>
                <th className="dlv-th dlv-col-id">ID</th>
                <th className="dlv-th dlv-col-receipt">Receipt</th>
                <th className="dlv-th dlv-col-date">Date / Time</th>
                <th className="dlv-th dlv-col-customer">Customer</th>
                <th className="dlv-th dlv-col-contact">Contact</th>
                <th className="dlv-th dlv-col-rider">Rider</th>
                <th className="dlv-th dlv-col-amount">Amount</th>
                <th className="dlv-th dlv-col-method">Method</th>
                <th className="dlv-th dlv-col-pay">Payment</th>
                <th className="dlv-th dlv-col-dlv">Delivery</th>
                <th className="dlv-th dlv-col-actions"></th>
              </tr>
            </thead>
            <tbody>
              {rows.map(row => {
                const isExpanded = expanded === row.delivery_id;
                const opts = statusOptions(row);
                return (
                  <Fragment key={row.delivery_id}>
                    <tr
                      className={[
                        "dlv-tr",
                        isExpanded ? "dlv-tr-expanded" : "",
                        row.delivery_status === "cancelled" ? "dlv-tr-cancelled" : "",
                        row.payment_status === "paid" ? "dlv-tr-paid" : "",
                      ].filter(Boolean).join(" ")}
                      onClick={() => setExpanded(isExpanded ? null : row.delivery_id)}
                    >
                      <td className="dlv-td dlv-col-id">
                        <span className="dlv-id-chip">{row.delivery_id.slice(0, 8)}</span>
                      </td>
                      <td className="dlv-td dlv-col-receipt">
                        <span className="dlv-receipt">#{row.receipt_number}</span>
                      </td>
                      <td className="dlv-td dlv-col-date">
                        <span className="dlv-date">{fmtDate(row.created_at)}</span>
                        <span className="dlv-time">{fmtTime(row.created_at)}</span>
                      </td>
                      <td className="dlv-td dlv-col-customer">
                        {row.customer_name || <span className="dlv-muted">—</span>}
                      </td>
                      <td className="dlv-td dlv-col-contact">{row.contact_number}</td>
                      <td className="dlv-td dlv-col-rider">
                        {row.delivery_staff_name
                          ? <span className="dlv-rider">🛵 {row.delivery_staff_name}</span>
                          : <span className="dlv-muted">—</span>}
                      </td>
                      <td className="dlv-td dlv-col-amount">
                        <span className="dlv-amount">{fmt(row.amount_minor)}</span>
                      </td>
                      <td className="dlv-td dlv-col-method">
                        <span className="dlv-method">{methodLabel(row.expected_payment_method)}</span>
                      </td>
                      <td className="dlv-td dlv-col-pay">{paymentBadge(row.payment_status)}</td>
                      <td className="dlv-td dlv-col-dlv">{deliveryBadge(row.delivery_status)}</td>
                      <td className="dlv-td dlv-col-actions" onClick={e => e.stopPropagation()}>
                        <div className="dlv-quick-actions">
                          {/* Call — always visible when contact exists */}
                          {row.contact_number && (
                            <button
                              className={`dlv-quick-btn dlv-quick-call${waSent[row.delivery_id] === "arrival" ? " dlv-quick-sent" : ""}`}
                              disabled={!!waLoading[row.delivery_id] || isArrivalExpired(row)}
                              onClick={() => handleNotifyArrival(row)}
                              title={isArrivalExpired(row) ? "Only available within 1 hour of delivery bill creation" : `Call ${row.contact_number}`}
                            >
                              📞
                            </button>
                          )}
                          {/* Reminder — unpaid only */}
                          {row.contact_number && row.payment_status === "unpaid" && (
                            <button
                              className={`dlv-quick-btn dlv-quick-remind${waSent[row.delivery_id] === "reminder" ? " dlv-quick-sent" : ""}`}
                              disabled={!!waLoading[row.delivery_id] || isReminderExpired(row)}
                              onClick={() => handlePaymentReminder(row)}
                              title={isReminderExpired(row) ? "Payment reminders can only be sent on the same day as the delivery bill" : "Send payment reminder"}
                            >
                              💳
                            </button>
                          )}
                          {/* Mark Paid — unpaid + manager: expand row + show confirm form */}
                          {isManager && row.payment_status === "unpaid" && row.delivery_status !== "cancelled" && (
                            <button
                              className="dlv-quick-btn dlv-quick-pay"
                              onClick={() => {
                                setExpanded(row.delivery_id);   // ensure detail panel is visible
                                setConfirmingId(row.delivery_id);
                              }}
                              title="Mark as paid"
                            >
                              ✓
                            </button>
                          )}
                          {/* Mark Unpaid — paid + manager */}
                          {isManager && row.payment_status === "paid" && (
                            <button
                              className="dlv-quick-btn dlv-quick-unpay"
                              onClick={() => handleRevertPayment(row)}
                              title="Revert to unpaid"
                            >
                              ↺
                            </button>
                          )}
                          <span className="dlv-expand-arrow">{isExpanded ? "▲" : "▼"}</span>
                        </div>
                      </td>
                    </tr>

                    {/* Expanded detail row */}
                    {isExpanded && (
                      <tr className="dlv-tr-detail">
                        <td colSpan={11}>
                          <div className="dlv-detail" onClick={e => e.stopPropagation()}>

                            {/* Left: address + info */}
                            <div className="dlv-detail-info">
                              <div className="dlv-detail-section">
                                <span className="dlv-detail-heading">Address</span>
                                <div className="dlv-detail-address">
                                  {row.house_number && <span>{row.house_number}</span>}
                                  {row.area && <span>{row.area}</span>}
                                  {row.address_text && <span>{row.address_text}</span>}
                                  {row.delivery_note && (
                                    <span className="dlv-detail-note">📝 {row.delivery_note}</span>
                                  )}
                                </div>
                              </div>

                              {isManager && (
                                <div className="dlv-detail-section">
                                  <span className="dlv-detail-heading">Audit</span>
                                  <div className="dlv-detail-audit">
                                    <span>Created by: <b>{userName(row.created_by_user_id)}</b></span>
                                    {row.paid_confirmed_by_user_id && (
                                      <span>
                                        Confirmed by: <b>{userName(row.paid_confirmed_by_user_id)}</b>
                                        {row.paid_confirmed_at && ` · ${fmtDateTime(row.paid_confirmed_at)}`}
                                      </span>
                                    )}
                                    {row.payment_reference && (
                                      <span>Ref: <b>{row.payment_reference}</b></span>
                                    )}
                                    {row.payment_note && (
                                      <span>Note: {row.payment_note}</span>
                                    )}
                                  </div>
                                </div>
                              )}
                            </div>

                            {/* Right: actions */}
                            <div className="dlv-detail-actions">

                              {/* Status advance */}
                              {opts.length > 0 && (
                                <div className="dlv-action-group">
                                  <span className="dlv-action-label">Update status:</span>
                                  {opts.map(s => (
                                    <button
                                      key={s}
                                      className="dlv-action-btn"
                                      onClick={() => handleStatusChange(row, s)}
                                    >
                                      <span className="icon-directional" aria-hidden="true">→</span>{" "}
                                      {deliveryStatusActionLabel(s)}
                                    </button>
                                  ))}
                                </div>
                              )}

                              {/* Customer contact actions — Call & Notify + Payment Reminder */}
                              {row.contact_number && (
                                <div className="dlv-contact-actions">
                                  <button
                                    className={`dlv-action-btn dlv-action-call${waSent[row.delivery_id] === "arrival" ? " dlv-action-sent" : ""}`}
                                    disabled={!!waLoading[row.delivery_id] || isArrivalExpired(row)}
                                    onClick={() => handleNotifyArrival(row)}
                                    title={isArrivalExpired(row) ? "Only available within 1 hour of delivery bill creation" : `Call ${row.contact_number} and send WhatsApp arrival notification`}
                                  >
                                    {waLoading[row.delivery_id] === "arrival"
                                      ? "Calling…"
                                      : waSent[row.delivery_id] === "arrival"
                                        ? "✓ Called & Notified"
                                        : isArrivalExpired(row)
                                          ? "📞 Window Closed"
                                          : "📞 Call & Notify"}
                                  </button>
                                  {row.payment_status === "unpaid" && (
                                    <button
                                      className={`dlv-action-btn dlv-action-remind${waSent[row.delivery_id] === "reminder" ? " dlv-action-sent" : ""}`}
                                      disabled={!!waLoading[row.delivery_id] || isReminderExpired(row)}
                                      onClick={() => handlePaymentReminder(row)}
                                      title={isReminderExpired(row) ? "Payment reminders can only be sent on the same day as the delivery bill" : "Send WhatsApp payment reminder with BenefitPay number"}
                                    >
                                      {waLoading[row.delivery_id] === "reminder"
                                        ? "Sending…"
                                        : waSent[row.delivery_id] === "reminder"
                                          ? "✓ Reminder Sent"
                                          : isReminderExpired(row)
                                            ? "💳 Window Closed"
                                            : "💳 Payment Reminder"}
                                    </button>
                                  )}
                                </div>
                              )}

                              {/* Confirm payment */}
                              {isManager && row.payment_status === "unpaid" && row.delivery_status !== "cancelled" && (
                                confirmingId === row.delivery_id ? (
                                  <div className="dlv-confirm-form">
                                    <input
                                      className="dlv-input"
                                      placeholder="Reference # (optional)"
                                      value={confirmRef}
                                      onChange={e => setConfirmRef(e.target.value)}
                                    />
                                    <input
                                      className="dlv-input"
                                      placeholder="Note (optional)"
                                      value={confirmNote}
                                      onChange={e => setConfirmNote(e.target.value)}
                                    />
                                    <div className="dlv-confirm-btns">
                                      <button
                                        className="dlv-btn-confirm"
                                        disabled={confirmLoading}
                                        onClick={() => handleConfirmPayment(row.delivery_id)}
                                      >
                                        {confirmLoading ? "Confirming…" : "✓ Confirm Payment"}
                                      </button>
                                      <button
                                        className="dlv-btn-secondary"
                                        onClick={() => { setConfirmingId(null); setConfirmRef(""); setConfirmNote(""); }}
                                      >
                                        Cancel
                                      </button>
                                    </div>
                                  </div>
                                ) : (
                                  <button
                                    className="dlv-action-btn dlv-action-pay"
                                    onClick={() => setConfirmingId(row.delivery_id)}
                                  >
                                    ✓ Mark as Paid
                                  </button>
                                )
                              )}

                              {/* Paid info + revert */}
                              {row.payment_status === "paid" && (
                                <div className="dlv-paid-info">
                                  ✓ Paid
                                  {row.paid_confirmed_at && ` · ${fmtDateTime(row.paid_confirmed_at)}`}
                                  {row.payment_reference && ` · Ref: ${row.payment_reference}`}
                                  {isManager && (
                                    <button
                                      className="dlv-action-btn dlv-action-revert"
                                      onClick={() => handleRevertPayment(row)}
                                      title="Revert payment to unpaid"
                                    >
                                      ↺ Mark Unpaid
                                    </button>
                                  )}
                                </div>
                              )}

                              {/* Cancel */}
                              {isManager && row.payment_status !== "paid" && row.delivery_status !== "cancelled" && (
                                <button
                                  className="dlv-action-btn dlv-action-danger"
                                  onClick={() => handleCancel(row)}
                                >
                                  ✕ Cancel Delivery
                                </button>
                              )}
                            </div>
                          </div>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {/* ── Cancel confirm dialog ── */}
      {cancelConfirm && (
        <div className="settings-confirm-overlay" onClick={() => setCancelConfirm(null)}>
          <div className="settings-confirm-dialog" onClick={e => e.stopPropagation()}>
            <div className="settings-confirm-header">Cancel Delivery</div>
            <p className="settings-confirm-msg">
              Cancel delivery <strong>#{cancelConfirm.receipt_number}</strong>?<br />
              This will release any assigned rider.
            </p>
            <div className="settings-confirm-buttons">
              <button className="btn-primary" style={{ background: "var(--error, #ef4444)" }} onClick={executeCancel}>
                Yes, Cancel Delivery
              </button>
              <button className="btn-secondary" onClick={() => setCancelConfirm(null)}>Go Back</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
