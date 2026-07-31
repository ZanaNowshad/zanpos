import { AlertTriangle, X } from "lucide-react";

export interface PosRecoveryAction {
  key: string;
  label: string;
  disabled?: boolean;
}

interface Props {
  error: string;
  actions: PosRecoveryAction[];
  onAction: (key: string) => void;
  onDismiss: () => void;
}

export default function PosRecoveryBanner({ error, actions, onAction, onDismiss }: Props) {
  return (
    <div className="pos-recovery-banner" role="alert">
      <div className="pos-recovery-message">
        <AlertTriangle size={17} aria-hidden="true" />
        <span>{error}</span>
      </div>
      <div className="pos-recovery-actions">
        {actions.slice(0, 5).map(action => (
          <button
            key={action.key}
            onClick={() => onAction(action.key)}
            disabled={action.disabled}
          >
            {action.label}
          </button>
        ))}
        <button className="pos-recovery-dismiss" onClick={onDismiss} aria-label="Dismiss error">
          <X size={16} />
        </button>
      </div>
    </div>
  );
}
