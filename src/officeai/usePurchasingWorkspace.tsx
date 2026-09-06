/**
 * Everything the purchasing workspace does, minus how it looks.
 *
 * Split from OfficeAIPurchasingWorkspace, which was 776 lines against the
 * 500-line ship-gate rule. The seam is state and effects on this side, markup
 * on the other — the component destructures this with the same names, so the
 * JSX it renders is byte-for-byte what it was.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SessionToken } from "../types";
import {
  appConfigLoad,
  poCancel,
  poCreate,
  poGet,
  poList,
  poReceive,
  productCostHistoryList,
  reportMargin,
  reportProductMargin,
  supplierDelete,
  supplierList,
  supplierUpsert,
} from "../tauri/commands";
import type {
  MarginSummary,
  ProductCostChange,
  ProductMarginRow,
  PurchaseOrderDetail,
  PurchaseOrderRow,
  ReceivePurchaseOrderResult,
  SupplierRow,
} from "../types";
import { useLanguage } from "../hooks/useLanguage";
import { countText, operationsTranslator, poStatusText } from "../i18n/operationsStrings";
import { isOpen, poStatus, PO_PIPELINE, PO_STATUS } from "../command/pages/purchasing/poLifecycle";
import type { Column } from "../components/templates";
import {
  buildPurchasingCommandModel,
  defaultPurchasingRange,
  purchasingMoney,
} from "./purchasingPresentation";

export function usePurchasingWorkspace(props: {
  actorUserId: string;
  sessionToken: SessionToken;
  currencyExp: number;
}) {
  const { actorUserId, sessionToken, currencyExp } = props;
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
  const [, setPoDetailLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // Secondary workflows live in drawers so they never occupy the canvas.
  const [suppliersOpen, setSuppliersOpen] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [exceptionsOpen, setExceptionsOpen] = useState(false);
  const [receivingFor, setReceivingFor] = useState<PurchaseOrderDetail | null>(null);
  /**
   * Identity of the receiving submission currently in the drawer.
   *
   * Minted when the drawer opens and reused for every retry of that same
   * submission, which is the whole point: if a request times out after the
   * server already applied it, the retry carries the same key and the server
   * rejects it instead of receiving the goods twice. Regenerating per attempt
   * would leave exactly the hole this closes.
   */
  const receiveOpKey = useRef<string>("");

  // Cost history. Loaded on demand rather than with the page: it is reference
  // material, not something the receiving workflow depends on.
  const [costHistoryOpen, setCostHistoryOpen] = useState(false);
  const [costHistory, setCostHistory] = useState<ProductCostChange[] | null>(null);
  const [costHistoryError, setCostHistoryError] = useState<string | null>(null);
  const [receiveSubmitting, setReceiveSubmitting] = useState(false);
  const [receiveError, setReceiveError] = useState<string | null>(null);
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
        supplierList(sessionToken),
        poList(sessionToken),
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
  }, [actorUserId, sessionToken, range.from, range.to, t]);

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
      if (poStatusFilter === "open") return isOpen(po.status);
      if (poStatusFilter === "all") return true;
      return po.status === poStatusFilter;
    })
    .slice(0, 50);

  /** PO columns. Only fields the backend actually returns on PurchaseOrderRow. */
  const poColumns: Column<PurchaseOrderRow>[] = [
    {
      id: "po",
      header: t("po"),
      cell: po => (
        <>
          <span className="zp-cell-primary">{po.po_id.slice(0, 8)}</span>
          <span className="zp-cell-sub">{po.supplier_name ?? t("noSupplier")}</span>
        </>
      ),
    },
    {
      id: "status",
      header: t("status"),
      width: "150px",
      cell: po => {
        const meta = PO_STATUS[poStatus(po.status)];
        return (
          <span className={`zp-status zp-status-${meta.tone}`}>
            {poStatusText(language, po.status)}
            {meta.step >= 0 && (
              <small className="zp-po-step">{meta.step + 1}/{PO_PIPELINE.length}</small>
            )}
          </span>
        );
      },
    },
    {
      id: "lines",
      header: t("lines"),
      align: "end",
      numeric: true,
      width: "90px",
      priority: 3,
      cell: po => po.line_count,
    },
    {
      id: "ordered",
      header: t("value"),
      align: "end",
      numeric: true,
      width: "130px",
      cell: po => purchasingMoney(po.ordered_total_minor, currencyExp),
    },
    {
      id: "received",
      header: t("received"),
      align: "end",
      numeric: true,
      width: "130px",
      priority: 2,
      cell: po => purchasingMoney(po.received_total_minor, currencyExp),
    },
    {
      id: "expected",
      header: t("expected"),
      width: "120px",
      priority: 3,
      cell: po => po.expected_date
        ? new Date(po.expected_date).toLocaleDateString()
        : <span className="zp-status-muted">—</span>,
    },
  ];
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
      await supplierUpsert(sessionToken, {
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
          await supplierDelete(sessionToken, supplierId);
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
      await poCreate(sessionToken, {
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
      const detail = await poGet(sessionToken, poId);
      setPoDetail(detail);
    } catch (e) {
      setError(typeof e === "string" ? e : t("poDetailsLoadFailed"));
    } finally {
      setPoDetailLoading(false);
    }
  };

  /**
   * Open the per-line receiving editor. The order detail is loaded first so the
   * editor always shows current ordered/received quantities rather than a stale
   * list row.
   */
  const receivePo = async (poId: string) => {
    setReceiveError(null);
    setError(null);
    try {
      const detail = poDetail?.order.po_id === poId ? poDetail : await poGet(sessionToken, poId);
      setPoDetail(detail);
      receiveOpKey.current = crypto.randomUUID();
      setReceivingFor(detail);
    } catch (e) {
      setError(typeof e === "string" ? e : t("poDetailsLoadFailed"));
    }
  };

  /**
   * Submit a receipt. Nothing in the UI claims stock has moved until the
   * backend confirms; on failure the drawer keeps the entered quantities.
   */
  const submitReceipt = async (
    lines: { po_line_id: string; received_qty: string }[],
  ): Promise<ReceivePurchaseOrderResult | null> => {
    if (!receivingFor || receiveSubmitting) return null;
    setReceiveSubmitting(true);
    setReceiveError(null);
    try {
      const result = await poReceive({
        po_id: receivingFor.order.po_id,
        idempotency_key: receiveOpKey.current,
        lines,
      }, sessionToken);
      setMessage(
        `${t("po")} ${poStatusText(language, result.status)}: ${countText(language, "units", Number(result.units_received))} ${t("received")}, ${countText(language, "costUpdates", result.cost_updates)}.`,
      );
      // Reconcile every surface that shows this order.
      const fresh = await poGet(sessionToken, receivingFor.order.po_id);
      setPoDetail(fresh);
      await load();
      // Applied: the next receiving action is a new operation.
      receiveOpKey.current = crypto.randomUUID();
      return result;
    } catch (e) {
      setReceiveError(typeof e === "string" ? e : t("receivePoFailed"));
      return null;
    } finally {
      setReceiveSubmitting(false);
    }
  };

  const openCostHistory = async () => {
    setCostHistoryOpen(true);
    setCostHistoryError(null);
    setCostHistory(null);
    try {
      setCostHistory(await productCostHistoryList(sessionToken));
    } catch (e) {
      setCostHistoryError(typeof e === "string" ? e : t("costHistoryFailed"));
    }
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
          await poCancel(sessionToken, poId);
          setMessage(t("poCancelled"));
          await load();
        } catch (e) {
          setError(typeof e === "string" ? e : t("poCancelFailed"));
        }
      },
    });
  };

  return {
    language,
    t,
    suppliers,
    setSuppliers,
    orders,
    setOrders,
    margin,
    setMargin,
    productMargins,
    setProductMargins,
    supplierSearch,
    setSupplierSearch,
    poStatusFilter,
    setPoStatusFilter,
    marginSearch,
    setMarginSearch,
    editingSupplier,
    setEditingSupplier,
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
    setMessage,
    error,
    setError,
    loading,
    setLoading,
    suppliersOpen,
    setSuppliersOpen,
    createOpen,
    setCreateOpen,
    exceptionsOpen,
    setExceptionsOpen,
    receivingFor,
    setReceivingFor,
    receiveOpKey,
    costHistoryOpen,
    setCostHistoryOpen,
    costHistory,
    setCostHistory,
    costHistoryError,
    setCostHistoryError,
    receiveSubmitting,
    setReceiveSubmitting,
    receiveError,
    setReceiveError,
    confirm,
    setConfirm,
    range,
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
  };
}
