import { useCallback, useEffect, useMemo, useState } from "react";
import { Minus, Plus, Users } from "lucide-react";
import type { CustomerRow } from "../../../types";
import { customerAddLoyalty, customerCreate, customerList, customerUpdate } from "../../../tauri/commands";
import { useLanguage } from "../../../hooks/useLanguage";
import { countText, operationsTranslator, type OperationsStringKey } from "../../../i18n/operationsStrings";
import {
  ConfirmDialog, DataTable, Drawer, EmptyState, LoadingSkeleton, PageTemplate, Toolbar,
} from "../../../components/templates";
import {
  directoryState, firstCustomerError, loyaltyDelta, projectedBalance,
  validateCustomerField, validateLoyaltyInput,
  type CustomerField, type CustomerFieldError, type LoyaltyDirection,
} from "./customerModel";
import { customerDirectoryColumns } from "./customerDirectoryColumns";
import "./customers.css";

const PAGE_SIZE = 50;

interface Props {
  actorUserId: string;
  /** Loyalty adjustment is manager_or_owner server-side. */
  canAdjustLoyalty: boolean;
}

/**
 * Customers: directory and loyalty as one workspace.
 *
 * Everything rendered here is backed by a real command. There is no spend,
 * visit count or last-visit column because `report_sales_list` cannot filter by
 * customer — inventing those would be the most tempting and least honest thing
 * this screen could do. Likewise there is no loyalty history: `loyalty_points`
 * is a single integer with no ledger behind it.
 */
