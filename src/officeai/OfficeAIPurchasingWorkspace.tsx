import { AlertTriangle, PackagePlus, Plus, Users } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  appConfigLoad,
  poCancel,
  poCreate,
  poGet,
  poList,
  poReceive,
  reportMargin,
  reportProductMargin,
  supplierDelete,
  supplierList,
  supplierUpsert,
} from "../tauri/commands";
import type {
  MarginSummary,
  ProductMarginRow,
  PurchaseOrderDetail,
  PurchaseOrderRow,
  SupplierRow,
} from "../types";
import AppConfirmModal from "../components/AppConfirmModal";
import { useLanguage } from "../hooks/useLanguage";
import {
  countText,
  operationsTranslator,
  poStatusText,
} from "../i18n/operationsStrings";
import PurchasingCommandStrip from "./PurchasingCommandStrip";
import PurchasingMetrics from "./PurchasingMetrics";
import {
  buildPurchasingCommandModel,
  defaultPurchasingRange,
  downloadPurchasingCsv,
  purchasingMoney,
} from "./purchasingPresentation";

export { buildPurchasingCommandModel } from "./purchasingPresentation";

interface Props {
  actorUserId: string;
  currencyExp: number;
  onSendPrompt: (prompt: string) => void;
}

export default function OfficeAIPurchasingWorkspace({ actorUserId, currencyExp, onSendPrompt }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);
  const [suppliers, setSuppliers] = useState<SupplierRow[]>([]);
  const [orders, setOrders] = useState<PurchaseOrderRow[]>([]);
  const [margin, setMargin] = useState<MarginSummary | null>(null);
  const [productMargins, setProductMargins] = useState<ProductMarginRow[]>([]);
  const [supplierSearch, setSupplierSearch] = useState("");
  const [poStatusFilter, setPoStatusFilter] = useState("open");
  const [marginSearch, setMarginSearch] = useState("");
  const [editingSupplier, setEditingSupplier] = useState<SupplierRow | null>(null);
  const [supplierName, setSupplierName] = useState("");
  const [supplierPhone, setSupplierPhone] = useState("");
  const [supplierEmail, setSupplierEmail] = useState("");
  const [supplierContact, setSupplierContact] = useState("");
  const [supplierNotes, setSupplierNotes] = useState("");
  const [poSupplierId, setPoSupplierId] = useState("");
  const [poProductName, setPoProductName] = useState("");
  const [poQty, setPoQty] = useState("1");
  const [poCost, setPoCost] = useState("0");
  const [poDetail, setPoDetail] = useState<PurchaseOrderDetail | null>(null);
  const [poDetailLoading, setPoDetailLoading] = useState(false);
  const [receiveQty, setReceiveQty] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [confirm, setConfirm] = useState<{
    title: string;
    description: string;
    confirmLabel: string;
    danger?: boolean;
    run: () => Promise<void>;
  } | null>(null);
  const range = useMemo(() => defaultPurchasingRange(), []);
  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const config = await appConfigLoad();
      const [nextSuppliers, nextOrders, nextMargin, nextProductMargins] = await Promise.all([
        supplierList(actorUserId),
        poList(actorUserId),
        reportMargin(actorUserId, config.branch_id, range.from, range.to).catch(() => null),
        reportProductMargin(actorUserId, config.branch_id, range.from, range.to, 8).catch(() => []),
      ]);
      setSuppliers(nextSuppliers);
      setOrders(nextOrders);
      setMargin(nextMargin);
      setProductMargins(nextProductMargins);
    } catch (e) {
      setError(typeof e === "string" ? e : t("purchasingLoadFailed"));
    } finally {
      setLoading(false);
    }
  }, [actorUserId, range.from, range.to, t]);

  useEffect(() => { void load(); }, [load]);

  const model = buildPurchasingCommandModel(suppliers, orders, margin);
  const filteredSuppliers = suppliers
    .filter(s => {
      const q = supplierSearch.trim().toLowerCase();
      return !q || [s.name, s.phone, s.email, s.contact_name]
        .filter(Boolean)
        .some(v => String(v).toLowerCase().includes(q));
    })
    .slice(0, 50);
  const filteredOrders = orders
    .filter(po => {
      if (poStatusFilter === "open") return ["draft", "ordered", "partial"].includes(po.status);
      if (poStatusFilter === "all") return true;
      return po.status === poStatusFilter;
    })
    .slice(0, 50);
  const filteredMargins = productMargins
    .filter(row => {
      const q = marginSearch.trim().toLowerCase();
      return !q || row.product_name.toLowerCase().includes(q);
    })
    .slice(0, 50);
  const startEditSupplier = (supplier: SupplierRow) => {
    setEditingSupplier(supplier);
    setSupplierName(supplier.name);
    setSupplierPhone(supplier.phone ?? "");
    setSupplierEmail(supplier.email ?? "");
    setSupplierContact(supplier.contact_name ?? "");
    setSupplierNotes(supplier.notes ?? "");
  };
  const resetSupplierForm = () => {
    setEditingSupplier(null);
    setSupplierName("");
    setSupplierPhone("");
    setSupplierEmail("");
    setSupplierContact("");
    setSupplierNotes("");
  };
  const saveSupplier = async () => {
    const name = supplierName.trim();
    if (!name) return;
    setError(null);
    setMessage(null);
    try {
      await supplierUpsert(actorUserId, {
        supplier_id: editingSupplier?.supplier_id,
        name,
        phone: supplierPhone.trim() || null,
        email: supplierEmail.trim() || null,
        contact_name: supplierContact.trim() || null,
        notes: supplierNotes.trim() || null,
        is_active: editingSupplier?.is_active ?? true,
      });
      resetSupplierForm();
      setMessage(t(editingSupplier ? "supplierUpdated" : "supplierSaved"));
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : t("supplierSaveFailed"));
    }
  };

  const deactivateSupplier = (supplierId: string) => {
    setConfirm({
      title: t("deactivateSupplier"),
      description: t("deactivateSupplierDescription"),
      confirmLabel: t("deactivate"),
      danger: true,
      run: async () => {
        setError(null);
        setMessage(null);
        try {
          await supplierDelete(actorUserId, supplierId);
          setMessage(t("supplierDeactivated"));
          await load();
        } catch (e) {
          setError(typeof e === "string" ? e : t("supplierDeactivateFailed"));
        }
      },
    });
  };

  const createPo = async () => {
    if (!poProductName.trim()) {
      setError(t("draftPoProductRequired"));
      return;
    }
    const unitCost = Math.round(Number(poCost || "0") * Math.pow(10, currencyExp));
    setError(null);
    setMessage(null);
    try {
      await poCreate(actorUserId, {
        supplier_id: poSupplierId || null,
        created_by: actorUserId,
        lines: [{
          product_name: poProductName.trim(),
          ordered_qty: poQty,
          unit_cost_minor: unitCost,
        }],
      });
      setPoProductName("");
      setPoQty("1");
      setPoCost("0");
      setMessage(t("draftPoCreated"));
      await load();
    } catch (e) {
      setError(typeof e === "string" ? e : t("poCreateFailed"));
    }
  };

  const openPoDetail = async (poId: string) => {
    setPoDetailLoading(true);
    setError(null);
    try {
      const detail = await poGet(actorUserId, poId);
      setPoDetail(detail);
      setReceiveQty(Object.fromEntries(
        detail.lines.map(line => [
          line.po_line_id,
          String(Math.max(0, Number(line.ordered_qty) - Number(line.received_qty))),
        ]),
      ));
    } catch (e) {
      setError(typeof e === "string" ? e : t("poDetailsLoadFailed"));
    } finally {
      setPoDetailLoading(false);
    }
  };

  const receivePo = (poId: string) => {
    setConfirm({
      title: t("receivePurchaseOrder"),
      description: t("receivePoDescription"),
      confirmLabel: t("receive"),
      run: async () => {
        setError(null);
        setMessage(null);
        try {
          const result = await poReceive({ po_id: poId, actor_user_id: actorUserId, lines: null });
          setMessage(`${t("po")} ${poStatusText(language, result.status)}: ${countText(language, "units", Number(result.units_received))} ${t("received")}, ${countText(language, "costUpdates", result.cost_updates)}.`);
          await load();
        } catch (e) {
          setError(typeof e === "string" ? e : t("receivePoFailed"));
        }
      },
    });
  };

  const receivePoLines = (detail: PurchaseOrderDetail) => {
    const lines = detail.lines
      .map(line => ({
        po_line_id: line.po_line_id,
        received_qty: receiveQty[line.po_line_id] ?? "0",
      }))
      .filter(line => Number(line.received_qty) > 0);
    if (lines.length === 0) {
      setError(t("receivedQuantityRequired"));
      return;
    }
    setConfirm({
      title: t("receiveSelectedLines"),
      description: t("receiveSelectedDescription"),
      confirmLabel: t("receiveLines"),
      run: async () => {
        setError(null);
        setMessage(null);
        try {
          const result = await poReceive({ po_id: detail.order.po_id, actor_user_id: actorUserId, lines });
          setMessage(`${t("po")} ${poStatusText(language, result.status)}: ${countText(language, "units", Number(result.units_received))} ${t("received")}, ${countText(language, "costUpdates", result.cost_updates)}.`);
          setPoDetail(null);
          await load();
        } catch (e) {
          setError(typeof e === "string" ? e : t("receivePoFailed"));
        }
      },
    });
  };

  const cancelPo = (poId: string) => {
    setConfirm({
      title: t("cancelPurchaseOrder"),
      description: t("cancelPoDescription"),
      confirmLabel: t("cancelPo"),
      danger: true,
      run: async () => {
        setError(null);
        setMessage(null);
        try {
          await poCancel(actorUserId, poId);
          setMessage(t("poCancelled"));
          await load();
        } catch (e) {
          setError(typeof e === "string" ? e : t("poCancelFailed"));
        }
      },
    });
  };

  return (
    <div className="oa-purchasing">
      {confirm && (
        <AppConfirmModal
          title={confirm.title}
          description={confirm.description}
          confirmLabel={confirm.confirmLabel}
          danger={confirm.danger}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const action = confirm.run;
            setConfirm(null);
            void action();
          }}
        />
      )}
      <PurchasingCommandStrip marginWarning={model.marginWarning} onSendPrompt={onSendPrompt} />

      {error && <div className="oa-inline-warning"><AlertTriangle size={16} /><span>{error}</span><button onClick={load}>{t("retry")}</button></div>}
      {message && <div className="oa-inline-success"><span>{message}</span></div>}

      <PurchasingMetrics model={model} margin={margin} currencyExp={currencyExp} />

      <section className="oa-two-column">
        <div className="oa-panel">
          <div className="oa-panel-header">
            <h2>{t("suppliers")}</h2>
            <div className="oa-inline-actions">
              <button onClick={() => downloadPurchasingCsv("zanpos-suppliers.csv", filteredSuppliers.map(s => ({
                name: s.name,
                phone: s.phone,
                email: s.email,
                contact_name: s.contact_name,
                active: s.is_active ? "yes" : "no",
                products: s.product_count,
                open_pos: s.open_po_count,
              })))} disabled={filteredSuppliers.length === 0}>{t("export")}</button>
              <button onClick={load} disabled={loading}>{t(loading ? "loading" : "refresh")}</button>
            </div>
          </div>
          <input className="bo-search" value={supplierSearch} onChange={e => setSupplierSearch(e.target.value)} placeholder={t("searchSuppliers")} />
          <div className="oa-inline-form">
            <input className="bo-input" value={supplierName} onChange={e => setSupplierName(e.target.value)} placeholder={t("supplierName")} />
            <button className="btn-primary" onClick={saveSupplier}><Plus size={15} /> {t(editingSupplier ? "save" : "add")}</button>
          </div>
          <div className="oa-form-grid">
            <input className="bo-input" value={supplierContact} onChange={e => setSupplierContact(e.target.value)} placeholder={t("contactPerson")} />
            <input className="bo-input" value={supplierPhone} onChange={e => setSupplierPhone(e.target.value)} placeholder={t("phone")} />
            <input className="bo-input" value={supplierEmail} onChange={e => setSupplierEmail(e.target.value)} placeholder={t("email")} />
            <input className="bo-input" value={supplierNotes} onChange={e => setSupplierNotes(e.target.value)} placeholder={t("notes")} />
            {editingSupplier && <button className="btn-secondary" onClick={resetSupplierForm}>{t("cancelEdit")}</button>}
          </div>
          {filteredSuppliers.length ? (
            <div className="oa-list">
              {filteredSuppliers.map(s => (
                <div key={s.supplier_id} className="oa-list-row">
                  <Users size={16} />
                  <span>
                    <strong>{s.name}</strong>
                    <small>{s.contact_name || t("noContact")} - {s.phone || t("noPhone")} - {s.product_count} {t("productsLabel")} - {s.open_po_count} {t("openPosLabel")} - {t(s.is_active ? "active" : "inactive")}</small>
                  </span>
                  <button onClick={() => startEditSupplier(s)}>{t("edit")}</button>
                  {s.is_active && <button onClick={() => deactivateSupplier(s.supplier_id)}>{t("deactivate")}</button>}
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("noSuppliers")}</div>
          )}
        </div>

        <div className="oa-panel">
          <div className="oa-panel-header"><h2>{t("createDraftPo")}</h2></div>
          <div className="oa-form-grid">
            <select className="bo-select" value={poSupplierId} onChange={e => setPoSupplierId(e.target.value)}>
              <option value="">{t("noSupplierSelected")}</option>
              {suppliers.filter(s => s.is_active).map(s => <option key={s.supplier_id} value={s.supplier_id}>{s.name}</option>)}
            </select>
            <input className="bo-input" value={poProductName} onChange={e => setPoProductName(e.target.value)} placeholder={t("productName")} />
            <input className="bo-input" value={poQty} onChange={e => setPoQty(e.target.value)} placeholder={t("quantityShort")} />
            <input className="bo-input" value={poCost} onChange={e => setPoCost(e.target.value)} placeholder={t("unitCostBhd")} />
            <button className="btn-primary" onClick={createPo}>{t("createPo")}</button>
          </div>
          <div className="oa-card-sub">{t("linkedReceivingNote")}</div>
        </div>
      </section>

      <section className="oa-two-column">
        <div className="oa-panel">
          <div className="oa-panel-header">
            <h2>{t("purchaseOrders")}</h2>
            <div className="oa-inline-actions">
              <select className="bo-select" value={poStatusFilter} onChange={e => setPoStatusFilter(e.target.value)}>
                <option value="open">{t("filterOpen")}</option>
                <option value="draft">{poStatusText(language, "draft")}</option>
                <option value="ordered">{poStatusText(language, "ordered")}</option>
                <option value="partial">{poStatusText(language, "partial")}</option>
                <option value="received">{poStatusText(language, "received")}</option>
                <option value="cancelled">{poStatusText(language, "cancelled")}</option>
                <option value="all">{t("filterAll")}</option>
              </select>
              <button onClick={() => downloadPurchasingCsv("zanpos-purchase-orders.csv", filteredOrders.map(po => ({
                po_id: po.po_id,
                status: po.status,
                supplier: po.supplier_name,
                lines: po.line_count,
                ordered_total_minor: po.ordered_total_minor,
                received_total_minor: po.received_total_minor,
              })))} disabled={filteredOrders.length === 0}>{t("export")}</button>
            </div>
          </div>
          {filteredOrders.length ? (
            <div className="oa-table">
              <div className="oa-table-head"><span>{t("po")}</span><span>{t("status")}</span><span>{t("supplier")}</span><span>{t("value")}</span></div>
              {filteredOrders.map(po => (
                <div key={po.po_id} className="oa-table-row">
                  <span>{po.po_id.slice(0, 8)}</span>
                  <span>{poStatusText(language, po.status)}</span>
                  <span>{po.supplier_name ?? t("noSupplier")}</span>
                  <span>{purchasingMoney(po.ordered_total_minor, currencyExp)}</span>
                  <button onClick={() => openPoDetail(po.po_id)} disabled={poDetailLoading}>{t("details")}</button>
                  <button onClick={() => receivePo(po.po_id)} disabled={po.line_count === 0}>{t("receive")}</button>
                  {po.status === "draft" || po.status === "ordered" ? <button onClick={() => cancelPo(po.po_id)}>{t("cancel")}</button> : null}
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("noOpenPurchaseOrders")}</div>
          )}
        </div>

        <div className="oa-panel">
          <div className="oa-panel-header">
            <h2>{t(poDetail ? "poDetail" : "marginAlerts")}</h2>
            <div className="oa-inline-actions">
              {poDetail ? (
                <button onClick={() => setPoDetail(null)}>{t("showMargins")}</button>
              ) : (
                <>
                  <input className="bo-search" value={marginSearch} onChange={e => setMarginSearch(e.target.value)} placeholder={t("findProduct")} />
                  <button onClick={() => downloadPurchasingCsv("zanpos-product-margins.csv", filteredMargins.map(row => ({
                    product_id: row.product_id,
                    product_name: row.product_name,
                    revenue_minor: row.revenue_minor,
                    cogs_minor: row.cogs_minor,
                    gross_margin_minor: row.gross_margin_minor,
                    unknown_cost_lines: row.unknown_cost_line_count,
                  })))} disabled={filteredMargins.length === 0}>{t("export")}</button>
                </>
              )}
            </div>
          </div>
          {poDetail ? (
            <div className="oa-list">
              <div className="oa-list-row">
                <PackagePlus size={16} />
                <span>
                  <strong>{poDetail.order.po_id.slice(0, 10)} - {poStatusText(language, poDetail.order.status)}</strong>
                  <small>{poDetail.order.supplier_name ?? t("noSupplier")} - {countText(language, "lines", poDetail.lines.length)} - {purchasingMoney(poDetail.order.ordered_total_minor, currencyExp)} {t("ordered")}</small>
                </span>
                <button className="oa-primary-mini" onClick={() => receivePoLines(poDetail)}>{t("receiveSelected")}</button>
              </div>
              {poDetail.lines.map(line => {
                const remaining = Math.max(0, Number(line.ordered_qty) - Number(line.received_qty));
                return (
                  <div key={line.po_line_id} className="oa-list-row">
                    <PackagePlus size={16} />
                    <span>
                      <strong>{line.product_name}</strong>
                      <small>{t("ordered")} {line.ordered_qty} - {t("received")} {line.received_qty} - {t("unitCost")} {purchasingMoney(line.unit_cost_minor, currencyExp)}</small>
                    </span>
                    <input
                      className="bo-input oa-mini-input"
                      value={receiveQty[line.po_line_id] ?? String(remaining)}
                      onChange={e => setReceiveQty(prev => ({ ...prev, [line.po_line_id]: e.target.value }))}
                      disabled={remaining <= 0}
                    />
                  </div>
                );
              })}
            </div>
          ) : filteredMargins.length ? (
            <div className="oa-list">
              {filteredMargins.map(row => (
                <div key={`${row.product_id ?? row.product_name}`} className="oa-list-row">
                  <PackagePlus size={16} />
                  <span><strong>{row.product_name}</strong><small>{t("margin")} {purchasingMoney(row.gross_margin_minor, currencyExp)} - {t("cogs")} {purchasingMoney(row.cogs_minor, currencyExp)}</small></span>
                </div>
              ))}
            </div>
          ) : (
            <div className="oa-empty-state">{t("marginSignalsEmpty")}</div>
          )}
        </div>
      </section>
    </div>
  );
}
