import { AlertTriangle, PackagePlus, Users } from "lucide-react";
import AppConfirmModal from "../components/AppConfirmModal";
import { poStatusText } from "../i18n/operationsStrings";
import PurchasingCommandStrip from "./PurchasingCommandStrip";
import PoDetail from "../command/pages/purchasing/PoDetail";
import ReceivingDrawer from "../command/pages/purchasing/ReceivingDrawer";
import { isOpen, poStatus, primaryActionFor } from "../command/pages/purchasing/poLifecycle";
import { DataTable, Drawer, EmptyState, LoadingSkeleton, PageTemplate } from "../components/templates";
import "../components/templates/datatable.css";
import { downloadPurchasingCsv, purchasingMoney } from "./purchasingPresentation";
import { usePurchasingWorkspace } from "./usePurchasingWorkspace";

export { buildPurchasingCommandModel } from "./purchasingPresentation";

interface Props {
  actorUserId: string;
  currencyExp: number;
  onSendPrompt: (prompt: string) => void;
}

export default function OfficeAIPurchasingWorkspace({ actorUserId, currencyExp, onSendPrompt }: Props) {
  const {
    language,
    t,
    suppliers,
    
    orders,
    
    
    
    productMargins,
    
    supplierSearch,
    setSupplierSearch,
    poStatusFilter,
    setPoStatusFilter,
    marginSearch,
    setMarginSearch,
    editingSupplier,
    
    supplierName,
    setSupplierName,
    supplierPhone,
    setSupplierPhone,
    supplierEmail,
    setSupplierEmail,
    supplierContact,
    setSupplierContact,
    supplierNotes,
    setSupplierNotes,
    poSupplierId,
    setPoSupplierId,
    poProductName,
    setPoProductName,
    poQty,
    setPoQty,
    poCost,
    setPoCost,
    poDetail,
    setPoDetail,
    message,
    
    error,
    
    loading,
    
    suppliersOpen,
    setSuppliersOpen,
    createOpen,
    setCreateOpen,
    exceptionsOpen,
    setExceptionsOpen,
    receivingFor,
    setReceivingFor,
    
    costHistoryOpen,
    setCostHistoryOpen,
    costHistory,
    
    costHistoryError,
    
    receiveSubmitting,
    
    receiveError,
    setReceiveError,
    confirm,
    setConfirm,
    
    load,
    model,
    filteredSuppliers,
    filteredOrders,
    poColumns,
    filteredMargins,
    startEditSupplier,
    resetSupplierForm,
    saveSupplier,
    deactivateSupplier,
    createPo,
    openPoDetail,
    receivePo,
    submitReceipt,
    openCostHistory,
    cancelPo,
  } = usePurchasingWorkspace({ actorUserId, currencyExp });

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
      <PageTemplate
        contentFlat
        header={{
          title: t("purchasing"),
          subtitle: t("purchasingSubtitle"),
          primaryAction: { label: t("createPurchaseOrder"), onClick: () => setCreateOpen(true) },
          secondaryActions: [
            { label: t("suppliers"), onClick: () => setSuppliersOpen(true) },
            { label: t("costHistory"), onClick: () => void openCostHistory() },
            { label: t("refresh"), onClick: () => void load(), disabled: loading },
          ],
        }}
        toolbar={
          <div className="zp-toolbar">
            <select
              className="zp-filter"
              value={poStatusFilter}
              aria-label={t("status")}
              onChange={e => setPoStatusFilter(e.target.value)}
            >
              <option value="open">{t("filterOpen")}</option>
              <option value="draft">{poStatusText(language, "draft")}</option>
              <option value="ordered">{poStatusText(language, "ordered")}</option>
              <option value="partial">{poStatusText(language, "partial")}</option>
              <option value="received">{poStatusText(language, "received")}</option>
              <option value="cancelled">{poStatusText(language, "cancelled")}</option>
              <option value="all">{t("filterAll")}</option>
            </select>
            <button
              className="zp-chip-clear"
              onClick={() => downloadPurchasingCsv("zanpos-purchase-orders.csv", filteredOrders.map(po => ({
                po_id: po.po_id,
                status: po.status,
                supplier: po.supplier_name,
                lines: po.line_count,
                ordered_total_minor: po.ordered_total_minor,
                received_total_minor: po.received_total_minor,
              })))}
              disabled={filteredOrders.length === 0}
            >
              {t("export")}
            </button>
            <span className="zp-toolbar-spacer" />
            <span className="zp-toolbar-count" aria-live="polite">
              {filteredOrders.length} / {orders.length}
            </span>
          </div>
        }
      >
        {error && orders.length > 0 && (
          <div className="oa-inline-warning"><AlertTriangle size={16} /><span>{error}</span><button onClick={load}>{t("retry")}</button></div>
        )}
        {message && <div className="oa-inline-success"><span>{message}</span></div>}

        {/* Margin exceptions are sales-derived and not tied to any single order,
            so they are a Purchasing-level exception summary rather than a panel
            competing with the order list. */}
        {productMargins.length > 0 && (
          <div className="zp-po-exceptions">
            <AlertTriangle size={15} aria-hidden="true" />
            <span><strong>{productMargins.length}</strong> {t("marginAlerts")}</span>
            <button className="oa-tool-btn" onClick={() => setExceptionsOpen(true)}>{t("details")}</button>
          </div>
        )}

        <div className={`zp-po-workspace${poDetail ? " has-selection" : ""}`}>
          <div className="zp-po-list">
            {error && orders.length === 0 ? (
              <EmptyState
                variant="degraded"
                title={t("purchasingLoadFailed")}
                description={error}
                stillWorks={t("sellingUnaffectedShort")}
                actions={[{ label: t("retry"), onClick: () => void load(), primary: true }]}
              />
            ) : filteredOrders.length > 0 ? (
              <DataTable
                caption={t("purchaseOrders")}
                rows={filteredOrders}
                rowKey={po => po.po_id}
                onRowClick={po => openPoDetail(po.po_id)}
                isRowActive={po => poDetail?.order.po_id === po.po_id}
                isRowMuted={po => poStatus(po.status) === "cancelled"}
                columns={poColumns}
                rowAction={po => {
                  const primary = primaryActionFor(po.status);
                  return (
                    <span className="zp-po-actions">
                      {primary === "receive" && (
                        <button className="btn-secondary zp-row-action" onClick={() => receivePo(po.po_id)} disabled={po.line_count === 0 || loading}>
                          {t("receive")}
                        </button>
                      )}
                      {isOpen(po.status) && (
                        <button className="btn-secondary zp-row-action" onClick={() => cancelPo(po.po_id)}>{t("cancel")}</button>
                      )}
                    </span>
                  );
                }}
              />
            ) : orders.length > 0 ? (
              <EmptyState
                variant="no-results"
                title={t("noMatchingOrders")}
                description={t("noMatchingOrdersHint")}
                actions={[{ label: t("filterAll"), onClick: () => setPoStatusFilter("all"), primary: true }]}
              />
            ) : (
              <EmptyState
                variant="first-use"
                title={t("noPurchaseOrdersYet")}
                description={t("noPurchaseOrdersHint")}
                actions={[{ label: t("createPurchaseOrder"), onClick: () => setCreateOpen(true), primary: true }]}
              />
            )}
          </div>

          {poDetail && (
            <PoDetail
              detail={poDetail}
              language={language}
              t={t}
              currencyExp={currencyExp}
              busy={loading}
              onReceive={receivePo}
              onCancel={cancelPo}
              onClose={() => setPoDetail(null)}
            />
          )}
        </div>

        {/* AI sits below the deterministic work surface so a recommendation is
            never visually equivalent to a store command. */}
        <PurchasingCommandStrip marginWarning={model.marginWarning} onSendPrompt={onSendPrompt} />
      </PageTemplate>

      <Drawer
        open={suppliersOpen}
        onOpenChange={open => { setSuppliersOpen(open); if (!open) resetSupplierForm(); }}
        title={t("suppliers")}
        footer={
          <>
            <button className="oa-tool-btn" onClick={() => { resetSupplierForm(); setSuppliersOpen(false); }}>{t("close")}</button>
            <button className="oa-primary-mini" onClick={saveSupplier} disabled={!supplierName.trim()}>
              {editingSupplier ? t("save") : t("addSupplier")}
            </button>
          </>
        }
      >
        <div className="zp-field">
          <label htmlFor="sup-name">{t("supplierName")}</label>
          <input id="sup-name" value={supplierName} onChange={e => setSupplierName(e.target.value)} />
        </div>
        <div className="zp-field-row">
          <div className="zp-field">
            <label htmlFor="sup-contact">{t("contactPerson")}</label>
            <input id="sup-contact" value={supplierContact} onChange={e => setSupplierContact(e.target.value)} />
          </div>
          <div className="zp-field">
            <label htmlFor="sup-phone">{t("phone")}</label>
            <input id="sup-phone" value={supplierPhone} onChange={e => setSupplierPhone(e.target.value)} />
          </div>
        </div>
        <div className="zp-field">
          <label htmlFor="sup-email">{t("email")}</label>
          <input id="sup-email" type="email" value={supplierEmail} onChange={e => setSupplierEmail(e.target.value)} />
        </div>
        <div className="zp-field">
          <label htmlFor="sup-notes">{t("notes")}</label>
          <textarea id="sup-notes" rows={2} value={supplierNotes} onChange={e => setSupplierNotes(e.target.value)} />
        </div>

        <div className="zp-field">
          <label htmlFor="sup-search">{t("searchSuppliers")}</label>
          <input id="sup-search" type="search" value={supplierSearch} onChange={e => setSupplierSearch(e.target.value)} />
        </div>

        {filteredSuppliers.length === 0 ? (
          <EmptyState
            variant={suppliers.length === 0 ? "first-use" : "no-results"}
            title={suppliers.length === 0 ? t("noSuppliersYet") : t("noMatchingSuppliers")}
            description={suppliers.length === 0 ? t("noSuppliersHint") : undefined}
          />
        ) : (
          filteredSuppliers.map(s => (
            <div key={s.supplier_id} className="zp-supplier-row">
              <Users size={15} aria-hidden="true" />
              <span className="zp-supplier-main">
                <span className="zp-supplier-name">{s.name}</span>
                <span className="zp-supplier-sub">{[s.contact_name, s.phone].filter(Boolean).join(" · ")}</span>
              </span>
              <button className="oa-tool-btn" onClick={() => startEditSupplier(s)}>{t("edit")}</button>
              <button className="oa-tool-btn" onClick={() => deactivateSupplier(s.supplier_id)}>{t("deactivate")}</button>
            </div>
          ))
        )}
      </Drawer>

      <Drawer
        open={createOpen}
        onOpenChange={setCreateOpen}
        title={t("createPurchaseOrder")}
        description={t("linkedReceivingNote")}
        footer={
          <>
            <button className="oa-tool-btn" onClick={() => setCreateOpen(false)}>{t("cancel")}</button>
            <button
              className="oa-primary-mini"
              onClick={async () => { await createPo(); setCreateOpen(false); }}
              disabled={!poProductName.trim()}
            >
              {t("createPo")}
            </button>
          </>
        }
      >
        {/* No supplier yet: guide instead of failing ambiguously. */}
        {suppliers.length === 0 && (
          <div className="zp-po-exceptions">
            <AlertTriangle size={15} aria-hidden="true" />
            <span>{t("noSuppliersHint")}</span>
            <button className="oa-tool-btn" onClick={() => { setCreateOpen(false); setSuppliersOpen(true); }}>
              {t("addSupplier")}
            </button>
          </div>
        )}
        <div className="zp-field">
          <label htmlFor="po-supplier">{t("supplier")}</label>
          <select id="po-supplier" value={poSupplierId} onChange={e => setPoSupplierId(e.target.value)}>
            <option value="">{t("noSupplierSelected")}</option>
            {suppliers.filter(s => s.is_active).map(s => (
              <option key={s.supplier_id} value={s.supplier_id}>{s.name}</option>
            ))}
          </select>
        </div>
        <div className="zp-field">
          <label htmlFor="po-product">{t("productName")}</label>
          <input id="po-product" value={poProductName} onChange={e => setPoProductName(e.target.value)} />
        </div>
        <div className="zp-field-row">
          <div className="zp-field">
            <label htmlFor="po-qty">{t("quantityShort")}</label>
            <input id="po-qty" inputMode="decimal" value={poQty} onChange={e => setPoQty(e.target.value)} />
          </div>
          <div className="zp-field">
            <label htmlFor="po-cost">{t("unitCostBhd")}</label>
            <input id="po-cost" inputMode="decimal" value={poCost} onChange={e => setPoCost(e.target.value)} />
          </div>
        </div>
      </Drawer>

      {receivingFor && (
        <ReceivingDrawer
          open
          onOpenChange={open => { if (!open) { setReceivingFor(null); setReceiveError(null); } }}
          detail={receivingFor}
          t={t}
          submitting={receiveSubmitting}
          error={receiveError}
          onSubmit={submitReceipt}
        />
      )}

      <Drawer
        open={exceptionsOpen}
        onOpenChange={setExceptionsOpen}
        title={t("marginAlerts")}
        width="lg"
      >
        <div className="zp-field">
          <label htmlFor="margin-search">{t("findProduct")}</label>
          <input id="margin-search" type="search" value={marginSearch} onChange={e => setMarginSearch(e.target.value)} />
        </div>
        {filteredMargins.length === 0 ? (
          <EmptyState
            variant={productMargins.length === 0 ? "first-use" : "no-results"}
            title={t("marginSignalsEmpty")}
          />
        ) : (
          filteredMargins.map(row => (
            <div key={`${row.product_id ?? row.product_name}`} className="zp-supplier-row">
              <PackagePlus size={15} aria-hidden="true" />
              <span className="zp-supplier-main">
                <span className="zp-supplier-name">{row.product_name}</span>
                <span className="zp-supplier-sub">
                  {t("margin")} {purchasingMoney(row.gross_margin_minor, currencyExp)}
                </span>
              </span>
            </div>
          ))
        )}
      </Drawer>

      {/* Cost history.
          Deliberately NOT called reconciliation or invoice variance: the row
          carries no po_id, so a change cannot be traced to the order that
          caused it -- only to the supplier recorded at the time. It is also
          store-wide rather than per-branch, because `products.cost_minor` has
          no branch dimension. The copy says exactly that. */}
      <Drawer
        open={costHistoryOpen}
        onOpenChange={open => { if (!open) setCostHistoryOpen(false); }}
        title={t("costHistory")}
        description={t("costHistoryScope")}
        width="lg"
      >
        {costHistoryError ? (
          <EmptyState
            variant="degraded"
            title={t("costHistoryFailed")}
            description={costHistoryError}
            actions={[{ label: t("retry"), onClick: () => void openCostHistory(), primary: true }]}
          />
        ) : costHistory === null ? (
          <LoadingSkeleton variant="table" count={5} />
        ) : costHistory.length === 0 ? (
          <EmptyState
            variant="first-use"
            title={t("noCostChanges")}
            description={t("noCostChangesHint")}
          />
        ) : (
          <table className="zp-po-lines">
            <thead>
              <tr>
                <th>{t("productName")}</th>
                <th>{t("supplier")}</th>
                <th className="zp-numeric">{t("previousCost")}</th>
                <th className="zp-numeric">{t("newCost")}</th>
                <th>{t("when")}</th>
              </tr>
            </thead>
            <tbody>
              {costHistory.map(c => (
                <tr key={c.cost_history_id}>
                  <td>{c.product_name}</td>
                  <td>{c.supplier_name ?? <span className="zp-status-muted">—</span>}</td>
                  <td className="zp-numeric">
                    {c.old_cost_minor == null
                      ? <span className="zp-status-muted">—</span>
                      : purchasingMoney(c.old_cost_minor, currencyExp)}
                  </td>
                  <td className="zp-numeric">{purchasingMoney(c.new_cost_minor, currencyExp)}</td>
                  <td>{new Date(c.created_at).toLocaleDateString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Drawer>
    </div>
  );
}
