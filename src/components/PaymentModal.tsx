import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { MessageCircle, ReceiptText, Truck, X } from "lucide-react";
import type { CustomerRow, DeliveryInput, PaymentInput, RiderRow } from "../types";
import { formatMoney, parseMoney } from "../money";
import { DEVICE } from "../types";
import { buildPaymentInputs, canConfirmPayment, paymentBlockReason, paymentBlockers } from "./paymentValidation";
import { mkLine, type ActiveField, type PaymentLine } from "./paymentLines";
import { PaymentCommandPanel, PaymentMethodPicker } from "./PaymentExperience";
import { useFocusTrap } from "../hooks/useFocusTrap";
import { usePaymentContact } from "../hooks/usePaymentContact";
import PaymentContactPanel from "./PaymentContactPanel";
import { focusFirstField, focusNextField } from "./paymentFieldNav";
import { useLanguage } from "../hooks/useLanguage";
import { detailTranslator } from "../i18n/detailStrings";
import { systemKeyboardOpen } from "../tauri/commands";
import { useFocusedField } from "./PaymentInputSurface";
import { PaymentCashTender, PaymentSplitLines } from "./PaymentCashFields";
import { usePaymentKeyboard } from "./usePaymentKeyboard";
import type { SessionToken } from "../types";
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
  sessionToken?: SessionToken;
  journey?: PaymentJourney;
  defaultPrintReceipt?: boolean;
}



