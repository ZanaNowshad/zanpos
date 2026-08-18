import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { Check, MessageCircle, Printer, ReceiptText, Truck, X } from "lucide-react";
import type { CustomerRow, DeliveryInput, PaymentInput, RiderRow } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import DeliveryForm from "./DeliveryForm";
import { normalizePhone } from "./DeliveryForm";
import { applyDialpadKey } from "./Dialpad";
import { typeIntoFocusedField } from "./paymentFieldTyping";
import { buildPaymentInputs, canConfirmPayment, paymentBlockReason } from "./paymentValidation";
import { mkLine, type ActiveField, type PaymentLine } from "./paymentLines";
import { PaymentCommandPanel, PaymentMethodPicker } from "./PaymentExperience";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { usePaymentCustomer } from "../hooks/usePaymentCustomer";
import PaymentContactField from "./PaymentContactField";
import RiderPicker from "./RiderPicker";
import { useLanguage } from "../hooks/useLanguage";
import { detailTranslator } from "../i18n/detailStrings";
import { systemKeyboardOpen } from "../tauri/commands";
export type PaymentJourney = "receipt" | "delivery" | "digital";

export interface PaymentCompletionOptions {
  journey: PaymentJourney;
  printReceipt: boolean;
  whatsappNumber?: string;
  /** Chosen at checkout, if any. Carried here rather than on DeliveryInput so
   *  the phone number needed to message them survives the round trip — the
   *  delivery row stores only the rider's id and name. */
  rider?: RiderRow;
}

interface Props {
  netTotal: number;
  onConfirm: (payments: PaymentInput[], customerId?: string, delivery?: DeliveryInput, selectedCustomer?: CustomerRow, options?: PaymentCompletionOptions) => void;
  onCancel: () => void;
  loading?: boolean;
  initialMethod?: PaymentInput["method"];
  splitMode?: boolean;
  sessionUserId?: string;
  journey?: PaymentJourney;
  defaultPrintReceipt?: boolean;
}



