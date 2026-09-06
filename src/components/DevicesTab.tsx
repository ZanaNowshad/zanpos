import { useCallback, useEffect, useMemo, useState } from "react";
import { Monitor, Power, Trash2, RefreshCw } from "lucide-react";
import { useAutoFocus } from "../hooks/useAutoFocus";
import type { DeviceRow, SessionToken } from "../types";
import { DEVICE } from "../types";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { deviceStatusText, operationsTranslator } from "../i18n/operationsStrings";
import ConfirmDialog from "./templates/ConfirmDialog";
import { PageTemplate, DataTable, Drawer, EmptyState, LoadingSkeleton } from "./templates";

interface Props { sessionToken: SessionToken; }

/**
 * The last back-office list still drawing its own table.
 *
 * It had a hand-rolled `<table class="devices-table">`, its own status badges,
 * a bare text node for loading, an empty state that was a `colspan` row, and a
 * form that pushed the table down the page when it opened. None of that was
 * wrong so much as separate: it missed the column priorities, the 40px touch
 * rows and the container queries every other list gets for free, and it was
 * the one page where registering something moved the thing you were reading.
 */
export default function DevicesTab({ sessionToken }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);
  const [devices, setDevices]   = useState<DeviceRow[]>([]);
  const [loading, setLoading]   = useState(true);
  const [error, setError]       = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [code, setCode]         = useState("");
  const [name, setName]         = useState("");
  const [saving, setSaving]     = useState(false);
  const [pendingRemove, setPendingRemove] = useState<DeviceRow | null>(null);
  const [pendingRekey, setPendingRekey] = useState(false);
  const [rekeyNotice, setRekeyNotice] = useState<string | null>(null);
  const codeRef = useAutoFocus<HTMLInputElement>();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const rows = await cmd.deviceList(sessionToken);
      setDevices(rows);
    } catch {
      setError(t("devicesLoadFailed"));
    } finally {
      setLoading(false);
    }
  }, [sessionToken, t]);

  useEffect(() => { load(); }, [load]);

  function openForm() {
    setShowForm(true); setCode(""); setName(""); setError(null);
  }

  async function handleAdd() {
    if (!code.trim() || !name.trim()) {
      setError(t("deviceCodeNameRequired")); return;
    }
    setSaving(true); setError(null);
    try {
      const created = await cmd.deviceCreate(sessionToken, { device_code: code.trim(), device_name: name.trim() });
      setDevices(prev => [...prev, created]);
      setShowForm(false); setCode(""); setName("");
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceCreateFailed"));
    } finally {
      setSaving(false);
    }
  }

  /* Removal is a soft delete the backend guards: it refuses the terminal making
     the request and any device with an open shift, because that shift's cash
     still has to be counted. Those refusals surface here as the error banner. */
  async function handleRemove(device: DeviceRow) {
    setPendingRemove(null);
    try {
      await cmd.deviceDelete(sessionToken, device.device_id);
      setDevices(prev => prev.filter(d => d.device_id !== device.device_id));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceUpdateFailed"));
    }
  }

  async function handleToggle(device: DeviceRow) {
    try {
      await cmd.deviceToggleActive(sessionToken, device.device_id, !device.is_active);
      setDevices(prev => prev.map(d =>
        d.device_id === device.device_id ? { ...d, is_active: !d.is_active } : d
      ));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceUpdateFailed"));
    }
  }

  /* Re-issuing identity is the recovery for a database cloned onto a second PC
     (backup restore): both machines then share one device_id, so heartbeats
     collide and each side's data is invisible to the other. Irreversible — the
     backend rewrites this terminal's origin across its whole history. */
  async function handleRekey() {
    setPendingRekey(false);
    try {
      await cmd.deviceRekey(sessionToken);
      setRekeyNotice(t("deviceRekeyDone"));
      setError(null);
      load();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : t("deviceRekeyFailed"));
    }
  }

  return (
    <PageTemplate
      contentFlat
      header={{
        title: t("posTerminals"),
        icon: <Monitor size={18} strokeWidth={1.7} aria-hidden="true" />,
        primaryAction: { label: t("registerDevice"), onClick: openForm },
      }}
      degraded={error ? {
        severity: "warning",
        message: error,
        onDismiss: () => setError(null),
      } : undefined}
    >
      {rekeyNotice && (
        <div className="devices-rekey-notice" role="status">{rekeyNotice}</div>
      )}
      {loading ? (
        <LoadingSkeleton variant="table" count={4} />
      ) : devices.length === 0 ? (
        <EmptyState
          icon={<Monitor size={36} strokeWidth={1.5} />}
          title={t("noDevicesRegistered")}
          actions={[{ label: t("registerDevice"), onClick: openForm, primary: true }]}
        />
      ) : (
        <DataTable
          caption={t("posTerminals")}
          columns={[
            {
              id: "code",
              header: t("code"),
              width: "130px",
              cell: d => <code className="numeric-ltr">{d.device_code}</code>,
            },
            {
              id: "name",
              header: t("name"),
              /* "This terminal" was its own column, which meant a column of
                 blanks for every device but one. It rides the name instead —
                 same information, one less column on a 1024px screen. */
              cell: d => (
                <span className="zp-cell-primary">
                  {d.device_name}
                  {d.device_id === DEVICE.device_id && (
                    <span className="devices-current-chip">{t("thisTerminal")}</span>
                  )}
                </span>
              ),
            },
            {
              id: "status",
              header: t("status"),
              width: "120px",
              priority: 2,
              cell: d => (
                <span className={`zp-status ${d.is_active ? "zp-status-ok" : "zp-status-muted"}`}>
                  {deviceStatusText(language, d.is_active)}
                </span>
              ),
            },
          ]}
          rows={devices}
          rowKey={d => d.device_id}
          isRowActive={d => d.device_id === DEVICE.device_id}
          isRowMuted={d => !d.is_active}
          rowAction={d => d.device_id === DEVICE.device_id ? (
            /* The one action the terminal you are standing at may take: it is
               the only install that can re-issue its own identity, which is
               the recovery for a cloned database sharing an id with another PC. */
            <div className="devices-row-actions">
              <button
                type="button"
                className="btn-secondary zp-row-action devices-rekey-btn"
                onClick={() => { setPendingRekey(true); setRekeyNotice(null); }}
                aria-label={`${t("deviceRekeyLabel")} ${d.device_name}`}
              >
                <RefreshCw size={14} aria-hidden="true" />
                <span className="zp-action-label">{t("deviceRekeyLabel")}</span>
              </button>
            </div>
          ) : (
            /* No actions on the terminal you are standing at: the backend
               refuses to deactivate or remove it, so offering the buttons
               would only produce an error banner. */
            <div className="devices-row-actions">
              <button
                type="button"
                className={d.is_active ? "btn-secondary zp-row-action" : "btn-primary zp-row-action"}
                onClick={() => handleToggle(d)}
                aria-label={`${t(d.is_active ? "deactivate" : "activate")} ${d.device_name}`}
              >
                {/* The icon carries the meaning once the table narrows: the
                    label is dropped below a 1000px container and a bare text
                    button would collapse to nothing but padding. */}
                <Power size={14} aria-hidden="true" />
                <span className="zp-action-label">{t(d.is_active ? "deactivate" : "activate")}</span>
              </button>
              <button
                type="button"
                className="btn-secondary zp-row-action devices-remove-btn"
                onClick={() => { setPendingRemove(d); setError(null); }}
                aria-label={`${t("remove")} ${d.device_name}`}
              >
                <Trash2 size={14} aria-hidden="true" />
                <span className="zp-action-label">{t("remove")}</span>
              </button>
            </div>
          )}
        />
      )}

      <Drawer
        open={showForm}
        onOpenChange={open => { if (!open) { setShowForm(false); setError(null); } }}
        title={t("newDevice")}
        footer={
          <div className="bo-form-actions">
            <button className="btn-secondary" onClick={() => setShowForm(false)}>{t("cancel")}</button>
            <button className="btn-primary" onClick={handleAdd} disabled={saving}>
              {t(saving ? "saving" : "register")}
            </button>
          </div>
        }
      >
        <label htmlFor="a11y-input-1" className="bo-label">{t("deviceCode")} *</label>
        <input id="a11y-input-1" className="bo-input" value={code} onChange={e => setCode(e.target.value)}
          placeholder="POS02" ref={codeRef} />

        <label htmlFor="a11y-input-2" className="bo-label">{t("deviceName")} *</label>
        <input id="a11y-input-2" className="bo-input" value={name} onChange={e => setName(e.target.value)}
          placeholder={t("counterTwo")} />
      </Drawer>

      <ConfirmDialog
        open={pendingRemove !== null}
        title={t("removeDevice")}
        message={pendingRemove ? t("removeDeviceWarning").replace("{name}", pendingRemove.device_name) : ""}
        confirmLabel={t("remove")}
        cancelLabel={t("cancel")}
        onConfirm={() => { if (pendingRemove) void handleRemove(pendingRemove); }}
        onCancel={() => setPendingRemove(null)}
      />

      <ConfirmDialog
        open={pendingRekey}
        title={t("deviceRekeyLabel")}
        message={t("deviceRekeyWarning")}
        confirmLabel={t("deviceRekeyLabel")}
        cancelLabel={t("cancel")}
        onConfirm={() => { void handleRekey(); }}
        onCancel={() => setPendingRekey(false)}
      />
    </PageTemplate>
  );
}
