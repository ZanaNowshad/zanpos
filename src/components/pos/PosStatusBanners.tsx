import { GraduationCap } from "lucide-react";
import type { PosStringKey } from "../../i18n/posStrings";
import type { RecoverableCart } from "../../hooks/useCartRecovery";
import LicenseGraceBanner from "../LicenseGraceBanner";
import PosRecoveryBanner, { type PosRecoveryAction } from "../PosRecoveryBanner";
import Button from "../ui/Button";

interface Props {
  trainingMode: boolean;
  t: (key: PosStringKey) => string;
  recoverable: RecoverableCart | null;
  error: string | null;
  recoveryActions: PosRecoveryAction[];
  onExitTraining: () => void;
  onRecoverCart: () => void;
  onDismissRecovery: () => void;
  onRecoveryAction: (key: string) => void;
  onDismissError: () => void;
}

export default function PosStatusBanners({
  trainingMode,
  t,
  recoverable,
  error,
  recoveryActions,
  onExitTraining,
  onRecoverCart,
  onDismissRecovery,
  onRecoveryAction,
  onDismissError,
}: Props) {
  return (
    <>
      {trainingMode && (
        <div className="pos-training-banner" role="status">
          <GraduationCap size={16} strokeWidth={2} aria-hidden="true" />
          <strong>{t("trainingMode")}</strong>
          <span>{t("trainingNotice")}</span>
          <Button variant="secondary" layoutClassName="pos-training-exit" onClick={onExitTraining}>
            {t("exitTraining")}
          </Button>
        </div>
      )}

      {/* Informational only. Licensing never gates the till. */}
      <LicenseGraceBanner />

      {recoverable && (
        <div className="pos-recovery-prompt" role="status">
          <span>
            <strong>A sale was interrupted.</strong>{" "}
            {recoverable.lineCount} item{recoverable.lineCount === 1 ? "" : "s"} were in the
            cart when the app last closed.
          </span>
          <Button variant="primary" onClick={onRecoverCart}>Restore cart</Button>
          <Button variant="ghost" onClick={onDismissRecovery}>Discard</Button>
        </div>
      )}

      {error && (
        <PosRecoveryBanner
          error={error}
          actions={recoveryActions}
          onAction={onRecoveryAction}
          onDismiss={onDismissError}
        />
      )}
    </>
  );
}