export default function PaymentModal({
  netTotal, onConfirm, onCancel, loading,
  initialMethod, splitMode, sessionToken, journey: initialJourney = "receipt",
  defaultPrintReceipt = false,
}: Props) {
  /* The journey is chosen before the modal opens, but customers change their
     mind at the counter — "actually, can you deliver it?". Switching in place
     keeps whatever has already been typed; making them cancel and start over
     is the kind of friction that gets a POS blamed for a queue. */
  const [journey, setJourney] = useState<PaymentJourney>(initialJourney);
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
    () => initialJourney !== "receipt"
      ? null
      : splitMode
      ? { kind: "amount", lineId: lines[0].id }
      : lines[0].method === "cash"
        ? { kind: "tendered", lineId: lines[0].id }
        : null
  );

  const contact = usePaymentContact();
  const selectedCust = contact.customer;

  const [deliveryData, setDeliveryData] = useState<Partial<DeliveryInput>>({});
  const [printReceipt, setPrintReceipt] = useState(defaultPrintReceipt);
  const requiresContact = journey !== "receipt";
  const isDelivery = journey === "delivery";
  /* Only a counter sale takes cash here and now. A delivery is collected by
     the rider later; a digital sale already cleared on a terminal. Neither has
     a "cash received" or change to count. */
  const takesCashNow = journey === "receipt";

  const [showSplit, setShowSplit] = useState(splitMode ?? false);
  const [rider, setRider] = useState<RiderRow | null>(null);
  const [keyboardError, setKeyboardError] = useState<string | null>(null);
  /* Set the first time a blocked confirm is pressed. Until then the modal stays
     quiet — listing faults at a cashier who has not finished typing is noise. */
  const [attempted, setAttempted] = useState(false);
  /* null = follow the focused field; set = the cashier chose, and that choice
     holds until they move to another field. */
  const [inputModeOverride, setInputModeOverride] = useState<"pad" | "keys" | null>(null);

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

  /* Switching journey keeps the basket, the customer and anything typed. Only
     the caret moves, to whatever that journey asks for first: a phone number
     for delivery and digital, the cash tendered for a receipt. */
  const switchJourney = useCallback((next: PaymentJourney) => {
    setJourney(next);
    // Cash is not a digital method. Landing on it left the cashier confirming
    // a "cash" digital sale, which is not a thing the shop can do.
    if (next === "digital" && lines[0]?.method === "cash") selectMethod("card");
    if (next === "receipt") {
      const cashLine = lines.find(line => line.method === "cash");
      setActiveField(cashLine ? { kind: "tendered", lineId: cashLine.id } : null);
    } else {
      setActiveField(null);
    }
  }, [lines, selectMethod]);

  /* The contact box owns the number; the delivery record needs it normalised.
     Mirroring it here keeps one source of truth — the validation, the WhatsApp
     send and the saved row all read the same field, whether the cashier typed
     the digits or picked someone whose number the till already knew. */
  useEffect(() => {
    setDeliveryData(previous => previous.contact_number === (contact.e164 ?? "")
      ? previous
      : { ...previous, contact_number: contact.e164 ?? "" });
  }, [contact.e164]);

  /* Open with the caret where the cashier has to type — see focusFirstField.
     Deferred a frame because the fields belong to children and the focus trap
     installs on the same commit, so focusing synchronously races both. */
  useEffect(() => {
    const frame = requestAnimationFrame(() => focusFirstField(containerRef.current));
    return () => cancelAnimationFrame(frame);
  }, []);

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

  /* Dialpad, keyboard shortcuts and Enter-to-confirm all live together: a
     keystroke belongs to a focused text field first and only falls through to
     the virtual display fields when nothing real has the caret. */
  const handleDialpadKey = usePaymentKeyboard({
    activeField, lines, updateLine, setLines, freshTenderedEntryRef,
    selectMethod, onCancel, netTotal, exp: EXP, canConfirm, loading, handleConfirmRef,
  });

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
  const focusedField = useFocusedField(containerRef);
  /* Resting state is the order summary. It becomes the keypad only while a
     field is being edited — the receipt journey opens straight into the cash
     field, so it lands on the keypad without anyone tapping anything. */
  const surface: "summary" | "input" =
    focusedField || activeField ? "input" : "summary";
  /* Numeric fields open on the pad, text fields on the letters, and either can
     be swapped — a flat is "3B" and a road is sometimes a name. */
  const inputMode: "pad" | "keys" =
    inputModeOverride ?? (focusedField && !focusedField.numeric ? "keys" : "pad");

  /* Moving to a different field drops a manual pad/keys choice — the next
     field gets whatever suits it. */
  const focusedLabel = focusedField?.label ?? null;
  useEffect(() => { setInputModeOverride(null); }, [focusedLabel]);

  const summaryRows = (() => {
    const rows = [
      { label: isDelivery ? "Delivery total" : "Total", value: `${DEVICE.currency} ${fmt(netTotal)}`, strong: true },
      { label: "Payment", value: isDelivery ? "Collect on delivery" : methodName },
    ];
    if (isDelivery) rows.push({ label: "Rider", value: rider?.name ?? "Not assigned" });
    if (selectedCust) rows.push({ label: "Customer", value: selectedCust.name });
    if (isDelivery && deliveryData.area) rows.push({ label: "Area", value: String(deliveryData.area) });
    return rows;
  })();

  const showNumericEntry = showSplit || mainLine?.method === "cash" || activeField?.kind === "phone";
  const confirmBlockReason = paymentBlockReason(validationInput, dt, fmt);
  const blockers = paymentBlockers(validationInput, dt, fmt);

  /* Pressing a blocked confirm lists every fault and puts the caret in the
     first field that needs one, so the fix starts where the finger already is. */
  const handleAttemptBlocked = () => {
    setAttempted(true);
    const target = blockers.find(blocker => blocker.focus);
    if (!target?.focus) return;
    const el = containerRef.current?.querySelector<HTMLElement>(target.focus);
    el?.focus();
    if (el && !(el instanceof HTMLInputElement)) el.click();
  };
  /* The button states what pressing it does, and the three journeys do
     different things. "Confirm … received" on the digital path is deliberate:
     the cashier is attesting that money arrived on another device, not moving
     it from this one. */
  const money = `${DEVICE.currency} ${fmt(netTotal)}`;
  const completionLabel = loading
    ? dt("completingSale")
    : language !== "en"
      ? `${dt("completeSale")} (${methodName}) · ${money}`
      : isDelivery
        ? `${rider ? `Send to ${rider.name}` : "Save delivery"} · ${money} to collect`
        : journey === "digital"
          ? `Confirm ${money} received`
          : `Take ${money} · ${methodName.toLowerCase()}`;

  return (
    /*
     * A div, not a button. The overlay used to be a `<button>` wrapping the
     * whole dialog, so every control inside it was a nested interactive
     * element: pressing Space anywhere the caret was not — on the method
     * picker, on a suggestion, on the close button — fired the overlay's own
     * activation behaviour with `target === currentTarget`, and the sale was
     * cancelled mid-entry. The dismiss target is now a sibling behind the
     * dialog, which is what the rest of the app's dialogs already use.
     */
    <div className="modal-overlay" role="presentation">
      <button
        type="button"
        className="modal-overlay-dismiss"
        onClick={onCancel}
        tabIndex={-1}
        aria-hidden="true"
      />
      <div className={`pm-shell pm-shell-calm${requiresContact ? " pm-shell-contact" : ""}`} role="dialog" aria-modal="true" aria-labelledby="pm-dialog-title" ref={containerRef}>

        <div className="pm-left">
          <div className="pm-header">
            <div className="pm-journey-switch" role="group" aria-label="Checkout type">
              {([
                ["receipt", "Receipt", <ReceiptText key="r" size={15} aria-hidden="true" />],
                ["delivery", "Delivery", <Truck key="d" size={15} aria-hidden="true" />],
                ["digital", "Digital", <MessageCircle key="g" size={15} aria-hidden="true" />],
              ] as const).map(([id, label, icon]) => (
                <button
                  key={id}
                  type="button"
                  className={`pm-journey-opt${journey === id ? " is-on" : ""}`}
                  aria-pressed={journey === id}
                  disabled={loading}
                  onClick={() => switchJourney(id)}
                >
                  {icon}<span>{label}</span>
                </button>
              ))}
            </div>
            <button className="pm-close-btn" onClick={onCancel} disabled={loading} aria-label={dt("closePayment")}>
              <X size={19} />
            </button>
          </div>

          {/* The one number the customer is asking about. Display size, not a
              row in a table — everything else on this screen is in service of
              it. */}
          <div className="pm-due">
            <span className="pm-due-label" id="pm-dialog-title">{dt("amountDue")}</span>
            <span className="pm-due-value">
              <span className="pm-due-cur">{DEVICE.currency}</span>{fmt(netTotal)}
            </span>
          </div>

          {/* On a delivery the money is not taken here: the order is recorded
              unpaid with an expected method and the rider collects it. Calling
              this "payment method" invited cashiers to believe the counter had
              been paid. */}
          <div className="pm-step-label">
            <span>1</span> {isDelivery ? "Rider collects with" : dt("choosePaymentMethod")}
          </div>
          <PaymentMethodPicker selected={mainLine.method} onSelect={selectMethod} walletLabel={journey === "receipt" ? undefined : "BenefitPay"} />
          <div className="pm-method-guidance">
            {isDelivery
              ? "Recorded as the method the rider expects to collect. The order stays unpaid until it is marked collected."
              : methodHint}
          </div>

          <div className="pm-step-label">
            <span>2</span> {isDelivery ? "Amount to collect" : dt("confirmAmount")}
          </div>
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

          {/* Cash received and change belong to money crossing the counter.
              On a delivery the rider collects later, so showing a tender field
              here invited the cashier to believe the order had been paid. */}
          {mainLine.method === "cash" && takesCashNow && (
            <PaymentCashTender
              mainLine={mainLine}
              activeField={activeField}
              onFocusTendered={() => setActiveField({ kind: "tendered", lineId: mainLine.id })}
              change={change}
              quickAmts={quickAmts}
              onQuick={applyQuick}
              netTotal={netTotal}
              fmt={fmt}
              dt={dt}
            />
          )}

          {showSplit && (
            <PaymentSplitLines
              lines={lines}
              activeField={activeField}
              remainingMinor={remainingMinor}
              onChangeMethod={(id, method) => updateLine(id, { method })}
              onFocusAmount={id => setActiveField({ kind: "amount", lineId: id })}
              onRemove={removeLine}
              fmt={fmt}
              dt={dt}
            />
          )}
        </div>

        <PaymentContactPanel
          journey={journey}
          contact={contact}
          sessionToken={sessionToken}
          deliveryData={deliveryData}
          onDeliveryChange={setDeliveryData}
          rider={rider}
          onRiderChange={setRider}
          printReceipt={printReceipt}
          onPrintReceiptChange={setPrintReceipt}
          onContactFocus={() => setActiveField({ kind: "phone" })}
          showSplitToggle={journey === "receipt" && !showSplit}
          splitLabel={dt("splitPayment")}
          onSplit={() => {
            setShowSplit(true);
            const rem = formatMoney(remainingMinor, EXP);
            const newLine = mkLine("card");
            if (remainingMinor > 0) newLine.amountStr = rem;
            setLines(previous => [...previous, newLine]);
          }}
        />

        {/* ── PANEL 3: Numpad + save ──────────────────────────────────── */}
        <PaymentCommandPanel
          surface={surface}
          summaryTitle="Order summary"
          summaryRows={summaryRows}
          fieldLabel={focusedField?.label ?? ""}
          fieldValue={focusedField?.value ?? ""}
          inputMode={inputMode}
          onToggleInputMode={() => setInputModeOverride(inputMode === "keys" ? "pad" : "keys")}
          onNextField={() => focusNextField(containerRef.current)}
          onDone={() => {
            (document.activeElement as HTMLElement | null)?.blur();
            setActiveField(null);
          }}
          showNumericEntry={showNumericEntry}
          showTendered={activeField?.kind === "tendered"}
          activeLabel={activeLabel}
          currency={DEVICE.currency}
          tenderedStr={mainLine.tenderedStr}
          totalLabel={fmt(netTotal)}
          methodName={methodName}
          readinessInstruction={readinessInstruction}
          confirmBlockReason={confirmBlockReason}
          blockers={blockers}
          attempted={attempted}
          onAttemptBlocked={handleAttemptBlocked}
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
    </div>
  );
}
