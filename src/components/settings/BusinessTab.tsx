import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { BusinessFlags, OperationalSettings, SessionToken, TaxRuleRow } from "../../types";
import type { ReportsConfig } from "../../tauri/commands";
import { reportsConfigLoad, reportsConfigSave, operationalSettingsLoad, operationalSettingsSave } from "../../tauri/commands";

interface BusinessTabProps {
  flags: BusinessFlags; setFlags: Dispatch<SetStateAction<BusinessFlags>>;
  taxRules: TaxRuleRow[];
  editingRule: (Partial<TaxRuleRow> & { rate_basis_points?: number }) | null;
  setEditingRule: Dispatch<SetStateAction<(Partial<TaxRuleRow> & { rate_basis_points?: number }) | null>>;
  taxRuleError: string | null; setTaxRuleError: Dispatch<SetStateAction<string | null>>;
  savingFlags: boolean; savedFlags: boolean; flagsError: string | null;
  savingRule: boolean;
  handleSaveFlags: () => void; handleSaveTaxRule: () => void;
  handleDeleteTaxRule: (tax_rule_id: string) => void;
  sessionUserId: string;
  sessionToken: SessionToken;
  onStartPractice?: () => void;
}

export default function BusinessTab(props: BusinessTabProps) {
  const {
    flags, setFlags, taxRules,
    editingRule, setEditingRule, taxRuleError, setTaxRuleError,
    savingFlags, savedFlags, flagsError, savingRule,
    handleSaveFlags, handleSaveTaxRule, handleDeleteTaxRule, sessionUserId, sessionToken, onStartPractice,
  } = props;

  const [reportsCfg, setReportsCfg]       = useState<ReportsConfig | null>(null);
  const [savingScope, setSavingScope]     = useState(false);
  const [savedScope, setSavedScope]       = useState(false);
  const [scopeError, setScopeError]       = useState<string | null>(null);

  useEffect(() => {
    reportsConfigLoad(sessionToken)
      .then(setReportsCfg)
      .catch(() => setReportsCfg({ device_scope: "origin", device_count: 1, local_device_id: "" }));
  }, [sessionToken]);

  const [opSettings, setOpSettings] = useState<OperationalSettings | null>(null);
  const [opSaving, setOpSaving] = useState(false);
  const [opSaved, setOpSaved] = useState(false);
  const [opError, setOpError] = useState<string | null>(null);

  useEffect(() => {
    operationalSettingsLoad()
      .then(setOpSettings)
      .catch(() => {});
  }, []);

  const handleSaveOpSettings = async () => {
    if (!opSettings) return;
    setOpSaving(true); setOpError(null); setOpSaved(false);
    try {
      await operationalSettingsSave(opSettings, sessionUserId);
      setOpSaved(true);
      setTimeout(() => setOpSaved(false), 2000);
    } catch (e: unknown) { setOpError(String(e)); }
    finally { setOpSaving(false); }
  };

  const handleSaveScope = async () => {
    if (!reportsCfg) return;
    setSavingScope(true);
    setScopeError(null);
    setSavedScope(false);
    try {
      await reportsConfigSave(reportsCfg.device_scope, sessionToken);
      setSavedScope(true);
      setTimeout(() => setSavedScope(false), 3000);
    } catch (e: unknown) {
      setScopeError(typeof e === "string" ? e : "Failed to save reports scope");
    } finally {
      setSavingScope(false);
    }
  };

  return (
    <div className="settings-page">
      {onStartPractice && (
        <section className="practice-settings-card">
          <div>
            <span className="practice-settings-kicker">Training workspace</span>
            <h3 className="settings-page-title">Practice mode</h3>
            <p className="settings-hint">Open a safe practice till. Practice sales are clearly marked and never affect stock, revenue, or reports.</p>
          </div>
          <button type="button" className="btn-primary" onClick={onStartPractice}>Start practice sale</button>
        </section>
      )}

      <section>
        <h3 className="settings-page-title">Business Rules</h3>
        <p className="settings-hint">
          Control how the POS behaves at the counter. Changes take effect immediately on the next action.
        </p>

        <div className="biz-flag-list">
          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">Allow selling when out of stock</div>
              <div className="biz-flag-hint">
                Sales proceed even if stock quantity is zero or negative.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-1" type="checkbox" checked={flags.allow_negative_stock}
                onChange={e => setFlags(f => ({ ...f, allow_negative_stock: e.target.checked }))} />
              <span className="biz-toggle-track" />
            </label>
          </div>

          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">Require a reason for every discount</div>
              <div className="biz-flag-hint">
                When ON, cashier must type a reason before applying any discount.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-2" type="checkbox" checked={flags.require_discount_reason}
                onChange={e => setFlags(f => ({ ...f, require_discount_reason: e.target.checked }))} />
              <span className="biz-toggle-track" />
            </label>
          </div>

          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">Allow cashiers to apply discounts</div>
              <div className="biz-flag-hint">
                When OFF, only managers and owners can apply discounts.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-3" type="checkbox" checked={flags.cashier_can_discount}
                onChange={e => setFlags(f => ({ ...f, cashier_can_discount: e.target.checked }))} />
              <span className="biz-toggle-track" />
            </label>
          </div>

          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">Auto-print receipt after every sale</div>
              <div className="biz-flag-hint">
                Automatically sends the receipt to the thermal printer after payment.
                Requires a thermal printer configured in the Printers section.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-4" type="checkbox" checked={flags.auto_print_receipt}
                onChange={e => setFlags(f => ({ ...f, auto_print_receipt: e.target.checked }))} />
              <span className="biz-toggle-track" />
            </label>
          </div>
        </div>
      </section>

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">Reports Scope</h3>
        <p className="settings-hint">
          Cross-device sync is active. Choose whether reports show only this
          device's transactions or the whole store.
          {reportsCfg && reportsCfg.device_count > 1 && (
            <> &nbsp;Currently <strong>{reportsCfg.device_count}</strong> device{reportsCfg.device_count !== 1 ? "s" : ""} registered.</>
          )}
        </p>

        <div className="biz-flag-list">
          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">This device only</div>
              <div className="biz-flag-hint">
                Reports show only sales, refunds, and shifts created on this
                terminal. Default for cashier accounts.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-5" type="radio" name="reports-scope"
                checked={reportsCfg?.device_scope === "origin"}
                onChange={() => setReportsCfg(c => c ? { ...c, device_scope: "origin" } : c)} />
              <span className="biz-toggle-track" />
            </label>
          </div>

          <div className="biz-flag-row">
            <div className="biz-flag-info">
              <div className="biz-flag-label">All devices</div>
              <div className="biz-flag-hint">
                Reports show the union of every registered device's data. Use
                this on the manager dashboard to see the whole store in one
                view.
              </div>
            </div>
            <label className="biz-toggle">
              <input id="a11y-input-6" type="radio" name="reports-scope"
                checked={reportsCfg?.device_scope === "all"}
                onChange={() => setReportsCfg(c => c ? { ...c, device_scope: "all" } : c)} />
              <span className="biz-toggle-track" />
            </label>
          </div>
        </div>

        <div className="settings-page-actions" style={{ marginTop: "8px" }}>
          {scopeError && <span className="settings-action-msg settings-action-err">{scopeError}</span>}
          {savedScope && <span className="settings-action-msg">✓ Saved</span>}
          <button className="btn-primary btn-sm" onClick={handleSaveScope} disabled={savingScope || !reportsCfg}>
            {savingScope ? "Saving…" : "Save Scope"}
          </button>
        </div>
      </section>

      <hr className="settings-page-divider" />

      {opSettings && (
        <>
          <section>
            <h3 className="settings-page-title">Loyalty</h3>
            <p className="settings-hint">
              Configure how loyalty points are earned. Set the number of points a customer receives for every 1 BHD spent.
            </p>
            <div className="ai-params-grid">
              <div className="ai-param-row">
                <span>Points per 1 BHD</span>
                <input id="a11y-input-7" type="number" className="field-input" min={0} value={opSettings.loyalty_points_per_bhd}
                  onChange={e => setOpSettings(s => s ? { ...s, loyalty_points_per_bhd: Number(e.target.value) } : s)} />
              </div>
            </div>
          </section>

          <hr className="settings-page-divider" />

          <section>
            <h3 className="settings-page-title">Data Retention</h3>
            <p className="settings-hint">
              Auto-delete synced data older than the configured number of days. Lower values save disk space; higher values keep more history for reports.
            </p>
            <div className="ai-params-grid">
              <div className="ai-param-row">
                <span>Keep sales (days)</span>
                <input id="a11y-input-8" type="number" className="field-input" min={1} value={opSettings.retention_days_sales}
                  onChange={e => setOpSettings(s => s ? { ...s, retention_days_sales: Number(e.target.value) } : s)} />
              </div>
              <div className="ai-param-row">
                <span>Keep logs (days)</span>
                <input id="a11y-input-9" type="number" className="field-input" min={1} value={opSettings.retention_days_logs}
                  onChange={e => setOpSettings(s => s ? { ...s, retention_days_logs: Number(e.target.value) } : s)} />
              </div>
            </div>
          </section>

          <hr className="settings-page-divider" />

          <section>
            <h3 className="settings-page-title">Sync Intervals</h3>
            <p className="settings-hint">
              How often the app syncs data. Terminal mode syncs frequently for real-time stock; hub mode runs housekeeping less often. Changes take effect on the next sync cycle.
            </p>
            <div className="ai-params-grid">
              <div className="ai-param-row">
                <span>Terminal sync (seconds)</span>
                <input id="a11y-input-10" type="number" className="field-input" min={2} value={opSettings.sync_interval_terminal_secs}
                  onChange={e => setOpSettings(s => s ? { ...s, sync_interval_terminal_secs: Number(e.target.value) } : s)} />
              </div>
              <div className="ai-param-row">
                <span>Hub sync (seconds)</span>
                <input id="a11y-input-11" type="number" className="field-input" min={10} value={opSettings.sync_interval_hub_secs}
                  onChange={e => setOpSettings(s => s ? { ...s, sync_interval_hub_secs: Number(e.target.value) } : s)} />
              </div>
            </div>
          </section>

          <div className="settings-page-actions">
            {opError && <span className="settings-action-msg settings-action-err">{opError}</span>}
            {opSaved && <span className="settings-action-msg">✓ Saved</span>}
            <button className="btn-primary" onClick={handleSaveOpSettings} disabled={opSaving}>
              {opSaving ? "Saving…" : "Save Operational Settings"}
            </button>
          </div>
        </>
      )}

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">Tax Rules</h3>
        <p className="settings-hint">
          Define tax rates applied to products.
          <strong> Inclusive</strong> — price already contains tax (extracted at checkout).
          <strong> Exclusive</strong> — tax added on top of the listed price.
        </p>

        <div className="tax-rules-list">
          {taxRules.length === 0 && (
            <div className="tax-rules-empty">No tax rules defined yet. Click "+ Add Tax Rule" to create one.</div>
          )}
          {taxRules.map(rule => (
            <div key={rule.tax_rule_id} className={`tax-rule-row${!rule.is_active ? " tax-rule-inactive" : ""}`}>
              <div className="tax-rule-info">
                <span className="tax-rule-name">{rule.name}</span>
                <span className="tax-rule-meta">
                  {(rule.rate_basis_points / 100).toFixed(2)}%
                  &nbsp;·&nbsp;
                  <span className={`tax-rule-badge ${rule.inclusive ? "tax-badge-inc" : "tax-badge-exc"}`}>
                    {rule.inclusive ? "Inclusive" : "Exclusive"}
                  </span>
                  {!rule.is_active && <span className="tax-rule-badge tax-badge-off">Inactive</span>}
                </span>
              </div>
              <div className="tax-rule-row-actions">
                <button className="btn-secondary btn-sm"
                  onClick={() => setEditingRule({ ...rule, rate_basis_points: rule.rate_basis_points })} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
                  Edit
                </button>
                <button className="btn-danger btn-sm"
                  onClick={() => handleDeleteTaxRule(rule.tax_rule_id)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>

        <button className="btn-secondary btn-sm" style={{ marginTop: "12px" }}
          onClick={() => setEditingRule({ name: "", rate_basis_points: 0, inclusive: false, is_active: true })} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
          + Add Tax Rule
        </button>

        {editingRule && (
          <div className="tax-rule-editor">
            <h4 className="tax-rule-editor-title">
              {editingRule.tax_rule_id ? "Edit Tax Rule" : "New Tax Rule"}
            </h4>

            <div className="bo-label">Name</div>
            <input id="a11y-input-12" className="bo-input" value={editingRule.name ?? ""}
              onChange={e => setEditingRule(r => r ? { ...r, name: e.target.value } : r)}
              placeholder="e.g. VAT 10%" maxLength={60} />

            <div className="bo-label">Rate (%)</div>
            <input id="a11y-input-13" className="bo-input" type="number" min={0} max={100} step={0.001}
              value={editingRule.rate_basis_points !== undefined ? editingRule.rate_basis_points / 100 : 0}
              onChange={e => setEditingRule(r => r ? { ...r, rate_basis_points: Math.round(parseFloat(e.target.value || "0") * 100) } : r)}
              placeholder="e.g. 10 for 10%" />

            <span className="bo-label">Tax Method</span>
            <div className="tax-method-toggle">
              <button className={`tax-method-btn${!editingRule.inclusive ? " active" : ""}`}
                onClick={() => setEditingRule(r => r ? { ...r, inclusive: false } : r)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
                Exclusive
                <span className="tax-method-hint">Tax added on top of price</span>
              </button>
              <button className={`tax-method-btn${editingRule.inclusive ? " active" : ""}`}
                onClick={() => setEditingRule(r => r ? { ...r, inclusive: true } : r)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
                Inclusive
                <span className="tax-method-hint">Price already includes tax</span>
              </button>
            </div>

            <div className="biz-flag-row" style={{ marginTop: "12px" }}>
              <div className="biz-flag-info">
                <div className="biz-flag-label">Active</div>
                <div className="biz-flag-hint">Inactive rules are hidden in the product editor.</div>
              </div>
              <label className="biz-toggle">
                <input id="a11y-input-14" type="checkbox" checked={editingRule.is_active ?? true}
                  onChange={e => setEditingRule(r => r ? { ...r, is_active: e.target.checked } : r)} />
                <span className="biz-toggle-track" />
              </label>
            </div>

            {taxRuleError && <div className="modal-error" style={{ marginTop: "8px" }}>{taxRuleError}</div>}

            <div className="tax-rule-editor-actions">
              <button className="btn-secondary btn-sm"
                onClick={() => { setEditingRule(null); setTaxRuleError(null); }} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}>
                Cancel
              </button>
              <button className="btn-primary btn-sm" onClick={handleSaveTaxRule} disabled={savingRule}>
                {savingRule ? "Saving…" : "Save Rule"}
              </button>
            </div>
          </div>
        )}
      </section>

      <div className="settings-page-actions">
        {flagsError && <span className="settings-action-msg settings-action-err">{flagsError}</span>}
        {savedFlags && <span className="settings-action-msg">✓ Saved</span>}
        <button className="btn-primary" onClick={handleSaveFlags} disabled={savingFlags}>
          {savingFlags ? "Saving…" : "Save Rules"}
        </button>
      </div>
    </div>
  );
}
