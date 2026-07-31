import { useEffect, useMemo, useState } from "react";
import type { ReprintQueueEntry } from "../types";
import {
  printReceiptRaw,
  reprintQueueMarkPrinted,
  reprintQueuePending,
} from "../tauri/commands";
import { clearPrintedReminder, retryPendingReprint } from "../utils/reprintQueue";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator } from "../i18n/backOfficeStrings";

interface Props {
  actorUserId: string;
}

function failedAtLabel(value: string, locale: string): string {
  return new Date(value).toLocaleString(locale, {
    timeZone: "Asia/Bahrain",
    dateStyle: "short",
    timeStyle: "short",
  });
}

export default function EodReprintQueue({ actorUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const locale = language === "ar" ? "ar-BH" : "en-BH";
  const [entries, setEntries] = useState<ReprintQueueEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [retryingId, setRetryingId] = useState<string | null>(null);
  const [printedButUnclearedIds, setPrintedButUnclearedIds] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    reprintQueuePending(actorUserId)
      .then((pending) => {
        if (!cancelled) setEntries(pending);
      })
      .catch((loadError: unknown) => {
        console.warn("Failed to load pending receipt prints for EOD:", loadError);
        if (!cancelled) {
          setError(`${t("failedLoadUnprintedReceipts")}. ${t("shiftClosingStillAvailable")}`);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [actorUserId, t]);

  const handleRetry = async (entry: ReprintQueueEntry) => {
    setRetryingId(entry.id);
    setError(null);
    if (printedButUnclearedIds.includes(entry.id)) {
      const result = await clearPrintedReminder(
        entries,
        entry.id,
        (id) => reprintQueueMarkPrinted(actorUserId, id),
      );
      setEntries(result.entries);
      setError(result.error);
      if (!result.printedButUncleared) {
        setPrintedButUnclearedIds((ids) => ids.filter((id) => id !== entry.id));
      }
      setRetryingId(null);
      return;
    }

    const result = await retryPendingReprint({
      entries,
      entry,
      print: (storeName, lines) => printReceiptRaw(actorUserId, storeName, lines),
      markPrinted: (id) => reprintQueueMarkPrinted(actorUserId, id),
    });
    setEntries(result.entries);
    setError(result.error);
    if (result.printedButUncleared) {
      setPrintedButUnclearedIds((ids) => [...ids, entry.id]);
    }
    setRetryingId(null);
  };

  if (!loading && entries.length === 0 && !error) return null;

  return (
    <section className="eod-reprint-queue" aria-labelledby="eod-reprint-title">
      <div className="eod-reprint-header">
        <div>
          <div className="zreport-title" id="eod-reprint-title">{t("unprintedReceipts")}</div>
          <div className="eod-reprint-help">
            {t("retryWhenPrinterReady")} {t("shiftClosingStillAvailable")}
          </div>
        </div>
        {entries.length > 0 && <span className="eod-reprint-count">{entries.length}</span>}
      </div>

      {loading && <div className="eod-reprint-help">{t("checkingReprintQueue")}</div>}
      {entries.map((entry) => {
        const clearOnly = printedButUnclearedIds.includes(entry.id);
        return <div className="eod-reprint-item" key={entry.id}>
          <div className="eod-reprint-details">
            <strong>{t("receipt")} {entry.receipt_number ?? t("numberUnavailable")}</strong>
            <span>{entry.business_date} · {failedAtLabel(entry.failed_at, locale)}</span>
            {entry.error && <span className="eod-reprint-reason">{entry.error}</span>}
            {entry.lines.length === 0 && (
              <span className="eod-reprint-reason">
                {t("storedReceiptUnreadable")}
              </span>
            )}
            {clearOnly && (
              <span>{t("printedReminderNeedsClear")}</span>
            )}
          </div>
          <button
            className="modal-btn-secondary eod-reprint-button"
            type="button"
            disabled={retryingId !== null || entry.lines.length === 0}
            onClick={() => void handleRetry(entry)}
          >
            {retryingId === entry.id
              ? clearOnly ? t("clearing") : t("printing")
              : entry.lines.length === 0 ? t("needsReview")
              : clearOnly ? t("clearReminder")
              : t("retryPrint")}
          </button>
        </div>
      })}
      {error && <div className="eod-reprint-error" role="status">{error}</div>}
    </section>
  );
}
