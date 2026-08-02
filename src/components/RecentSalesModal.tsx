import { useEffect, useMemo, useState } from "react";
import type { SaleListRow } from "../types";
import { DEVICE } from "../types";
import { formatMoney } from "../money";
import * as cmd from "../tauri/commands";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator, ownedModalLabel } from "../i18n/modalStrings";

interface Props {
  onReprint: (receiptNumber: string) => Promise<void>;
  onEdit: (sale: SaleListRow) => Promise<void>;
  onClose: () => void;
  sessionUserId: string;
}

function todayStr() {
  return new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

export default function RecentSalesModal({ onReprint, onEdit, onClose, sessionUserId }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const [date, setDate]         = useState(todayStr());
  const [sales, setSales]       = useState<SaleListRow[]>([]);
  const [loading, setLoading]   = useState(false);
  const [error, setError]       = useState<string | null>(null);
  const [selected, setSelected] = useState<SaleListRow | null>(null);
  const [working, setWorking]   = useState(false);

  const EXP = DEVICE.currency_exponent;
  const fmt = (n: number) => `${DEVICE.currency} ${formatMoney(n, EXP)}`;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setSales([]);
    setSelected(null);
    cmd.reportSalesList(sessionUserId, DEVICE.branch_id, date, date)
      .then(page => { if (!cancelled) setSales(page.items); })
      .catch((e: unknown) => {
        if (!cancelled) setError(typeof e === "string" ? e : t("failed"));
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [date, sessionUserId, t]);

  async function handleReprint() {
    if (!selected || working) return;
    setWorking(true);
    try { await onReprint(selected.receipt_number); }
    catch (e: unknown) { setError(typeof e === "string" ? e : t("failed")); }
    finally { setWorking(false); }
  }

  async function handleEdit() {
    if (!selected || working) return;
    setWorking(true);
    try { await onEdit(selected); }
    catch (e: unknown) { setError(typeof e === "string" ? e : t("failed")); }
    finally { setWorking(false); }
  }

  return (
    <button className="modal-overlay" type="button" onClick={onClose}>
      <div className="modal recent-sales-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <span className="modal-title">{t("recentSales")}</span>
          <input
            className="recent-date-input"
            type="date"
            value={date}
            onChange={e => setDate(e.target.value)}
          />
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {error && <div className="modal-error">{error}</div>}

        <div className="recent-sales-list">
          {loading && <div className="recent-loading">{t("loading")}</div>}
          {!loading && sales.length === 0 && (
            <div className="recent-empty">{t("noSalesFor")} {date}</div>
          )}
          {sales.map(s => (
            <button
              key={s.sale_id}
              className={`recent-sale-row ${s.status === "voided" ? "recent-sale-voided" : ""} ${selected?.sale_id === s.sale_id ? "recent-sale-selected" : ""}`}
              onClick={() => setSelected(selected?.sale_id === s.sale_id ? null : s)}
            >
              <span className="recent-sale-receipt">{s.receipt_number}</span>
              <span className="recent-sale-time">
                {new Date(s.sold_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </span>
              <span className="recent-sale-cashier">{s.cashier_name}</span>
              <span className="recent-sale-methods">{s.payment_methods}</span>
              <span className="recent-sale-total">{fmt(s.net_total_minor)}</span>
              <span className={`recent-sale-status recent-sale-status-${s.status}`}>{ownedModalLabel(language, s.status)}</span>
            </button>
          ))}
        </div>

        {selected && (
          <div className="recent-sale-actions">
            <span className="recent-selected-label">#{selected.receipt_number} · {fmt(selected.net_total_minor)}</span>
            <button
              className="btn-secondary"
              onClick={handleReprint}
              disabled={working}
            >
              🖨 {t("reprint")}
            </button>
            {selected.status !== "voided" && (
              <button
                className="btn-primary"
                onClick={handleEdit}
                disabled={working}
                title={t("edit")}
              >
                {working ? t("loading") : `✏ ${t("edit")}`}
              </button>
            )}
          </div>
        )}

        <div className="modal-actions">
          <button className="btn-secondary" onClick={onClose}>{t("close")}</button>
        </div>
      </div>
    </div>
  );
}
