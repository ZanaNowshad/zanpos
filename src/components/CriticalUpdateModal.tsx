import { useMemo } from "react";
import { useLanguage } from "../hooks/useLanguage";
import { modalTranslator } from "../i18n/modalStrings";

interface Props {
  version: string;
  onGoToSettings: () => void;
  onRemindLater?: () => void;
  onSignOut?: () => void;
  onContinueToTill?: () => void;
  requiredBeforeShift?: boolean;
  canManageUpdates?: boolean;
}

// Blocking prompt for a critical release. The unsigned `critical` flag is
// advisory-only (see src-tauri/src/commands/updater_commands.rs) — this
// modal never installs anything itself; "Go to Settings" only routes to the
// existing owner-gated update UI so the real install stays on the signed
// check_for_updates / download_and_install_update path with its existing
// RBAC check, not a duplicate one here.
export default function CriticalUpdateModal({
  version,
  onGoToSettings,
  onRemindLater,
  onSignOut,
  onContinueToTill,
  requiredBeforeShift = false,
  canManageUpdates = true,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => modalTranslator(language), [language]);
  const dismiss = requiredBeforeShift ? undefined : onRemindLater;
  return (
    <button className="modal-overlay" type="button" onClick={dismiss}>
      <div
        className="modal critical-update-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="critical-update-title"
      >
        <div className="modal-header">
          <span className="modal-title" id="critical-update-title">{t("importantUpdateAvailable")}</span>
        </div>
        <div className="critical-update-body">
          <p>
            {version} {t("versionContainsImportantFix")}{" "}
            {requiredBeforeShift
              ? t("updateRequiredOwnerNote")
              : t("updateBeforeNextShift")}
          </p>
        </div>
        <div className="critical-update-actions">
          {requiredBeforeShift ? (
            <>
              {onSignOut && <button className="btn-secondary" onClick={onSignOut}>{t("signOutForOwner")}</button>}
              {onContinueToTill && (
                <button className="btn-secondary" onClick={onContinueToTill}>
                  {t("continueToTillUpdateDue")}
                </button>
              )}
              {canManageUpdates && (
                <button className="btn-primary" onClick={onGoToSettings}>{t("goToSettings")}</button>
              )}
            </>
          ) : (
            <>
              <button className="btn-secondary" onClick={onRemindLater}>{t("remindLater")}</button>
              <button className="btn-primary" onClick={onGoToSettings}>{t("goToSettings")}</button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
