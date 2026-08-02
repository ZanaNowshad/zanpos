import { Banknote, Check, CreditCard, MoreHorizontal, Smartphone } from "lucide-react";
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
  { id: "other", icon: MoreHorizontal, label: "other", detail: "approvedMethod", key: "F4" },
];

export function PaymentMethodPicker({
  selected,
  onSelect,
}: {
  selected: PaymentInput["method"];
  onSelect: (method: PaymentInput["method"]) => void;
}) {
  const { language } = useLanguage();
  const t = detailTranslator(language);
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
            ref={(el) => { if (active && el instanceof HTMLElement) el.focus(); }}
            onClick={() => onSelect(method.id)}
          >
            <Icon className="pm-method-icon" size={19} strokeWidth={1.8} aria-hidden="true" />
            <span className="pm-method-copy">
              <span className="pm-method-label">{t(method.label)}</span>
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
  onKey: (key: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
}

export function PaymentCommandPanel(props: CommandPanelProps) {
  const { language } = useLanguage();
  const t = detailTranslator(language);
  return (
    <div className="pm-right">
      {props.showNumericEntry ? (
        <>
          <div className="pm-entry-heading">
            <span>{props.showTendered ? t("cashReceived") : props.activeLabel}</span>
            <strong>
              {props.showTendered
                ? `${props.currency} ${props.tenderedStr || "0.000"}`
                : t("useDialpad")}
            </strong>
          </div>
          <Dialpad onKey={props.onKey} />
        </>
      ) : (
        <div className="pm-ready-panel">
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
