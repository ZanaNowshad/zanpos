import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ExternalLink } from "lucide-react";
import SettingsShell, { type SettingsSection } from "./SettingsShell";
import { canOpenGroup } from "./settingsConfig";
import { EmptyState } from "../../../components/templates";
import { checkForUpdates, dbBackup } from "../../../tauri/commands";
import { useLanguage } from "../../../hooks/useLanguage";
import { commandTranslator, type CommandStringKey } from "../../../i18n/commandStrings";
import "./settings.css";

interface Props {
  sessionUserId: string;
  sessionToken: string;
  sessionRole: string;
  initialSection?: SettingsSection;
  /** Navigate to another domain, e.g. Team or Audit, instead of duplicating it. */
  onOpenTab?: (tab: string) => void;
  onStartPractice?: () => void;
}

const StoreIdentityPage = lazy(() => import("./StoreIdentityPage"));
const ReceiptsPage = lazy(() => import("./ReceiptsPage"));
const BusinessRulesPage = lazy(() => import("./BusinessRulesPage"));
const HardwarePage = lazy(() => import("./HardwarePage"));
const StorefrontManagementTab = lazy(() => import("../../../components/settings/StorefrontManagementTab"));
const HubTab = lazy(() => import("../../../components/settings/HubTab"));
const WhatsAppTab = lazy(() => import("../../../components/settings/WhatsAppTab"));
const AITab = lazy(() => import("../../../components/settings/AITab"));
const SystemTab = lazy(() => import("../../../components/settings/SystemTab"));
const MaintenanceTab = lazy(() => import("../../../components/settings/MaintenanceTab"));

function Fallback() { return <div className="zp-set-loading">Loading…</div>; }

/**
 * Settings router.
 *
 * Nine role-filtered groups, each rendering one work area. Integrations and
 * System previously stacked three and two full feature panels respectively into
 * a single scroll; those are now separate groups (Integrations / AI /
 * Data & sync / Security / Advanced) so a page answers one question at a time.
 */
