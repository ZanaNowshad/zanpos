import { useEffect, useMemo, useRef, useState } from "react";
import type { SyncQueueItem } from "../types";
import { syncQueueList, syncQueueRetry, syncQueueDismiss } from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator, ownedModalLabel } from "../i18n/modalStrings";
import { detailTranslator } from "../i18n/detailStrings";

interface Props {
  onClose: () => void;
  sessionUserId: string;
}

const STATUS_CLASS: Record<string, string> = {
  pending:  "sq-badge-pending",
  failed:   "sq-badge-failed",
  conflict: "sq-badge-conflict",
};

export default function SyncQueueModal({ onClose, sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dt = useMemo(() => detailTranslator(language), [language]);
  const [items, setItems] = useState<SyncQueueItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const cancelledRef = useRef(false);

  const load = () => {
    setLoading(true);
    cancelledRef.current = false;
    syncQueueList(sessionUserId)
      .then(data => { if (!cancelledRef.current) setItems(data); })
      .catch(() => { if (!cancelledRef.current) setError(dt("failedLoadSyncQueue")); })
      .finally(() => { if (!cancelledRef.current) setLoading(false); });
  };

  useEffect(load, [dt, sessionUserId]);

  const handleRetry = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await syncQueueRetry(sessionUserId, id);
      load();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : dt("retryFailed"));
    } finally {
      setBusyId(null);
    }
  };

  const handleDismiss = async (id: string) => {
    setBusyId(id);
    setError(null);
    try {
      await syncQueueDismiss(sessionUserId, id);
      setItems(prev => prev.filter(i => i.sync_event_id !== id));
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : dt("failedDismiss"));
    } finally {
      setBusyId(null);
    }
  };

  const retryAll = async () => {
    const retryable = items.filter(i => i.status === "failed" || i.status === "conflict");
    for (const item of retryable) {
      try { await syncQueueRetry(sessionUserId, item.sync_event_id); } catch { /* continue */ }
    }
    load();
  };

  return (
    <div className="modal-overlay">
      <div className="modal sync-queue-modal">
        <h2 className="modal-title">{t("syncQueue")}</h2>
        <p className="modal-subtitle">
          {t("eventsWaitingToSync")}
        </p>

        {loading ? (
          <div className="sq-loading">{t("loading")}</div>
        ) : items.length === 0 ? (
          <div className="sq-empty">
            <div className="sq-empty-icon">✓</div>
            <div className="sq-empty-msg">{t("allEventsSynced")}</div>
          </div>
        ) : (
          <>
            <div className="sq-toolbar">
              <span className="sq-count">{items.length} {t("items")}</span>
              {items.some(i => i.status === "failed" || i.status === "conflict") && (
                <button className="sq-retry-all-btn" onClick={retryAll}>
                  {t("retryAllFailed")}
                </button>
              )}
            </div>
            <div className="sq-list">
              {items.map(item => (
                <div key={item.sync_event_id} className="sq-item">
                  <div className="sq-item-header">
                    <span className={`sq-badge ${STATUS_CLASS[item.status] ?? "sq-badge-pending"}`}>
                      {ownedModalLabel(language, item.status)}
                    </span>
                    <span className="sq-entity">{item.entity_type} / {item.operation}</span>
                    <span className="sq-attempts">{t("attempts")}: {item.attempt_count}</span>
                  </div>
                  <div className="sq-item-id">{item.entity_id}</div>
                  {item.last_error && (
                    <div className="sq-error-msg">{item.last_error}</div>
                  )}
                  <div className="sq-item-footer">
                    <span className="sq-date">
                      {new Date(item.created_at).toLocaleString([], {
                        month: "short", day: "numeric",
                        hour: "2-digit", minute: "2-digit",
                      })}
                    </span>
                    <div className="sq-item-actions">
                      {(item.status === "failed" || item.status === "conflict") && (
                        <button
                          className="sq-btn sq-btn-retry"
                          onClick={() => handleRetry(item.sync_event_id)}
                          disabled={busyId === item.sync_event_id}
                        >
                          {t("retry")}
                        </button>
                      )}
                      <button
                        className="sq-btn sq-btn-dismiss"
                        onClick={() => handleDismiss(item.sync_event_id)}
                        disabled={busyId === item.sync_event_id}
                      >
                        {t("dismiss")}
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        {error && <div className="modal-error">{error}</div>}

        <div className="modal-actions">
          <button className="modal-btn-secondary" onClick={onClose}>{t("close")}</button>
          <button className="modal-btn-secondary" onClick={load} disabled={loading}>
            ↻ {t("refresh")}
          </button>
        </div>
      </div>
    </div>
  );
}
