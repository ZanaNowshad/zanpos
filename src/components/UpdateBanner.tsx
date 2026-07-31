import { Info, X } from "lucide-react";

interface Props {
  version: string;
  onDismiss: () => void;
  critical?: boolean;
  onAction?: () => void;
  actionLabel?: string;
}

// Cashier-facing at the till in an Arabic-first market — this specific
// banner is a lead-ratified exception to the operator-UI English-only rule
// (see CriticalUpdateModal.tsx). Hardcoded inline AR+EN pair, no i18n system.
export default function UpdateBanner({
  version,
  onDismiss,
  critical = false,
  onAction,
  actionLabel = "Update now",
}: Props) {
  return (
    <div className="pos-recovery-banner update-banner" role="alert">
      <div className="pos-recovery-message">
        <Info size={17} aria-hidden="true" />
        <span>
          {critical
            ? `تحديث مهم مطلوب قبل الوردية التالية (الإصدار ${version}) — Important update before the next shift (v${version})`
            : `يتوفر تحديث جديد (الإصدار ${version}) — Update available (v${version})`}
        </span>
      </div>
      <div className="pos-recovery-actions">
        {onAction && (
          <button className="pos-recovery-action" onClick={onAction}>
            {actionLabel}
          </button>
        )}
        <button className="pos-recovery-dismiss" onClick={onDismiss} aria-label="Dismiss update notice">
          <X size={16} />
        </button>
      </div>
    </div>
  );
}