export default function CustomersWorkspace({ actorUserId, canAdjustLoyalty }: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => operationsTranslator(language), [language]);

  const [rows, setRows] = useState<CustomerRow[]>([]);
  const [selected, setSelected] = useState<CustomerRow | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Server-side paging: the directory holds one page, `total` describes the
  // whole branch match so the count never overstates what was loaded.
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);

  // Drawers
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<CustomerRow | null>(null);
  const [fName, setFName] = useState("");
  const [fPhone, setFPhone] = useState("");
  const [fEmail, setFEmail] = useState("");
  const [fNotes, setFNotes] = useState("");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  // Loyalty
  const [loyaltyOpen, setLoyaltyOpen] = useState(false);
  const [direction, setDirection] = useState<LoyaltyDirection>("add");
  const [pointsInput, setPointsInput] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [loyaltyBusy, setLoyaltyBusy] = useState(false);
  const [loyaltyError, setLoyaltyError] = useState<string | null>(null);

  // `customer_list` performs the search server-side, so this is a real query,
  // not a filter over an already-loaded page.
  useEffect(() => {
    const id = setTimeout(() => { setSearch(searchInput); setOffset(0); }, 300);
    return () => clearTimeout(id);
  }, [searchInput]);

  const load = useCallback(async (q: string, off: number) => {
    setLoading(true);
    setError(null);
    try {
      const page = await customerList(actorUserId, q, off, PAGE_SIZE);
      setRows(page.items);
      setTotal(page.total);
      setOffset(page.offset);
      // Keep the open profile only if it is still on the page in view.
      setSelected(prev =>
        prev ? page.items.find(r => r.customer_id === prev.customer_id) ?? null : null);
    } catch (e) {
      setError(typeof e === "string" ? e : t("customersLoadFailed"));
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, [actorUserId, t]);

  useEffect(() => { void load(search, offset); }, [load, search, offset]);

  const state = directoryState(rows, search.trim() !== "", error);

  function openCreate() {
    setEditing(null);
    setFName(""); setFPhone(""); setFEmail(""); setFNotes("");
    setFormError(null);
    setFormOpen(true);
  }

  function openEdit(c: CustomerRow) {
    setEditing(c);
    setFName(c.name); setFPhone(c.phone ?? ""); setFEmail(c.email ?? ""); setFNotes(c.notes ?? "");
    setFormError(null);
    setFormOpen(true);
  }

  const formBlocker = firstCustomerError({ name: fName, phone: fPhone, email: fEmail });

  const FIELD_ERROR_TEXT: Record<CustomerFieldError, OperationsStringKey> = {
    "name-required": "nameRequired",
    "name-too-long": "nameTooLong",
    "phone-too-long": "phoneTooLong",
    "phone-charset": "phoneCharset",
    "email-invalid": "emailInvalid",
  };

  /** Message for a field, but only once the person has typed something in it —
      a required-field error on an untouched form is a scolding, not help. */
  function fieldError(field: CustomerField): string | null {
    const value = field === "name" ? fName : field === "phone" ? fPhone : fEmail;
    if (value.trim() === "") return null;
    const err = validateCustomerField(field, value);
    return err ? t(FIELD_ERROR_TEXT[err]) : null;
  }

  async function saveCustomer() {
    if (formBlocker || saving) return;
    setSaving(true);
    setFormError(null);
    try {
      if (editing) {
        await customerUpdate({
          customer_id: editing.customer_id, name: fName.trim(),
          phone: fPhone.trim() || undefined, email: fEmail.trim() || undefined,
          notes: fNotes.trim() || undefined, actor_user_id: actorUserId,
        });
      } else {
        await customerCreate({
          name: fName.trim(), phone: fPhone.trim() || undefined,
          email: fEmail.trim() || undefined, notes: fNotes.trim() || undefined,
          actor_user_id: actorUserId,
        });
      }
      setFormOpen(false);
      await load(search, offset);
    } catch (e) {
      setFormError(typeof e === "string" ? e : t("saveFailed"));
    } finally {
      setSaving(false);
    }
  }

  const balance = selected?.loyalty_points ?? 0;
  const pointsError = validateLoyaltyInput(pointsInput, direction, balance);
  const projected = projectedBalance(pointsInput, direction, balance);

  async function applyLoyalty() {
    const delta = selected ? loyaltyDelta(pointsInput, direction, balance) : null;
    if (delta === null || !selected || loyaltyBusy) return;
    setConfirming(false);
    setLoyaltyBusy(true);
    setLoyaltyError(null);
    try {
      // Returns the new balance; the list is reloaded so both surfaces agree.
      await customerAddLoyalty(actorUserId, selected.customer_id, delta);
      setLoyaltyOpen(false);
      setPointsInput("");
      await load(search, offset);
    } catch (e) {
      setLoyaltyError(typeof e === "string" ? e : t("loyaltyUpdateFailed"));
    } finally {
      setLoyaltyBusy(false);
    }
  }

  const columns = customerDirectoryColumns(t);

  return (
    <>
      <PageTemplate
        contentFlat
        header={{
          title: t("customers"),
          primaryAction: { label: t("newCustomer"), onClick: openCreate },
        }}
        toolbar={
          <Toolbar
            search={{
              value: searchInput, onChange: setSearchInput,
              placeholder: t("searchCustomer"), label: t("searchCustomer"),
            }}
            count={total ? countText(language, "customers", total) : ""}
            onClear={searchInput ? () => setSearchInput("") : undefined}
            clearLabel={t("clearFilters")}
          />
        }
      >
        <div className={`zp-cust-workspace${selected ? " has-selection" : ""}`}>
          <div>
            {/* A first fetch must not flash "No customers yet" before the rows
                arrive — that is the same false statement the empty states exist
                to prevent, just briefer. */}
            {loading && rows.length === 0 ? (
              <LoadingSkeleton variant="table" />
            ) : state === "degraded" ? (
              <EmptyState
                variant="degraded"
                title={t("customersLoadFailed")}
                description={error ?? undefined}
                stillWorks={t("sellingUnaffectedShort")}
                actions={[{ label: t("retry"), onClick: () => void load(search, offset), primary: true }]}
              />
            ) : state === "no-results" ? (
              <EmptyState
                variant="no-results"
                title={t("noCustomersFound")}
                description={t("noMatchingCustomersHint")}
                actions={[{ label: t("clearFilters"), onClick: () => setSearchInput(""), primary: true }]}
              />
            ) : state === "first-use" ? (
              <EmptyState
                variant="first-use"
                icon={<Users size={32} strokeWidth={1.5} />}
                title={t("noCustomersYet")}
                description={t("noCustomersYetHint")}
                actions={[{ label: t("newCustomer"), onClick: openCreate, primary: true }]}
              />
            ) : (
              <>
                <DataTable
                  caption={t("customers")}
                  rows={rows}
                  rowKey={c => c.customer_id}
                  columns={columns}
                  onRowClick={setSelected}
                  isRowActive={c => selected?.customer_id === c.customer_id}
                />
                {/* Shown only when the branch actually has more than one page.
                    Search resets to page one, so a match is never hidden behind
                    a stale offset. */}
                {total > PAGE_SIZE && (
                  <div className="bo-pagination">
                    <span className="bo-pagination-info">
                      {loading ? t("loading")
                        : `${offset + 1}–${Math.min(offset + rows.length, total)} ${t("of")} ${total.toLocaleString()}`}
                    </span>
                    <button
                      className="bo-pagination-btn"
                      disabled={offset === 0 || loading}
                      onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                    >
                      <span className="icon-directional" aria-hidden="true">‹</span> {t("previous")}
                    </button>
                    <button
                      className="bo-pagination-btn"
                      disabled={offset + PAGE_SIZE >= total || loading}
                      onClick={() => setOffset(offset + PAGE_SIZE)}
                    >
                      {t("next")} <span className="icon-directional" aria-hidden="true">›</span>
                    </button>
                  </div>
                )}
              </>
            )}
          </div>

          {selected && (
            <aside className="zp-cust-detail" aria-label={t("customerProfile")}>
              <header className="zp-cust-detail-head">
                <span className="zp-cust-name">{selected.name}</span>
                <button type="button" className="zp-cust-close" onClick={() => setSelected(null)}>
                  {t("close")}
                </button>
              </header>

              <dl className="zp-cust-facts">
                <div><dt>{t("phone")}</dt><dd>{selected.phone ?? "—"}</dd></div>
                <div><dt>{t("email")}</dt><dd>{selected.email ?? "—"}</dd></div>
                <div><dt>{t("customerSince")}</dt><dd>{new Date(selected.created_at).toLocaleDateString()}</dd></div>
              </dl>

              {selected.notes && <p className="zp-cust-notes">{selected.notes}</p>}

              {/* Loyalty: balance only. There is no ledger behind it. */}
              <h3 className="zp-cust-section-title">{t("loyalty")}</h3>
              <div className="zp-cust-loyalty">
                <span className="zp-cust-balance zp-numeric">{balance}</span>
                <span className="zp-cust-balance-label">{t("pointsBalance")}</span>
              </div>
              <p className="zp-cust-note">{t("loyaltyNoHistory")}</p>

              <div className="zp-cust-actions">
                <button type="button" className="oa-tool-btn" onClick={() => openEdit(selected)}>
                  {t("editCustomer")}
                </button>
                <button
                  type="button"
                  className="oa-primary-mini"
                  disabled={!canAdjustLoyalty}
                  title={canAdjustLoyalty ? undefined : t("loyaltyManagerOnly")}
                  onClick={() => { setPointsInput(""); setLoyaltyError(null); setLoyaltyOpen(true); }}
                >
                  {t("adjustLoyalty")}
                </button>
              </div>
              {!canAdjustLoyalty && <p className="zp-cust-note">{t("loyaltyManagerOnly")}</p>}
            </aside>
          )}
        </div>
      </PageTemplate>

      {/* ── Add / edit ─────────────────────────────────────────────────────── */}
      <Drawer
        open={formOpen}
        onOpenChange={setFormOpen}
        title={editing ? t("editCustomer") : t("newCustomer")}
        footer={
          <>
            <button className="oa-tool-btn" onClick={() => setFormOpen(false)} disabled={saving}>
              {t("cancel")}
            </button>
            <button
              className="oa-primary-mini"
              onClick={saveCustomer}
              disabled={saving || formBlocker !== null}
            >
              {saving ? t("saving") : t("save")}
            </button>
          </>
        }
      >
        {formError && <div className="zp-cust-error" role="alert"><span>{formError}</span></div>}
        <div className="zp-field">
          <label htmlFor="cust-name">{t("fullName")} *</label>
          <input
            id="cust-name"
            value={fName}
            onChange={e => setFName(e.target.value)}
            aria-invalid={fieldError("name") !== null || undefined}
            aria-describedby={fieldError("name") ? "cust-name-err" : undefined}
          />
          {fieldError("name") && (
            <span id="cust-name-err" className="zp-cust-field-error">{fieldError("name")}</span>
          )}
        </div>
        <div className="zp-field">
          <label htmlFor="cust-phone">{t("phone")}</label>
          <input
            id="cust-phone"
            inputMode="tel"
            value={fPhone}
            onChange={e => setFPhone(e.target.value)}
            aria-invalid={fieldError("phone") !== null || undefined}
            aria-describedby={fieldError("phone") ? "cust-phone-err" : undefined}
          />
          {fieldError("phone") && (
            <span id="cust-phone-err" className="zp-cust-field-error">{fieldError("phone")}</span>
          )}
        </div>
        <div className="zp-field">
          <label htmlFor="cust-email">{t("email")}</label>
          <input
            id="cust-email"
            type="email"
            value={fEmail}
            onChange={e => setFEmail(e.target.value)}
            aria-invalid={fieldError("email") !== null || undefined}
            aria-describedby={fieldError("email") ? "cust-email-err" : undefined}
          />
          {fieldError("email") && (
            <span id="cust-email-err" className="zp-cust-field-error">{fieldError("email")}</span>
          )}
        </div>
        <div className="zp-field">
          <label htmlFor="cust-notes">{t("notes")}</label>
          <textarea id="cust-notes" rows={3} value={fNotes} onChange={e => setFNotes(e.target.value)} />
        </div>
      </Drawer>

      {/* ── Loyalty adjustment ─────────────────────────────────────────────── */}
      <Drawer
        open={loyaltyOpen}
        onOpenChange={o => { setLoyaltyOpen(o); if (!o) setLoyaltyError(null); }}
        title={t("adjustLoyalty")}
        description={selected?.name}
        footer={
          <>
            <button className="oa-tool-btn" onClick={() => setLoyaltyOpen(false)} disabled={loyaltyBusy}>
              {t("cancel")}
            </button>
            <button
              className="oa-primary-mini"
              onClick={() => setConfirming(true)}
              disabled={loyaltyBusy || pointsError !== null}
            >
              {direction === "add" ? t("awardPoints") : t("redeemPoints")}
            </button>
          </>
        }
      >
        {loyaltyError && <div className="zp-cust-error" role="alert"><span>{loyaltyError}</span></div>}

        <div className="zp-cust-current">
          {t("pointsBalance")} <strong className="zp-numeric">{balance}</strong>
        </div>

        <div className="zp-field">
          <span className="zp-cust-seg-label" id="loyalty-dir">{t("action")}</span>
          <div className="zp-cust-seg" role="group" aria-labelledby="loyalty-dir">
            {(["add", "redeem"] as const).map(d => (
              <button
                key={d}
                type="button"
                className={`zp-cust-seg-btn${direction === d ? " is-on" : ""}`}
                aria-pressed={direction === d}
                onClick={() => setDirection(d)}
              >
                {d === "add" ? <Plus size={13} aria-hidden="true" /> : <Minus size={13} aria-hidden="true" />}
                {d === "add" ? t("awardPoints") : t("redeemPoints")}
              </button>
            ))}
          </div>
        </div>

        <div className="zp-field">
          <label htmlFor="loyalty-points">{t("points")}</label>
          <input
            id="loyalty-points"
            inputMode="numeric"
            value={pointsInput}
            aria-invalid={pointsError ? true : undefined}
            aria-describedby={pointsError ? "loyalty-points-err" : undefined}
            onChange={e => setPointsInput(e.target.value)}
          />
          {pointsError && (
            <span className="zp-cust-field-error" id="loyalty-points-err">
              {pointsError === "insufficient" ? t("insufficientPoints")
                : pointsError === "negative-input" ? t("pointsPositiveOnly")
                : pointsError === "not-a-number" ? t("pointsWholeNumber")
                : t("pointsNonZero")}
            </span>
          )}
          {projected !== null && (
            <span className="zp-field-hint">{t("newBalance")} {projected}</span>
          )}
        </div>
      </Drawer>

      {confirming && selected && projected !== null && (
        <ConfirmDialog
          open
          title={direction === "add" ? t("awardPoints") : t("redeemPoints")}
          message={`${selected.name}: ${balance} → ${projected}. ${t("loyaltyRecorded")}`}
          confirmLabel={direction === "add" ? t("awardPoints") : t("redeemPoints")}
          cancelLabel={t("cancel")}
          onConfirm={() => void applyLoyalty()}
          onCancel={() => setConfirming(false)}
        />
      )}
    </>
  );
}
