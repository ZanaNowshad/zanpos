import { useEffect, useState } from "react";
import type { BusinessFlags, TaxRuleRow } from "../../../types";
import { businessFlagsLoad, businessFlagsSave, adminListTaxRules, adminSaveTaxRule, adminDeleteTaxRule } from "../../../tauri/commands";
import BusinessTab from "../../../components/settings/BusinessTab";
import type { SessionToken } from "../../../types";

interface Props { sessionToken: SessionToken; onStartPractice?: () => void; }

export default function BusinessRulesPage({ sessionToken, onStartPractice }: Props) {
  const [flags, setFlags] = useState<BusinessFlags>({ allow_negative_stock: false, require_discount_reason: true, cashier_can_discount: false, auto_print_receipt: false });
  const [taxRules, setTaxRules] = useState<TaxRuleRow[]>([]);
  const [editingRule, setEditingRule] = useState<Partial<TaxRuleRow> & { rate_basis_points?: number } | null>(null);
  const [savingFlags, setSavingFlags] = useState(false); const [savedFlags, setSavedFlags] = useState(false);
  const [flagsError, setFlagsError] = useState<string | null>(null);
  const [savingRule, setSavingRule] = useState(false);
  const [taxRuleError, setTaxRuleError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      businessFlagsLoad().catch(() => ({ allow_negative_stock: false, require_discount_reason: true, cashier_can_discount: false, auto_print_receipt: false }) as BusinessFlags),
      adminListTaxRules(sessionToken).catch(() => [] as TaxRuleRow[]),
    ])
      .then(([f, rules]) => { setFlags(f); setTaxRules(rules); })
      .finally(() => setLoading(false));
  }, [sessionToken]);

  const handleSaveFlags = async () => {
    setSavingFlags(true); setFlagsError(null);
    try { await businessFlagsSave(flags, sessionToken); setSavedFlags(true); setTimeout(() => setSavedFlags(false), 3000); }
    catch (e: unknown) { setFlagsError(typeof e === "string" ? e : "Failed to save"); }
    finally { setSavingFlags(false); }
  };

  const handleSaveTaxRule = async () => {
    if (!editingRule || !(editingRule.name ?? "").trim()) { setTaxRuleError("Name is required"); return; }
    setSavingRule(true); setTaxRuleError(null);
    try {
      const saved = await adminSaveTaxRule({
        tax_rule_id: editingRule.tax_rule_id, name: (editingRule.name ?? "").trim(),
        rate_basis_points: editingRule.rate_basis_points ?? 0,
        inclusive: editingRule.inclusive ?? false, is_active: editingRule.is_active ?? true,
      }, sessionToken);
      setTaxRules(prev => { const i = prev.findIndex(r => r.tax_rule_id === saved.tax_rule_id); if (i >= 0) { const n = [...prev]; n[i] = saved; return n; } return [...prev, saved]; });
      setEditingRule(null);
    } catch (e: unknown) { setTaxRuleError(typeof e === "string" ? e : "Failed to save"); }
    finally { setSavingRule(false); }
  };

  const handleDeleteTaxRule = async (id: string) => {
    setSavingRule(true);
    try { await adminDeleteTaxRule(id, sessionToken); setTaxRules(prev => prev.filter(r => r.tax_rule_id !== id)); setEditingRule(null); }
    finally { setSavingRule(false); }
  };

  if (loading) return <div className="bo-empty">Loading business rules…</div>;
  return (
    <BusinessTab flags={flags} setFlags={setFlags} taxRules={taxRules}
      editingRule={editingRule} setEditingRule={setEditingRule}
      taxRuleError={taxRuleError} setTaxRuleError={setTaxRuleError}
      savingFlags={savingFlags} savedFlags={savedFlags} flagsError={flagsError}
      savingRule={savingRule}
      handleSaveFlags={handleSaveFlags} handleSaveTaxRule={handleSaveTaxRule}
      handleDeleteTaxRule={handleDeleteTaxRule} sessionToken={sessionToken} onStartPractice={onStartPractice} />
  );
}
