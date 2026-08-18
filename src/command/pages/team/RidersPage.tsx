import { useCallback, useEffect, useState } from "react";
import { Bike, Pencil, Trash2 } from "lucide-react";
import type { RiderRow } from "../../../types";
import { riderCreate, riderDelete, riderList, riderUpdate } from "../../../tauri/commands";
import { DataTable, EmptyState, LoadingSkeleton, PageTemplate } from "../../../components/templates";
import type { Column } from "../../../components/templates";
import ConfirmDialog from "../../../components/templates/ConfirmDialog";

interface Props {
  actorUserId: string;
}

interface Draft {
  rider_id: string | null;
  name: string;
  phone: string;
  notes: string;
  is_active: boolean;
}

const EMPTY_DRAFT: Draft = { rider_id: null, name: "", phone: "", notes: "", is_active: true };

/**
 * The delivery rider roster.
 *
 * Separate from Staff on purpose. Staff are `users`: they sign in, hold a role
 * and a PIN, and appear on the cashier-selection screen. A rider does none of
 * those things — what the shop needs of them is a name and a WhatsApp number to
 * send a drop to. Merging the two lists would mean either inventing credentials
 * nobody uses or putting riders on the login screen.
 */
export default function RidersPage({ actorUserId }: Props) {
  const [riders, setRiders] = useState<RiderRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<RiderRow | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setRiders(await riderList(actorUserId, false));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, [actorUserId]);

  useEffect(() => { void refresh(); }, [refresh]);

  const save = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      if (draft.rider_id) {
        await riderUpdate({
          rider_id: draft.rider_id,
          name: draft.name,
          phone: draft.phone,
          notes: draft.notes || undefined,
          is_active: draft.is_active,
          actor_user_id: actorUserId,
        });
      } else {
        await riderCreate({
          name: draft.name,
          phone: draft.phone,
          notes: draft.notes || undefined,
          actor_user_id: actorUserId,
        });
      }
      setDraft(null);
      await refresh();
    } catch (cause) {
      // The backend owns validation (an 8-digit Bahrain number, unique on the
      // roster); showing its message beats duplicating the rules here.
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    try {
      await riderDelete(pendingDelete.rider_id, actorUserId);
      setPendingDelete(null);
      await refresh();
    } catch (cause) {
      setError(String(cause));
      setPendingDelete(null);
    }
  };

  const columns: Column<RiderRow>[] = [
    {
      id: "name",
      header: "Rider",
      cell: rider => (
        <span>
          <span className="zp-cell-primary">{rider.name}</span>
          {rider.notes && <span className="zp-cell-sub">{rider.notes}</span>}
        </span>
      ),
    },
    {
      id: "phone",
      header: "WhatsApp",
      cell: rider => <bdi className="zp-numeric" dir="ltr">{rider.phone}</bdi>,
    },
    {
      id: "status",
      header: "Status",
      priority: 2,
      cell: rider => (
        <span className={rider.is_active ? "zp-status-ok" : "zp-status-muted"}>
          {rider.is_active ? "Active" : "Inactive"}
        </span>
      ),
    },
  ];

  return (
    <PageTemplate
      header={{
        title: "Riders",
        subtitle: "Delivery riders and the WhatsApp number their drops are sent to.",
        icon: <Bike size={18} />,
        primaryAction: { label: "+ Add rider", onClick: () => setDraft(EMPTY_DRAFT), primary: true },
      }}
      degraded={error ? { severity: "critical", message: error, onDismiss: () => setError(null) } : undefined}
      contentFlat
    >
      {loading ? (
        <LoadingSkeleton variant="table" count={4} />
      ) : riders.length === 0 ? (
        <EmptyState
          icon={<Bike size={36} strokeWidth={1.5} />}
          title="No riders yet"
          description="Add a rider to send them delivery drops on WhatsApp straight from checkout."
        />
      ) : (
        <DataTable
          caption="Delivery riders"
          rows={riders}
          columns={columns}
          rowKey={rider => rider.rider_id}
          isRowMuted={rider => !rider.is_active}
          rowAction={rider => (
            <div className="zp-row-actions">
              <button
                className="btn-secondary zp-row-action"
                onClick={() => setDraft({
                  rider_id: rider.rider_id,
                  name: rider.name,
                  phone: rider.phone,
                  notes: rider.notes ?? "",
                  is_active: rider.is_active,
                })}
                aria-label={`Edit ${rider.name}`}
              >
                <Pencil size={14} /><span className="zp-action-label">Edit</span>
              </button>
              <button
                className="btn-secondary zp-row-action"
                onClick={() => setPendingDelete(rider)}
                aria-label={`Remove ${rider.name}`}
              >
                <Trash2 size={14} /><span className="zp-action-label">Remove</span>
              </button>
            </div>
          )}
        />
      )}

      {draft && (
        <button className="modal-overlay" type="button" onClick={event => { if (event.target === event.currentTarget) setDraft(null); }}>
          <div className="modal bo-form-modal" role="dialog" aria-modal="true" aria-labelledby="rider-dialog-title">
            <div className="bo-form-modal-header">
              <h2 id="rider-dialog-title">{draft.rider_id ? "Edit rider" : "New rider"}</h2>
              <button className="bo-form-modal-close" onClick={() => setDraft(null)} aria-label="Close">✕</button>
            </div>
            <div className="bo-form-modal-body">
              <div className="bo-form-field">
                <label className="bo-label" htmlFor="rider-name">Name *</label>
                <input
                  id="rider-name"
                  className="bo-input"
                  value={draft.name}
                  onChange={event => setDraft({ ...draft, name: event.target.value })}
                />
              </div>
              <div className="bo-form-field">
                <label className="bo-label" htmlFor="rider-phone">WhatsApp number *</label>
                <input
                  id="rider-phone"
                  className="bo-input"
                  inputMode="numeric"
                  dir="ltr"
                  placeholder="33050666"
                  value={draft.phone}
                  onChange={event => setDraft({ ...draft, phone: event.target.value })}
                />
                <small className="bo-hint">8-digit Bahrain mobile. Drops are sent here.</small>
              </div>
              <div className="bo-form-field">
                <label className="bo-label" htmlFor="rider-notes">Notes</label>
                <input
                  id="rider-notes"
                  className="bo-input"
                  placeholder="Bike plate, shift, anything useful"
                  value={draft.notes}
                  onChange={event => setDraft({ ...draft, notes: event.target.value })}
                />
              </div>
              {draft.rider_id && (
                <label className="bo-checkbox-row">
                  <input
                    type="checkbox"
                    checked={draft.is_active}
                    onChange={event => setDraft({ ...draft, is_active: event.target.checked })}
                  />
                  <span>Active — an inactive rider is not offered at checkout</span>
                </label>
              )}
            </div>
            <div className="bo-form-modal-footer">
              <button className="btn-secondary" onClick={() => setDraft(null)} disabled={saving}>Cancel</button>
              <button
                className="btn-primary"
                onClick={() => void save()}
                disabled={saving || !draft.name.trim() || !draft.phone.trim()}
              >
                {saving ? "Saving…" : draft.rider_id ? "Save rider" : "Add rider"}
              </button>
            </div>
          </div>
        </button>
      )}

      <ConfirmDialog
        open={pendingDelete !== null}
        title="Remove rider"
        message={pendingDelete
          ? `${pendingDelete.name} will no longer be offered at checkout. Deliveries they already took keep their name.`
          : ""}
        confirmLabel="Remove"
        cancelLabel="Keep"
        onConfirm={() => void confirmDelete()}
        onCancel={() => setPendingDelete(null)}
      />
    </PageTemplate>
  );
}
