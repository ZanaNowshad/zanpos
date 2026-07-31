import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Check, ChevronDown, ChevronUp, Truck, X } from "lucide-react";
import type { CustomerRow, DeliveryInput, PaymentInput } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import DeliveryForm from "./DeliveryForm";
import { applyDialpadKey } from "./Dialpad";
import { PaymentCommandPanel, PaymentMethodPicker } from "./PaymentExperience";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { usePaymentCustomer } from "../hooks/usePaymentCustomer";
import PaymentCustomerSelector from "./PaymentCustomerSelector";
import { useLanguage } from "../hooks/useLanguage";
import { detailTranslator } from "../i18n/detailStrings";
interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[], customerId?: string, delivery?: DeliveryInput, selectedCustomer?: CustomerRow) => void;
  onCancel: () => void;
  loading?: boolean;
  initialMethod?: PaymentInput["method"];
  splitMode?: boolean;
  sessionUserId?: string;
}

interface PaymentLine {
  id: number;
  method: PaymentInput["method"];
  amountStr: string;
  tenderedStr: string;
}

type ActiveField =
  | { kind: "amount";   lineId: number }
  | { kind: "tendered"; lineId: number }
  | { kind: "phone" }
  | null;

let lineIdCounter = 200;
const mkLine = (method: PaymentInput["method"] = "cash"): PaymentLine =>
  ({ id: lineIdCounter++, method, amountStr: "", tenderedStr: "" });

