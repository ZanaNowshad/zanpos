import { useEffect, useRef } from "react";
import { Banknote, Check, CreditCard, Keyboard, Smartphone } from "lucide-react";
import type { PaymentInput } from "../types";
import Dialpad from "./Dialpad";
import { useLanguage } from "../hooks/useLanguage";
import { detailTranslator, type DetailStringKey } from "../i18n/detailStrings";

const METHODS: {
  id: PaymentInput["method"];
  icon: typeof Banknote;
  label: DetailStringKey;
  detail: DetailStringKey;
  key: string;
}[] = [
  { id: "cash", icon: Banknote, label: "cash", detail: "enterReceived", key: "F1" },
  { id: "card", icon: CreditCard, label: "card", detail: "cardTerminal", key: "F2" },
  { id: "wallet", icon: Smartphone, label: "wallet", detail: "wallet", key: "F3" },
];

export function PaymentMethodPicker({
  selected,
  onSelect,
  walletLabel,
}: {
  selected: PaymentInput["method"];
  onSelect: (method: PaymentInput["method"]) => void;
  walletLabel?: string;
}) {
  const { language } = useLanguage();
  const t = detailTranslator(language);
  const activeRef = useRef<HTMLButtonElement>(null);
  const focusedFor = useRef<PaymentInput["method"] | null>(null);

  /*
   * Move focus to the selected method when the selection changes — F1/F2/F3
   * and the c/a/w shortcuts pick a method without touching the mouse, and the
   * radio group needs focus for the arrow keys to work from there.
   *
   * This used to be an inline callback ref: `ref={el => active && el.focus()}`.
   * An inline arrow is a new function on every render, so React detached and
   * re-attached the ref — and re-ran `.focus()` — on *every* render. The modal
   * re-renders on every keystroke, so the cash-received field lost the caret as
   * fast as the cashier could type. Guarding on the selection makes it fire
   * once per change, and a focused text field is never interrupted.
   */
  useEffect(() => {
    if (focusedFor.current === selected) return;
    focusedFor.current = selected;
    const inField = document.activeElement instanceof HTMLInputElement
      || document.activeElement instanceof HTMLTextAreaElement;
    if (inField) return;
    activeRef.current?.focus();
  }, [selected]);

  return (
    <div className="pm-methods" role="radiogroup" aria-label={t("paymentMethod")}>
      {METHODS.map(method => {
        const Icon = method.icon;
        const active = selected === method.id;
        return (
          <button
            key={method.id}
            className={`pm-method${active ? " pm-method-active" : ""}`}
            role="radio"
            aria-checked={active}
            ref={active ? activeRef : undefined}
            onClick={() => onSelect(method.id)}
          >
            <Icon className="pm-method-icon" size={19} strokeWidth={1.8} aria-hidden="true" />
            <span className="pm-method-copy">
              <span className="pm-method-label">{method.id === "wallet" && walletLabel ? walletLabel : t(method.label)}</span>
              <span className="pm-method-detail">{t(method.detail)}</span>
            </span>
            <kbd>{method.key}</kbd>
          </button>
        );
      })}
    </div>
  );
}

interface CommandPanelProps {
  showNumericEntry: boolean;
  showTendered: boolean;
  activeLabel: string;
  currency: string;
  tenderedStr: string;
  totalLabel: string;
  methodName: string;
  readinessInstruction: string;
  confirmBlockReason: string | null;
  canConfirm: boolean;
  loading?: boolean;
  completionLabel: string;
  keyboardError: string | null;
  onKey: (key: string) => void;
  onOpenKeyboard: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

export function PaymentCommandPanel(props: CommandPanelProps) {
  const { language } = useLanguage();
  const t = detailTranslator(language);
  /*
   * The dialpad is always mounted. It used to be swapped out for the readiness
   * panel whenever the sale had no cash amount to type, which left card, wallet
   * and delivery sales with no on-screen keypad at all — the phone, house, flat
   * and road fields could only be filled from a physical keyboard, on what is
   * usually a touchscreen. It now types into whichever field holds the caret,
   * so the readiness panel becomes a compact strip above it rather than a
   * replacement for it.
   */
  return (
    <div className="pm-right">
      {props.showNumericEntry ? (
        <div className="pm-entry-heading">
          <span>{props.showTendered ? t("cashReceived") : props.activeLabel}</span>
          <strong>
            {props.showTendered
              ? `${props.currency} ${props.tenderedStr || "0.000"}`
              : t("useDialpad")}
          </strong>
        </div>
      ) : (
        <div className="pm-ready-panel pm-ready-panel-compact">
          <div className="pm-ready-icon"><Check size={24} strokeWidth={2.5} /></div>
          <span className="pm-ready-kicker">{t("readyToComplete")}</span>
          <h3>{props.methodName} {t("payment")}</h3>
          <div className="pm-ready-amount">{props.currency} {props.totalLabel}</div>
          <div className="pm-ready-check">
            <Check size={16} />
            <span>{t("exactAmountMatched")}</span>
          </div>
          <div className="pm-ready-check pm-ready-check-action">
            <CreditCard size={16} />
            <span>{props.readinessInstruction}</span>
          </div>
        </div>
      )}
      {/* The dialpad covers digits. A delivery sale also asks for a road name
          and a customer name, so the letters have to come from somewhere on a
          till with no keyboard plugged in. */}
      <div className="pm-keyboard-row">
        <button
          type="button"
          className="pm-keyboard-btn"
          /* Same trick as the dialpad keys: without this the button takes
             focus on press, the caret leaves the field the cashier wants to
             type into, and the keyboard they just asked for has no target. */
          onMouseDown={event => event.preventDefault()}
          onClick={props.onOpenKeyboard}
          title={t("onScreenKeyboardHint")}
        >
          <Keyboard size={16} aria-hidden="true" />
          <span>{t("onScreenKeyboard")}</span>
        </button>
      </div>
      {props.keyboardError && (
        <div className="pm-keyboard-error" role="status">{props.keyboardError}</div>
      )}
      <Dialpad onKey={props.onKey} />
      <div className="pm-actions">
        <div className={`pm-confirm-status${props.confirmBlockReason ? " pm-confirm-status-blocked" : ""}`} role="status">
          {props.confirmBlockReason ?? <>{t("readyPressEnter")}</>}
        </div>
        <button
          className="pm-confirm-btn"
          onClick={props.onConfirm}
          disabled={!props.canConfirm || props.loading}
        >
          {props.completionLabel}
        </button>
        <button className="pm-cancel-btn" onClick={props.onCancel} disabled={props.loading}>
          {t("cancel")}
        </button>
      </div>
    </div>
  );
}
