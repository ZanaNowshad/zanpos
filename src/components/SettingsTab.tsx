import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useLanguage } from "../hooks/useLanguage";
import { backOfficeTranslator } from "../i18n/backOfficeStrings";
import type { BranchSettings, BusinessFlags, TaxRuleRow, ThermalConfig } from "../types";
import {
  settingsGetBranch,
  settingsUpdateBranch,
  appConfigGetTimeout,
  appConfigSetTimeout,
  dbBackup,
  thermalListPorts,
  thermalGetConfig,
  thermalSetConfig,
  thermalPrintTest,
  checkForUpdates,
  businessFlagsLoad,
  businessFlagsSave,
  adminListTaxRules,
  adminSaveTaxRule,
  adminDeleteTaxRule,
} from "../tauri/commands";
import { IcoStore, IcoReceipt, IcoRules, IcoPrinter, IcoSystem } from "./settings/Icons";
import StoreTab from "./settings/StoreTab";
import ReceiptTab from "./settings/ReceiptTab";
import BusinessTab from "./settings/BusinessTab";
import PrinterTab from "./settings/PrinterTab";
import SystemControlTab from "./settings/SystemControlTab";
import StorefrontManagementTab from "./settings/StorefrontManagementTab";

const TIMEZONES = [
  "Asia/Bahrain", "Asia/Riyadh", "Asia/Dubai", "Asia/Kuwait", "Asia/Muscat",
  "Asia/Qatar", "Africa/Cairo", "Europe/London", "America/New_York",
  "America/Los_Angeles", "Asia/Singapore",
];

const TIMEOUT_OPTIONS = [0, 1, 2, 5, 10, 15, 30, 60];

function timeoutLabel(m: number) {
  if (m === 0) return "Off — Never lock";
  return `${m} minute${m !== 1 ? "s" : ""}`;
}

interface Props {
  sessionUserId: string;
  sessionToken: string;
  sessionRole: string;
  initialMaintenancePane?: boolean;
}

type SettingsSubTab = "store" | "receipt" | "business" | "printer" | "storefront" | "system_control";