export default function PaymentModal({
  netTotal, onConfirm, onCancel, loading,
  initialMethod, splitMode, sessionUserId,
}: Props) {
  const { language } = useLanguage();
  const dt = useMemo(() => detailTranslator(language), [language]);
  const EXP = DEVICE.currency_exponent;
  const containerRef = useRef<HTMLDivElement>(null);
  const handleConfirmRef = useRef<() => void>(() => {});
  const freshTenderedEntryRef = useRef(true);
  useFocusTrap(containerRef, onCancel);
  const fmt = (n: number) => `${formatMoney(n, EXP)}`;

  const [lines, setLines] = useState<PaymentLine[]>(() => {
    const first = mkLine(initialMethod ?? "cash");
    if (!splitMode) {
      const s = formatMoney(netTotal, EXP);
      first.amountStr = s;
      if (!initialMethod || initialMethod === "cash") first.tenderedStr = s;
    }
    return splitMode ? [first, mkLine("card")] : [first];
  });

  const [activeField, setActiveField] = useState<ActiveField>(
    () => splitMode
      ? { kind: "amount", lineId: lines[0].id }
      : lines[0].method === "cash"
        ? { kind: "tendered", lineId: lines[0].id }
        : null
  );

  const {
    showCust, custSearch, custResults, selectedCust, showCustDrop,
    toggleCustomer, changeCustomerSearch, blurCustomerSearch, selectCustomer, removeCustomer,
  } = usePaymentCustomer(sessionUserId);

  const [showDelivery, setShowDelivery] = useState(false);
  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});
  const [phoneRaw, setPhoneRaw]       = useState("");
  const [phoneError, setPhoneError]   = useState<string | null>(null);

  const [showSplit, setShowSplit] = useState(splitMode ?? false);

  const updateLine = useCallback((id: number, patch: Partial<PaymentLine>) =>
    setLines(prev => prev.map(l => l.id === id ? { ...l, ...patch } : l)), []);
  const removeLine = (id: number) =>
    setLines(prev => prev.filter(l => l.id !== id));

  const selectMethod = useCallback((method: PaymentLine["method"]) => {
    const exact = formatMoney(netTotal, EXP);
    setLines(prev => prev.map((line, index) => index === 0
      ? {
          ...line,
          method,
          amountStr: showSplit ? line.amountStr : exact,
          tenderedStr: method === "cash" && !showSplit ? exact : line.tenderedStr,
        }
      : line));
    setActiveField(showSplit
      ? { kind: "amount", lineId: lines[0].id }
      : method === "cash"
        ? { kind: "tendered", lineId: lines[0].id }
        : null);
    freshTenderedEntryRef.current = method === "cash";
  }, [EXP, lines, netTotal, showSplit]);

  const allocatedMinor = lines.reduce(
    (s, l) => s + Math.max(parseMoney(l.amountStr, EXP), 0), 0
  );
  const remainingMinor = netTotal - allocatedMinor;

  const handleDialpadKey = useCallback((key: string) => {
    // If a real <input> or <textarea> is focused, write to it directly
    const el = document.activeElement;
    if (el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
      if (key === "⌫") {
        const start = el.selectionStart ?? el.value.length;
        if (start > 0) {
          el.setSelectionRange(start - 1, start);
          // Fire input event so React onChange handlers pick up the change
          el.dispatchEvent(new Event("input", { bubbles: true }));
        }
        // Let the native backspace-delete handle removal via the selection
        // we just set (next keydown will delete the selected char). For immediate
        // deletion we use execCommand which works across all modern browsers:
        document.execCommand("delete", false);
      } else if (key === "C") {
        // Treat "C" as clear: select all then delete
        el.select();
        el.dispatchEvent(new Event("input", { bubbles: true }));
        document.execCommand("delete", false);
      } else if (key === "." || key === "00") {
        // Insert as-is (phone/phone fields may use these)
        const v = key === "00" ? "00" : ".";
        const start = el.selectionStart ?? el.value.length;
        const end = el.selectionEnd ?? el.value.length;
        el.setRangeText(v, start, end, "end");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      } else if (key >= "0" && key <= "9") {
        const start = el.selectionStart ?? el.value.length;
        const end = el.selectionEnd ?? el.value.length;
        el.setRangeText(key, start, end, "end");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }
      return;
    }
    // Fall through to virtual-display-field logic
    if (!activeField) return;
    if (activeField.kind === "amount" || activeField.kind === "tendered") {
      const line = lines.find(l => l.id === activeField.lineId);
      if (!line) return;
      const cur = activeField.kind === "amount" ? line.amountStr : line.tenderedStr;
      const startsFreshTendered = activeField.kind === "tendered" && freshTenderedEntryRef.current;
      const next = applyDialpadKey(startsFreshTendered && key !== "⌫" ? "" : cur, key);
      if (activeField.kind === "amount") updateLine(activeField.lineId, { amountStr: next });
      else {
        freshTenderedEntryRef.current = false;
        updateLine(activeField.lineId, { tenderedStr: next });
      }
    } else if (activeField.kind === "phone") {
      setPhoneRaw(prev => {
        if (key === "⌫") return prev.slice(0, -1);
        if (key === "C")  return "";
        if (key === "." || key === "00") return prev;
        if (prev.length >= 8) return prev;
        return prev + key;
      });
    }
  }, [activeField, lines, updateLine]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const k = e.key.toLowerCase();
      if (e.key >= "0" && e.key <= "9") { e.preventDefault(); handleDialpadKey(e.key); }
      else if (e.key === "Backspace")   { e.preventDefault(); handleDialpadKey("⌫"); }
      else if (e.key === "Delete")      { e.preventDefault(); handleDialpadKey("C"); }
      else if (e.key === ".")           { e.preventDefault(); handleDialpadKey("."); }
      else if (k === "c" || e.key === "F1") { e.preventDefault(); selectMethod("cash"); }
      else if (k === "a" || e.key === "F2") { e.preventDefault(); selectMethod("card"); }
      else if (k === "w" || e.key === "F3") { e.preventDefault(); selectMethod("wallet"); }
      else if (e.key === "F4") { e.preventDefault(); selectMethod("other"); }
      else if (k === "e") {
        e.preventDefault();
        const due = formatMoney(netTotal, EXP);
        setLines(p => p.map((l, i) => i === 0 ? { ...l, amountStr: due, tenderedStr: due } : l));
      }
      else if (e.key === "Escape")      { e.preventDefault(); onCancel(); }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [handleDialpadKey, onCancel, netTotal, EXP, selectMethod]);

  const quickAmts = (() => {
    const unit = Math.pow(10, EXP);
    return [5, 10, 20, 50]
      .map(n => Math.round(n * unit))
      .filter(n => n > netTotal);
  })();

  const applyQuick = (minor: number) => {
    const cashLine = lines.find(l => l.method === "cash");
    if (!cashLine) return;
    const s = formatMoney(minor, EXP);
    updateLine(cashLine.id, { tenderedStr: s });
    freshTenderedEntryRef.current = false;
    setActiveField({ kind: "tendered", lineId: cashLine.id });
  };

  const canConfirm = (() => {
    if (!lines.length) return false;
    for (const l of lines) {
      if (parseMoney(l.amountStr, EXP) <= 0) return false;
      if (l.method === "cash") {
        const t = parseMoney(l.tenderedStr || l.amountStr, EXP);
        if (t < parseMoney(l.amountStr, EXP)) return false;
      }
    }
    if (Math.abs(allocatedMinor - netTotal) > 1) return false;
    if (showDelivery) {
      if (!deliveryData.contact_number) return false;
      if (!deliveryData.house_number?.trim()) return false;
      if (!deliveryData.address_text?.trim()) return false;
    }
    return true;
  })();

  const handleConfirm = () => {
    const payments: PaymentInput[] = lines.map(l => {
      const amount = parseMoney(l.amountStr, EXP);
      if (l.method === "cash") {
        const tendered = Math.max(parseMoney(l.tenderedStr || l.amountStr, EXP), amount);
        return { method: l.method, amount_minor: amount, tendered_minor: tendered };
      }
      return { method: l.method, amount_minor: amount };
    });
    const delivery: DeliveryInput | undefined = showDelivery
      ? { ...(deliveryData as DeliveryInput), customer_id: selectedCust?.customer_id, expected_payment_method: lines[0]?.method ?? "cash" }
      : undefined;
    onConfirm(payments, selectedCust?.customer_id, delivery, selectedCust ?? undefined);
  };
  // Keep ref current so Enter-key handler always calls the latest handleConfirm
  useEffect(() => { handleConfirmRef.current = handleConfirm; });
  useEffect(() => {
    const onEnterKey = (e: KeyboardEvent) => {
      // Only skip if a dropdown/autocomplete is visible (avoid accidental confirm)
      const active = document.activeElement as HTMLElement | null;
      if (active && active.closest(".bo-select-dropdown, [role='listbox'], .customer-drop")) return;
      if (e.key === "Enter" || e.key === "NumpadEnter") {
        e.preventDefault();
        if (canConfirm && !loading) handleConfirmRef.current();
      }
    };
    document.addEventListener("keydown", onEnterKey);
    return () => document.removeEventListener("keydown", onEnterKey);
  }, [canConfirm, loading]);

  const activeLabel =
    activeField?.kind === "tendered" ? dt("tendered") :
    activeField?.kind === "phone" ? dt("phone") : dt("amount");

  const mainLine = lines[0];
  const amt = parseMoney(mainLine?.amountStr ?? "", EXP);
  const tendered = parseMoney(mainLine?.tenderedStr ?? mainLine?.amountStr ?? "", EXP);
  const change = mainLine?.method === "cash" && tendered > amt ? tendered - amt : 0;
  const methodName =
    mainLine?.method === "cash" ? dt("cash") :
    mainLine?.method === "card" ? dt("card") :
    mainLine?.method === "wallet" ? dt("wallet") :
    dt("other");
  const methodHint =
    mainLine?.method === "cash" ? dt("cashGuidance") :
    mainLine?.method === "card" ? dt("cardGuidance") :
    mainLine?.method === "wallet" ? dt("walletGuidance") :
    dt("lockedAmountGuidance");
  const readinessInstruction =
    mainLine?.method === "card" ? dt("confirmCardApproval") :
    mainLine?.method === "wallet" ? dt("confirmWalletReceived") :
    dt("confirmPaymentReceived");
  const showNumericEntry = showSplit || mainLine?.method === "cash" || activeField?.kind === "phone";
  const confirmBlockReason = (() => {
    if (loading) return dt("recordingPayment");
    if (!lines.length || lines.some(line => parseMoney(line.amountStr, EXP) <= 0)) return dt("everyPaymentAmount");
    const shortCash = lines.find(line =>
      line.method === "cash"
      && parseMoney(line.tenderedStr || line.amountStr, EXP) < parseMoney(line.amountStr, EXP)
    );
    if (shortCash) {
      const short = parseMoney(shortCash.amountStr, EXP) - parseMoney(shortCash.tenderedStr || shortCash.amountStr, EXP);
      return `${DEVICE.currency} ${fmt(short)} ${dt("moreCashNeeded")}`;
    }
    if (remainingMinor > 1) return `${DEVICE.currency} ${fmt(remainingMinor)} ${dt("stillDue")}`;
    if (remainingMinor < -1) return `${dt("reducePaymentsBy")} ${DEVICE.currency} ${fmt(-remainingMinor)}.`;
    if (showDelivery && !deliveryData.contact_number) return dt("validDeliveryContact");
    if (showDelivery && !deliveryData.house_number?.trim()) return dt("deliveryBuilding");
    if (showDelivery && !deliveryData.address_text?.trim()) return dt("deliveryAddress");
    return null;
  })();
  const completionLabel = loading
    ? dt("completingSale")
    : language === "en" ? `Complete ${methodName.toLowerCase()} sale · ${DEVICE.currency} ${fmt(netTotal)}`
    : `${dt("completeSale")} (${methodName}) · ${DEVICE.currency} ${fmt(netTotal)}`;

  return (
    <div className="modal-overlay" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className="pm-shell pm-shell-calm" role="dialog" aria-modal="true" aria-labelledby="pm-dialog-title" ref={containerRef}>

        <div className="pm-left">
          <div className="pm-header">
            <div className="pm-header-copy">
              <span className="pm-title">{dt("collectPayment")}</span>
              <h2 id="pm-dialog-title">{dt("amountDue")}</h2>
            </div>
            <span className="pm-total-badge">{DEVICE.currency} {fmt(netTotal)}</span>
            <button className="pm-close-btn" onClick={onCancel} disabled={loading} aria-label={dt("closePayment")}>
              <X size={19} />
            </button>
          </div>

          <div className="pm-step-label"><span>1</span> {dt("choosePaymentMethod")}</div>
          <PaymentMethodPicker selected={mainLine.method} onSelect={selectMethod} />
          <div className="pm-method-guidance">{methodHint}</div>

          <div className="pm-step-label"><span>2</span> {dt("confirmAmount")}</div>
          <div
            className={`pm-amount-box${showSplit ? " pm-amount-editable" : " pm-amount-locked"}${activeField?.kind === "amount" ? " pm-field-active" : ""}`}
            onClick={() => showSplit && setActiveField({ kind: "amount", lineId: mainLine.id })}
          >
            <span className="pm-amount-label">{showSplit ? dt("paymentAmount") : dt("exactSaleAmount")}</span>
            <span className="pm-amount-value">
              {mainLine.amountStr || formatMoney(netTotal, EXP)}
              {activeField?.kind === "amount" && <span className="pm-cursor">|</span>}
            </span>
          </div>

          {mainLine.method === "cash" && (
            <div
              className={`pm-amount-box pm-tendered-box${activeField?.kind === "tendered" ? " pm-field-active" : ""}`}
              onClick={() => setActiveField({ kind: "tendered", lineId: mainLine.id })}
            >
              <span className="pm-amount-label">{dt("cashReceived")}</span>
              <span className="pm-amount-value">
                {mainLine.tenderedStr || mainLine.amountStr || "0"}
                {activeField?.kind === "tendered" && <span className="pm-cursor">|</span>}
              </span>
            </div>
          )}

          {mainLine.method === "cash" && (
            <div className="pm-change-pill" aria-live="polite" aria-atomic="true">
              <span><Check size={16} /> {dt("changeDue")}</span>
              <strong>{DEVICE.currency} {fmt(change)}</strong>
            </div>
          )}

          {mainLine.method === "cash" && (
            <div className="pm-quick">
              <button className="pm-quick-btn pm-quick-exact" onClick={() => applyQuick(netTotal)}>
                {dt("exact")} <kbd>E</kbd>
              </button>
              {quickAmts.slice(0, 3).map(a => (
                <button key={a} className="pm-quick-btn" onClick={() => applyQuick(a)}>
                  {DEVICE.currency} {fmt(a)}
                </button>
              ))}
            </div>
          )}

          {showSplit && lines.length > 1 && (
            <div className="pm-split-section">
              {lines.slice(1).map(line => (
                <div key={line.id} className="pm-split-line">
                  <select
                    className="pm-split-method"
                    value={line.method}
                    onChange={e => updateLine(line.id, { method: e.target.value as PaymentInput["method"] })}
                  >
                    <option value="card">{dt("card")}</option>
                    <option value="cash">{dt("cash")}</option>
                    <option value="wallet">{dt("wallet")}</option>
                    <option value="other">••• {dt("other")}</option>
                  </select>
                  <div
                    className={`pm-amount-box pm-split-amount${activeField?.kind === "amount" && activeField.lineId === line.id ? " pm-field-active" : ""}`}
                    onClick={() => setActiveField({ kind: "amount", lineId: line.id })}
                  >
                    <span className="pm-amount-value" style={{fontSize: "1rem"}}>
                      {line.amountStr || "0"}
                      {activeField?.kind === "amount" && activeField.lineId === line.id && <span className="pm-cursor">|</span>}
                    </span>
                  </div>
                  <button className="pm-split-remove" onClick={() => removeLine(line.id)} aria-label={dt("removeSplitPayment")}>
                    <X size={15} />
                  </button>
                </div>
              ))}
              <div className="pm-split-remaining">
                {dt("remaining")}: {DEVICE.currency} {fmt(remainingMinor)}
              </div>
            </div>
          )}

          {showSplit && (
            <div className={`pm-remaining${remainingMinor < 0 ? " pm-remaining-change" : remainingMinor === 0 ? " pm-remaining-ok" : ""}`}>
              {remainingMinor > 0 ? `${dt("due")}: ${DEVICE.currency} ${fmt(remainingMinor)}` :
               remainingMinor < 0 ? `${dt("changeDue")}: ${DEVICE.currency} ${fmt(-remainingMinor)}` :
               `✓ ${dt("fullyPaid")}`}
            </div>
          )}
        </div>

        <div className="pm-mid">
          <div className="pm-step-label pm-step-muted"><span>3</span> {dt("optionalDetails")}</div>
          <div className="pm-mid-title">{dt("optionalOrderDetails")}</div>

          {/* Split payment toggle */}
          {!showSplit && (
            <button
              className="pm-split-toggle"
              onClick={() => {
                setShowSplit(true);
                const rem = formatMoney(remainingMinor, EXP);
                const newLine = mkLine("card");
                if (remainingMinor > 0) newLine.amountStr = rem;
                setLines(p => [...p, newLine]);
              }}
            >
              {dt("splitPayment")}
            </button>
          )}

          <PaymentCustomerSelector
            selectedCustomer={selectedCust}
            open={showCust}
            search={custSearch}
            results={custResults}
            showResults={showCustDrop}
            onToggle={toggleCustomer}
            onSearchChange={changeCustomerSearch}
            onSearchBlur={blurCustomerSearch}
            onSelect={selectCustomer}
            onRemove={removeCustomer}
          />

          {/* Delivery section */}
          <div className="pm-section">
            <button
              className={`pm-section-hdr${showDelivery ? " pm-section-hdr-open pm-section-hdr-active" : ""}`}
              onClick={() => setShowDelivery(v => !v)}
            >
              <span className="pm-section-title"><Truck size={15} /> {dt("delivery")}</span>
              <span className="pm-chevron">{showDelivery ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</span>
            </button>
            {showDelivery && (
              <div className="pm-section-body pm-delivery-body">
                <DeliveryForm
                  value={deliveryData}
                  onChange={setDeliveryData}
                  selectedCustomer={selectedCust}
                  expectedPaymentMethod={lines[0]?.method ?? "cash"}
                  actorUserId={sessionUserId ?? DEVICE.device_id}
                  phoneRaw={phoneRaw}
                  phoneError={phoneError}
                  onPhoneChange={(raw, normalized, err) => {
                    setPhoneRaw(raw);
                    setPhoneError(err);
                    setDeliveryData(prev => ({ ...prev, contact_number: normalized ?? "" }));
                  }}
                  onPhoneFocus={() => setActiveField({ kind: "phone" })}
                />
              </div>
            )}
          </div>
        </div>

        {/* ── PANEL 3: Numpad + save ──────────────────────────────────── */}
        <PaymentCommandPanel
          showNumericEntry={showNumericEntry}
          showTendered={activeField?.kind === "tendered"}
          activeLabel={activeLabel}
          currency={DEVICE.currency}
          tenderedStr={mainLine.tenderedStr}
          totalLabel={fmt(netTotal)}
          methodName={methodName}
          readinessInstruction={readinessInstruction}
          confirmBlockReason={confirmBlockReason}
          canConfirm={canConfirm}
          loading={loading}
          completionLabel={completionLabel}
          onKey={handleDialpadKey}
          onConfirm={handleConfirm}
          onCancel={onCancel}
        />

      </div>
    </div>
  );
}