export default function CommandSettingsPage({
  sessionUserId, sessionToken, sessionRole, initialSection, onOpenTab, onStartPractice,
}: Props) {
  const { language } = useLanguage();
  const t = useMemo(() => commandTranslator(language), [language]);
  const [section, setSection] = useState<SettingsSection>(initialSection ?? "store");

  const timerRefs = useRef<ReturnType<typeof setTimeout>[]>([]);
  const registerTimer = (x: ReturnType<typeof setTimeout>) => { timerRefs.current.push(x); };
  useEffect(() => () => { timerRefs.current.forEach(clearTimeout); timerRefs.current = []; }, []);

  // ── Real backup / update wiring ────────────────────────────────────────────
  // SystemTab shipped with no-op handlers and an empty version string, so
  // "Backup Now" and "Check for updates" rendered as working controls that did
  // nothing at all. Both commands exist; they are wired here.
  const [backingUp, setBackingUp] = useState(false);
  const [backupMsg, setBackupMsg] = useState<string | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const appVersion = (import.meta.env.VITE_APP_VERSION as string | undefined) ?? "";

  const handleBackup = useCallback(async () => {
    setBackingUp(true);
    setBackupMsg(null);
    try {
      const dest = await dbBackup("", sessionUserId);
      setBackupMsg(dest ? `${t("backupSaved" as CommandStringKey)} ${dest}` : t("backupSaved" as CommandStringKey));
    } catch (e) {
      setBackupMsg(typeof e === "string" ? e : t("backupFailed" as CommandStringKey));
    } finally {
      setBackingUp(false);
    }
  }, [sessionUserId, t]);

  const handleCheckUpdate = useCallback(async () => {
    setCheckingUpdate(true);
    setUpdateMsg(null);
    try {
      const found = await checkForUpdates();
      setUpdateMsg(found ?? t("upToDate" as CommandStringKey));
    } catch (e) {
      setUpdateMsg(typeof e === "string" ? e : t("updateCheckFailed" as CommandStringKey));
    } finally {
      setCheckingUpdate(false);
    }
  }, [t]);

  // A section reached by deep link must still respect the role, not merely be
  // absent from the rail.
  if (!canOpenGroup(section, sessionRole)) {
    return (
      <SettingsShell activeSection="store" roleName={sessionRole} onSelectSection={setSection}>
        <EmptyState
          variant="restricted"
          title={t("settingsRestricted" as CommandStringKey)}
          description={t("settingsRestrictedHint" as CommandStringKey)}
          actions={[{ label: t("setStore" as CommandStringKey), onClick: () => setSection("store"), primary: true }]}
        />
      </SettingsShell>
    );
  }

  return (
    <SettingsShell activeSection={section} roleName={sessionRole} onSelectSection={setSection}>
      <Suspense fallback={<Fallback />}>
        {section === "store" && <StoreIdentityPage sessionUserId={sessionUserId} sessionRole={sessionRole} />}

        {section === "sales" && (
          <>
            <ReceiptsPage sessionUserId={sessionUserId} />
            <BusinessRulesPage sessionUserId={sessionUserId} onStartPractice={onStartPractice} />
          </>
        )}

        {/* Team configuration lives in the Team domain. Settings points at it
            rather than shipping a second copy of user administration. */}
        {section === "team" && (
          <section className="zp-set-section">
            <h2 className="zp-set-h2">{t("setTeam" as CommandStringKey)}</h2>
            <p className="zp-set-sub">{t("setTeamPointer" as CommandStringKey)}</p>
            <div className="zp-set-actions">
              <button
                type="button"
                className="oa-primary-mini"
                onClick={() => onOpenTab?.("users")}
                disabled={!onOpenTab}
              >
                {t("openTeam" as CommandStringKey)} <ExternalLink size={13} aria-hidden="true" />
              </button>
            </div>
          </section>
        )}

        {section === "hardware" && <HardwarePage sessionUserId={sessionUserId} />}

        {section === "integrations" && (
          <>
            <WhatsAppTab sessionUserId={sessionUserId} sessionRole={sessionRole} registerTimer={registerTimer} />
            <StorefrontManagementTab sessionUserId={sessionUserId} sessionRole={sessionRole} />
          </>
        )}

        {section === "ai" && <AITab sessionUserId={sessionUserId} sessionToken={sessionToken} />}

        {section === "data" && (
          <>
            <HubTab sessionToken={sessionToken} sessionUserId={sessionUserId} />
            <SystemTab
              appVersion={appVersion}
              backingUp={backingUp} backupMsg={backupMsg}
              checkingUpdate={checkingUpdate} updateMsg={updateMsg}
              handleBackup={handleBackup} handleCheckUpdate={handleCheckUpdate}
            />
          </>
        )}

        {/* Session timeout is a security policy but is edited on the store form
            today. Rather than split one form across two groups, this points at
            where the control actually lives. */}
        {section === "security" && (
          <section className="zp-set-section">
            <h2 className="zp-set-h2">{t("setSecurity" as CommandStringKey)}</h2>
            <p className="zp-set-sub">{t("setSecurityPointer" as CommandStringKey)}</p>
            <div className="zp-set-actions">
              <button type="button" className="oa-tool-btn" onClick={() => setSection("store")}>
                {t("setStore" as CommandStringKey)}
              </button>
              <button
                type="button"
                className="oa-tool-btn"
                onClick={() => onOpenTab?.("audit")}
                disabled={!onOpenTab}
              >
                {t("audit" as CommandStringKey)} <ExternalLink size={13} aria-hidden="true" />
              </button>
            </div>
          </section>
        )}

        {section === "advanced" && <MaintenanceTab sessionUserId={sessionUserId} />}
      </Suspense>
    </SettingsShell>
  );
}