export default function PaymentModal({
  netTotal, onConfirm, onCancel, loading,
  initialMethod, splitMode, sessionUserId, journey = "receipt", defaultPrintReceipt = false,
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
    () => journey !== "receipt"
      ? { kind: "phone" }
      : splitMode
      ? { kind: "amount", lineId: lines[0].id }
      : lines[0].method === "cash"
        ? { kind: "tendered", lineId: lines[0].id }
        : null
  );

  const {
    custResults, selectedCust, showCustDrop,
    changeCustomerSearch, blurCustomerSearch, selectCustomer, removeCustomer,
  } = usePaymentCustomer(sessionUserId);

  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});
  const [phoneRaw, setPhoneRaw]       = useState("");
  const [phoneError, setPhoneError]   = useState<string | null>(null);
  const [printReceipt, setPrintReceipt] = useState(defaultPrintReceipt);
  const requiresContact = journey !== "receipt";
  const isDelivery = journey === "delivery";

  const [showSplit, setShowSplit] = useState(splitMode ?? false);
  const [rider, setRider] = useState<RiderRow | null>(null);
  const [keyboardError, setKeyboardError] = useState<string | null>(null);

  /* Raising the OS keyboard moves focus to it, so the field the cashier was in
     has to be put back — otherwise the keys would go nowhere. */
  const openSystemKeyboard = useCallback(() => {
    const target = document.activeElement;
    setKeyboardError(null);
    systemKeyboardOpen()
      .then(() => {
        if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
          setTimeout(() => target.focus(), 250);
        }
      })
      .catch(() => setKeyboardError(dt("onScreenKeyboardFailed")));
  }, [dt]);

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
    // A focused text field wins over the virtual display fields below.
    if (typeIntoFocusedField(key)) return;
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

  const validationInput = {
    lines, netTotal, allocatedMinor, remainingMinor,
    currencyExponent: EXP, requiresContact, isDelivery, deliveryData, loading,
  };
  const canConfirm = canConfirmPayment(validationInput);

  const handleConfirm = () => {
    const payments = buildPaymentInputs(lines, EXP);
    const delivery: DeliveryInput | undefined = isDelivery
      ? {
          ...(deliveryData as DeliveryInput),
          address_text: deliveryData.address_text ?? "",
          customer_id: selectedCust?.customer_id,
          expected_payment_method: lines[0]?.method ?? "cash",
          rider_id: rider?.rider_id,
          delivery_staff_name: rider?.name ?? deliveryData.delivery_staff_name,
        }
      : undefined;
    onConfirm(payments, selectedCust?.customer_id, delivery, selectedCust ?? undefined, {
      journey,
      printReceipt,
      whatsappNumber: requiresContact ? deliveryData.contact_number : undefined,
      rider: rider ?? undefined,
    });
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
    mainLine?.method === "wallet" ? (journey === "receipt" ? dt("wallet") : "BenefitPay") :
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
  const confirmBlockReason = paymentBlockReason(validationInput, dt, fmt);
  const completionLabel = loading
    ? dt("completingSale")
    : language === "en" ? `Complete ${methodName.toLowerCase()} sale · ${DEVICE.currency} ${fmt(netTotal)}`
    : `${dt("completeSale")} (${methodName}) · ${DEVICE.currency} ${fmt(netTotal)}`;

  return (
    <button className="modal-overlay" type="button" onClick={e => e.target === e.currentTarget && onCancel()}>
      <div className={`pm-shell pm-shell-calm${requiresContact ? " pm-shell-contact" : ""}`} role="dialog" aria-modal="true" aria-labelledby="pm-dialog-title" ref={containerRef}>

        <div className="pm-left">
          <div className="pm-header">
            <div className="pm-header-copy">
              <span className={`pm-journey-tag pm-journey-${journey}`}>
                {journey === "receipt" ? <ReceiptText size={14} /> : journey === "delivery" ? <Truck size={14} /> : <MessageCircle size={14} />}
                {journey === "receipt" ? "Receipt sale" : journey === "delivery" ? "Delivery order" : "Digital receipt"}
              </span>
              <h2 id="pm-dialog-title">{dt("amountDue")}</h2>
            </div>
            <span className="pm-total-badge">{DEVICE.currency} {fmt(netTotal)}</span>
            <button className="pm-close-btn" onClick={onCancel} disabled={loading} aria-label={dt("closePayment")}>
              <X size={19} />
            </button>
          </div>

          <div className="pm-step-label"><span>1</span> {dt("choosePaymentMethod")}</div>
          <PaymentMethodPicker selected={mainLine.method} onSelect={selectMethod} walletLabel={journey === "receipt" ? undefined : "BenefitPay"} />
          <div className="pm-method-guidance">{methodHint}</div>

          <div className="pm-step-label"><span>2</span> {dt("confirmAmount")}</div>
          <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} 
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
            <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} 
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
                  </select>
                  <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }} 
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

        <div className={`pm-mid pm-mid-${journey}`}>
          <div className="pm-step-label pm-step-muted"><span>3</span> {requiresContact ? "Order contact" : "Receipt options"}</div>
          <div className="pm-mid-title">{requiresContact ? (isDelivery ? "Delivery details" : "Digital receipt destination") : "Finish the receipt"}</div>

          {journey === "receipt" && !showSplit && (
            <button
              className="pm-split-toggle"
              onClick={() => {
                setShowSplit(true);
                const rem = formatMoney(remainingMinor, EXP);
                const newLine = mkLine("card");
                if (remainingMinor > 0) newLine.amountStr = rem;
                setLines(previous => [...previous, newLine]);
              }}
            >
              {dt("splitPayment")}
            </button>
          )}

          {requiresContact && (
            <PaymentContactField
              phoneRaw={phoneRaw}
              phoneError={phoneError}
              selectedCustomer={selectedCust}
              suggestions={custResults}
              showSuggestions={showCustDrop}
              sessionUserId={sessionUserId}
              onFocus={() => setActiveField({ kind: "phone" })}
              onPhoneBlur={blurCustomerSearch}
              onPhoneChange={raw => {
                const normalized = normalizePhone(raw);
                setPhoneRaw(raw);
                setPhoneError(raw && !normalized ? "Enter 8 digits" : null);
                setDeliveryData(previous => ({ ...previous, contact_number: normalized ?? "" }));
                changeCustomerSearch(raw);
              }}
              onSelect={customer => {
                const raw = (customer.phone ?? "").replace(/\D/g, "").replace(/^(00)?973/, "").slice(-8);
                const normalized = normalizePhone(raw);
                selectCustomer(customer);
                setPhoneRaw(raw);
                setPhoneError(normalized ? null : "This customer does not have a valid Bahrain mobile number");
                setDeliveryData(previous => ({ ...previous, contact_number: normalized ?? "" }));
              }}
              onClearCustomer={() => {
                removeCustomer();
                setPhoneRaw("");
                setPhoneError(null);
                setDeliveryData(previous => ({ ...previous, contact_number: "" }));
              }}
            />
          )}

          {journey === "digital" && (
            <div className="pm-digital-guide">
              <MessageCircle size={20} />
              <div><strong>Send it straight to WhatsApp</strong><span>Enter a mobile number or choose a saved customer. The receipt is sent after payment.</span></div>
            </div>
          )}

          {isDelivery && (
            <>
              <DeliveryForm value={deliveryData} onChange={setDeliveryData} expectedPaymentMethod={lines[0]?.method ?? "cash"} />
              <RiderPicker
                sessionUserId={sessionUserId}
                selectedId={rider?.rider_id ?? null}
                onSelect={setRider}
              />
            </>
          )}

          <label className="pm-print-option">
            <input type="checkbox" checked={printReceipt} onChange={event => setPrintReceipt(event.target.checked)} />
            <Printer size={17} />
            <span><strong>Print receipt now</strong><small>Print immediately after payment</small></span>
          </label>
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
          keyboardError={keyboardError}
          onKey={handleDialpadKey}
          onOpenKeyboard={openSystemKeyboard}
          onConfirm={handleConfirm}
          onCancel={onCancel}
        />

      </div>
    </button>
  );
}