export default function SettingsTab({
  sessionUserId,
  sessionToken,
  sessionRole,
  initialMaintenancePane = false,
}: Props) {
  const [settingsSubTab, setSettingsSubTab] = useState<SettingsSubTab>(
    initialMaintenancePane ? "system_control" : "store",
  );
  const { language } = useLanguage();
  const t = useMemo(() => backOfficeTranslator(language), [language]);
  const [settings, setSettings] = useState<BranchSettings | null>(null);
  const [loading, setLoading]   = useState(true);
  const [appVersion, setAppVersion] = useState("1.0.2");
  const timerRefs = useRef<ReturnType<typeof setTimeout>[]>([]);
  const registerTimer = useCallback((t: ReturnType<typeof setTimeout>) => {
    timerRefs.current.push(t);
  }, []);
  useEffect(() => {
    return () => { timerRefs.current.forEach(clearTimeout); timerRefs.current = []; };
  }, []);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const [name, setName]                   = useState("");
  const [timezone, setTimezone]           = useState("Asia/Bahrain");
  const [address, setAddress]             = useState("");
  const [phone, setPhone]                 = useState("");
  const [taxNumber, setTaxNumber]         = useState("");
  const [crNumber, setCrNumber]           = useState("");
  const [receiptHeader, setReceiptHeader] = useState("");
  const [receiptFooter, setReceiptFooter] = useState("");
  const [saving, setSaving]   = useState(false);
  const [savedStore, setSavedStore]     = useState(false);
  const [savedReceipt, setSavedReceipt]     = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [timeoutMinutes, setTimeoutMinutes] = useState(5);
  const [savingTimeout, setSavingTimeout]   = useState(false);
  const [savedTimeout, setSavedTimeout]     = useState(false);
  const [timeoutError, setTimeoutError]     = useState<string | null>(null);

  const [backingUp, setBackingUp] = useState(false);
  const [backupMsg, setBackupMsg] = useState<string | null>(null);

  const [thermal, setThermal]               = useState<ThermalConfig>({ enabled: false, port: "", baud: "9600" });
  const [savingThermal, setSavingThermal]   = useState(false);
  const [savedThermal, setSavedThermal]     = useState(false);
  const [testingPrint, setTestingPrint]     = useState(false);
  const [printTestMsg, setPrintTestMsg]     = useState<string | null>(null);
  const [availablePorts, setAvailablePorts] = useState<import("../tauri/commands").PortEntry[]>([]);
  const [portsLoading, setPortsLoading]     = useState(false);

  const [flags, setFlags]         = useState<BusinessFlags>({
    allow_negative_stock: false,
    require_discount_reason: true,
    cashier_can_discount: false,
    auto_print_receipt: false,
  });
  const [savingFlags, setSavingFlags] = useState(false);
  const [savedFlags, setSavedFlags]   = useState(false);
  const [flagsError, setFlagsError]   = useState<string | null>(null);

  const [taxRules, setTaxRules]         = useState<TaxRuleRow[]>([]);
  const [editingRule, setEditingRule]   = useState<Partial<TaxRuleRow> & { rate_basis_points?: number } | null>(null);
  const [savingRule, setSavingRule]     = useState(false);
  const [taxRuleError, setTaxRuleError] = useState<string | null>(null);

  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMsg, setUpdateMsg]           = useState<string | null>(null);

  const loadPorts = useCallback(async () => {
    setPortsLoading(true);
    try {
      const ports = await thermalListPorts();
      setAvailablePorts(ports);
      setThermal(prev => {
        if (prev.port === "") {
          const def = ports.find(p => p.is_default) ?? ports[0];
          return def ? { ...prev, port: def.port } : prev;
        }
        return prev;
      });
    } finally {
      setPortsLoading(false);
    }
  }, []);

  useEffect(() => {
    Promise.all([
      settingsGetBranch(sessionUserId),
      appConfigGetTimeout().catch(() => 5),
      thermalGetConfig(sessionUserId).catch(() => ({ enabled: false, port: "", baud: "9600" })),
      businessFlagsLoad().catch(() => ({
        allow_negative_stock: false,
        require_discount_reason: true,
        cashier_can_discount: false,
        auto_print_receipt: false,
      })),
      adminListTaxRules(sessionUserId).catch(() => [] as TaxRuleRow[]),
    ])
      .then(([s, minutes, tc, bf, rules]) => {
        const branch = s as BranchSettings;
        setSettings(branch);
        setName(branch.name);
        setTimezone(branch.timezone);
        setAddress(branch.address ?? "");
        setPhone(branch.phone ?? "");
        setTaxNumber(branch.tax_number ?? "");
        setCrNumber(branch.cr_number ?? "");
        setReceiptHeader(branch.receipt_header ?? "");
        setReceiptFooter(branch.receipt_footer ?? "");
        setTimeoutMinutes(minutes as number);
        setThermal(tc as ThermalConfig);
        setFlags(bf as BusinessFlags);
        setTaxRules(rules as TaxRuleRow[]);
        loadPorts();
      })
      .catch(() => setSaveError("Failed to load settings"))
      .finally(() => setLoading(false));
  }, [loadPorts, sessionUserId]);

  const handleSave = async () => {
    if (!name.trim()) { setSaveError("Store name is required"); return; }
    setSaving(true);
    setSaveError(null);
    try {
      const updated = await settingsUpdateBranch({
        name:           name.trim(),
        timezone,
        address:        address.trim() || undefined,
        phone:          phone.trim() || undefined,
        tax_number:     taxNumber.trim() || undefined,
        cr_number:      crNumber.trim() || undefined,
        receipt_header: receiptHeader.trim() || undefined,
        receipt_footer: receiptFooter.trim() || undefined,
        actor_user_id:  sessionUserId,
      });
      setSettings(updated);
      setSavedStore(true);
      setSavedReceipt(true);
      registerTimer(setTimeout(() => { setSavedStore(false); setSavedReceipt(false); }, 3000));
    } catch (e: unknown) {
      setSaveError(typeof e === "string" ? e : "Failed to save settings");
    } finally {
      setSaving(false);
    }
  };

  const handleSaveTimeout = async () => {
    setSavingTimeout(true);
    setTimeoutError(null);
    try {
      await appConfigSetTimeout(timeoutMinutes, sessionUserId);
      setSavedTimeout(true);
      registerTimer(setTimeout(() => setSavedTimeout(false), 3000));
    } catch (e: unknown) {
      setTimeoutError(typeof e === "string" ? e : "Failed to save timeout");
    } finally {
      setSavingTimeout(false);
    }
  };

  const handleSaveFlags = async () => {
    setSavingFlags(true);
    setFlagsError(null);
    setSavedFlags(false);
    try {
      await businessFlagsSave(flags, sessionUserId);
      setSavedFlags(true);
      registerTimer(setTimeout(() => setSavedFlags(false), 3000));
    } catch (e: unknown) {
      setFlagsError(typeof e === "string" ? e : "Failed to save business rules");
    } finally {
      setSavingFlags(false);
    }
  };

  const handleSaveThermal = async () => {
    setSavingThermal(true);
    setPrintTestMsg(null);
    try {
      await thermalSetConfig(sessionUserId, thermal);
      setSavedThermal(true);
      registerTimer(setTimeout(() => setSavedThermal(false), 3000));
    } catch (e: unknown) {
      setPrintTestMsg(typeof e === "string" ? e : "Failed to save printer settings");
    } finally {
      setSavingThermal(false);
    }
  };

  const handleTestPrint = async () => {
    setTestingPrint(true);
    setPrintTestMsg(null);
    try {
      const msg = await thermalPrintTest(sessionUserId);
      setPrintTestMsg(msg);
    } catch (e: unknown) {
      setPrintTestMsg(typeof e === "string" ? e : "Test failed");
    } finally {
      setTestingPrint(false);
    }
  };

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateMsg(null);
    try {
      const version = await checkForUpdates();
      setUpdateMsg(version
        ? `Version ${version} is available — restart to install.`
        : "You are up to date.");
    } catch (e: unknown) {
      setUpdateMsg(typeof e === "string" ? e : "Update check failed");
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleBackup = async () => {
    setBackupMsg(null);
    setBackingUp(true);
    try {
      const savedTo = await dbBackup("", sessionUserId);
      setBackupMsg(`Backup saved: ${savedTo}`);
    } catch (e: unknown) {
      setBackupMsg(typeof e === "string" ? e : "Backup failed");
    } finally {
      setBackingUp(false);
    }
  };

  const handleSaveTaxRule = async () => {
    if (!editingRule) return;
    const ruleName = (editingRule.name ?? "").trim();
    if (!ruleName) { setTaxRuleError("Name is required"); return; }
    const bp = editingRule.rate_basis_points ?? 0;
    if (bp < 0 || bp > 10000) { setTaxRuleError("Rate must be 0–100%"); return; }
    setSavingRule(true);
    setTaxRuleError(null);
    try {
      const saved = await adminSaveTaxRule({
        tax_rule_id: editingRule.tax_rule_id,
        name: ruleName,
        rate_basis_points: bp,
        inclusive: editingRule.inclusive ?? false,
        is_active: editingRule.is_active ?? true,
        actor_user_id: sessionUserId,
      });
      setTaxRules(prev => {
        const idx = prev.findIndex(r => r.tax_rule_id === saved.tax_rule_id);
        if (idx >= 0) { const next = [...prev]; next[idx] = saved; return next; }
        return [...prev, saved];
      });
      setEditingRule(null);
    } catch (e: unknown) {
      setTaxRuleError(typeof e === "string" ? e : "Failed to save tax rule");
    } finally {
      setSavingRule(false);
    }
  };

  const handleDeleteTaxRule = async (tax_rule_id: string) => {
    setSavingRule(true);
    setTaxRuleError(null);
    try {
      await adminDeleteTaxRule(tax_rule_id, sessionUserId);
      setTaxRules(prev => prev.filter(r => r.tax_rule_id !== tax_rule_id));
      setEditingRule(null);
    } catch (e: unknown) {
      setTaxRuleError(typeof e === "string" ? e : "Failed to delete tax rule");
    } finally {
      setSavingRule(false);
    }
  };

  if (loading) return <div className="bo-empty">Loading settings…</div>;

  return (
    <div className="settings-root">
      {settings && (
        <div className="settings-meta-bar">
          <span className="settings-meta-chip">
            <span className="settings-meta-key">Branch</span>
            <code>{settings.branch_code}</code>
          </span>
          <span className="settings-meta-chip">
            <span className="settings-meta-key">Currency</span>
            <strong>{settings.currency}</strong>
          </span>
          <span className="settings-meta-chip settings-meta-chip-muted">
            <span className="settings-meta-key">ID</span>
            <code>{settings.branch_id.slice(0, 8)}…</code>
          </span>
        </div>
      )}

      <div className="settings-sub-tabs" role="tablist" aria-label="Settings sections">
        <button role="tab" aria-selected={settingsSubTab === 'store'}
          className={`settings-sub-tab${settingsSubTab === 'store' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('store')}>
          <IcoStore /> {t("store")}
        </button>
        <button role="tab" aria-selected={settingsSubTab === 'receipt'}
          className={`settings-sub-tab${settingsSubTab === 'receipt' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('receipt')}>
          <IcoReceipt /> {t("receipt")}
        </button>
        <button role="tab" aria-selected={settingsSubTab === 'business'}
          className={`settings-sub-tab${settingsSubTab === 'business' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('business')}>
          <IcoRules /> {t("business")}
        </button>
        <button role="tab" aria-selected={settingsSubTab === 'printer'}
          className={`settings-sub-tab${settingsSubTab === 'printer' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('printer')}>
          <IcoPrinter /> {t("printer")}
        </button>
        <button role="tab" aria-selected={settingsSubTab === 'storefront'}
          className={`settings-sub-tab${settingsSubTab === 'storefront' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('storefront')}>
          <IcoStore /> ZanShop
        </button>
        <button role="tab" aria-selected={settingsSubTab === 'system_control'}
          className={`settings-sub-tab${settingsSubTab === 'system_control' ? ' active' : ''}`}
          onClick={() => setSettingsSubTab('system_control')}>
          <IcoSystem /> {t("systemControl")}
        </button>
      </div>

      {settingsSubTab === 'store' && (
        <StoreTab
          name={name} setName={setName}
          timezone={timezone} setTimezone={setTimezone}
          address={address} setAddress={setAddress}
          phone={phone} setPhone={setPhone}
          taxNumber={taxNumber} setTaxNumber={setTaxNumber}
          crNumber={crNumber} setCrNumber={setCrNumber}
          setSavedStore={setSavedStore}
          timeoutMinutes={timeoutMinutes} setTimeoutMinutes={setTimeoutMinutes}
          saveError={saveError} savedStore={savedStore} saving={saving}
          timeoutError={timeoutError} savedTimeout={savedTimeout} savingTimeout={savingTimeout}
          handleSave={handleSave} handleSaveTimeout={handleSaveTimeout}
          TIMEZONES={TIMEZONES} TIMEOUT_OPTIONS={TIMEOUT_OPTIONS} timeoutLabel={timeoutLabel}
        />
      )}
      {settingsSubTab === 'receipt' && (
        <ReceiptTab
          receiptHeader={receiptHeader} setReceiptHeader={setReceiptHeader}
          receiptFooter={receiptFooter} setReceiptFooter={setReceiptFooter}
          setSavedReceipt={setSavedReceipt}
          name={name} address={address} phone={phone}
          taxNumber={taxNumber} crNumber={crNumber}
          savedReceipt={savedReceipt} saving={saving}
          handleSave={handleSave}
        />
      )}
      {settingsSubTab === 'business' && (
        <BusinessTab
          flags={flags} setFlags={setFlags}
          taxRules={taxRules}
          editingRule={editingRule} setEditingRule={setEditingRule}
          taxRuleError={taxRuleError} setTaxRuleError={setTaxRuleError}
          savingFlags={savingFlags} savedFlags={savedFlags} flagsError={flagsError}
          savingRule={savingRule}
          handleSaveFlags={handleSaveFlags} handleSaveTaxRule={handleSaveTaxRule}
          handleDeleteTaxRule={handleDeleteTaxRule}
          sessionUserId={sessionUserId}
        />
      )}
      {settingsSubTab === 'printer' && (
        <PrinterTab
          thermal={thermal} setThermal={setThermal}
          availablePorts={availablePorts} portsLoading={portsLoading}
          savingThermal={savingThermal} savedThermal={savedThermal}
          testingPrint={testingPrint} printTestMsg={printTestMsg}
          handleSaveThermal={handleSaveThermal} handleTestPrint={handleTestPrint}
          loadPorts={loadPorts}
        />
      )}
      {settingsSubTab === 'storefront' && (
        <StorefrontManagementTab sessionUserId={sessionUserId} sessionRole={sessionRole} />
      )}
      {settingsSubTab === 'system_control' && (
        <SystemControlTab
          sessionUserId={sessionUserId}
          sessionToken={sessionToken}
          sessionRole={sessionRole}
          appVersion={appVersion}
          backingUp={backingUp} backupMsg={backupMsg}
          checkingUpdate={checkingUpdate} updateMsg={updateMsg}
          handleBackup={handleBackup} handleCheckUpdate={handleCheckUpdate}
          registerTimer={registerTimer}
          initialPane={initialMaintenancePane ? "maintenance" : undefined}
        />
      )}
    </div>
  );
}
