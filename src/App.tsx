import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import type { AiHandoff, AppConfig, ProductPrefill, SessionUser, Shift, StartupComponentStatus } from "./types";
import { DEVICE } from "./types";
import { appConfigLoad, appConfigGetTimeout, authLogout, shiftGetActive, shiftOpen, startupHealthCheck, startupRestartSidecar } from "./tauri/commands";
import LoginScreen from "./pages/LoginScreen";
import PosPage from "./pages/PosPage";
import ShiftModal from "./components/ShiftModal";
import LockScreen from "./components/LockScreen";
import WindowControls from "./components/WindowControls";
import ErrorBoundary from "./components/ErrorBoundary";
import ReminderPopup from "./components/ReminderPopup";
import UpdateBanner from "./components/UpdateBanner";
import CriticalUpdateModal from "./components/CriticalUpdateModal";
import { useUpdatePromptLifecycle } from "./hooks/useUpdatePromptLifecycle";
import type { OfficeTab } from "./officeai/officeAiTypes";
import { useIdleTimer } from "./hooks/useIdleTimer";
import { useReminderChecker } from "./hooks/useReminderChecker";
import { useTheme } from "./hooks/useTheme";
import type { StickyNote } from "./utils/stickyNotes";
// Tokens first: every theme variable App.css consumes is defined here, and the
// cascade depends on this order.
import "./styles/tokens.css";
import "./App.css";
import "./components/ui/ui.css";
import "./setup-styles.css";
import "./operator-ux.css";

const OfficeAIPage = lazy(() => import("./officeai/OfficeAIPage"));
const SetupWizard = lazy(() => import("./pages/SetupWizard"));
const MigrationAgentPage = lazy(() => import("./pages/MigrationAgentPage"));

type View = "login" | "shift_check" | "shift_open" | "pos" | "office_ai";

export default function App() {
  // ── Theme (initialised early so there's no flash on load) ─────────────────
  const { theme, toggle: toggleTheme } = useTheme();

  // ── App config (loaded from DB before showing any UI) ──────────────────────
  const [appConfig, setAppConfig] = useState<AppConfig | null>(null);
  const [configLoading, setConfigLoading] = useState(true);
  const [idleTimeoutMs, setIdleTimeoutMs] = useState(5 * 60 * 1000);

  useEffect(() => {
    Promise.all([appConfigLoad(), appConfigGetTimeout().catch(() => 5)])
      .then(([cfg, minutes]) => {
        DEVICE.init(cfg);       // sync module-level DEVICE object with real DB values
        setAppConfig(cfg);
        setIdleTimeoutMs((minutes as number) * 60 * 1000);
      })
      .catch(() => {
        // Cannot load DB at all — show error; app is in broken state
        setAppConfig(null);
      })
      .finally(() => setConfigLoading(false));
  }, []);

  // ── Migration mode (post-setup import flow) ───────────────────────────────
  const [migrationMode, setMigrationMode] = useState(false);

  // ── Startup health check (after config loads, before login) ─────────────────
  const [startupStatuses, setStartupStatuses] = useState<StartupComponentStatus[]>([]);
  const [startupTicker, setStartupTicker] = useState(0);
  const startupForceAttempted = useRef(false);

  useEffect(() => {
    if (configLoading || !appConfig || !appConfig.setup_complete) return;
    let cancelled = false;
    setStartupTicker(0);

    const poll = async (): Promise<StartupComponentStatus[]> => {
      if (cancelled) return [];
      try {
        const statuses = await startupHealthCheck();
        if (cancelled) return [];
        setStartupStatuses(statuses);
        return statuses;
      } catch {
        return [];
      }
    };

    const loop = async () => {
      let statuses = await poll();
      let elapsed = 0;
      while (!cancelled) {
        await new Promise((r) => setTimeout(r, 2000));
        if (cancelled) return;
        statuses = await poll();
        elapsed += 2;
        setStartupTicker(elapsed);

        const allOk = statuses.every((s) => s.status === "ok" || s.status !== "starting");
        if (allOk) return;

        const sidecar = statuses.find((s) => s.component === "whatsapp_sidecar");
        if (sidecar?.status === "starting" && elapsed >= 12 && !startupForceAttempted.current) {
          startupForceAttempted.current = true;
          await startupRestartSidecar().catch(() => {});
          continue;
        }
        if (elapsed >= 30) return;
      }
    };

    loop();
    return () => { cancelled = true; };
  }, [configLoading, appConfig]);

  const allComponentsGreen =
    startupStatuses.length > 0 &&
    startupStatuses.every((s) => s.status === "ok");

  const anyComponentError =
    startupStatuses.length > 0 &&
    startupStatuses.some((s) => s.status === "error");

  const startupCheckDone = allComponentsGreen || (anyComponentError && startupTicker >= 14);

  // ── Session state ──────────────────────────────────────────────────────────
  const [view, setView]               = useState<View>("login");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [shift, setShift]             = useState<Shift | null>(null);
  const [locked, setLocked]           = useState(false);
  const [idleWarning, setIdleWarning] = useState(false);
  // Seed for OfficeAI's product create form when opened from a POS notification.
  const [officeAiPrefill, setOfficeAiPrefill] = useState<ProductPrefill | null>(null);
  // Message (+ optional image) to auto-send to the OfficeAI assistant (from a WhatsApp notification).
  const [officeAiInitialMessage, setOfficeAiInitialMessage] = useState<AiHandoff | null>(null);
  const [officeAiInitialTab, setOfficeAiInitialTab] = useState<OfficeTab | undefined>();
  const [officeAiMaintenance, setOfficeAiMaintenance] = useState(false);
  const [ownerUpdateIntent, setOwnerUpdateIntent] = useState(false);

  const revokeSession = useCallback(async (user: SessionUser | null) => {
    if (user?.session_token) await authLogout(user.session_token).catch(() => {});
  }, []);

  const handleLock = useCallback(async () => {
    if (!sessionUser) return;
    await revokeSession(sessionUser);
    setIdleWarning(false);
    setLocked(true);
  }, [revokeSession, sessionUser]);

  // Idle timer — only when session is active
  const isSessionActive = sessionUser !== null && view !== "login";
  const handleIdle = useCallback(() => {
    if (isSessionActive) handleLock();
  }, [handleLock, isSessionActive]);
  const handleIdleWarn   = useCallback(() => { if (isSessionActive) setIdleWarning(true); }, [isSessionActive]);
  const handleIdleResume = useCallback(() => setIdleWarning(false), []);
  useIdleTimer(
    isSessionActive ? idleTimeoutMs : 0,
    handleIdle,
    handleIdleWarn,
    handleIdleResume,
  );

  // ── Reminder queue ─────────────────────────────────────────────────────────
  // Reminders fire globally — they appear on top of whichever page is active.
  const [reminderQueue, setReminderQueue] = useState<StickyNote[]>([]);

  useReminderChecker(useCallback((due: StickyNote[]) => {
    setReminderQueue(prev => {
      const existingIds = new Set(prev.map(n => n.id));
      const fresh = due.filter(n => !existingIds.has(n.id));
      return fresh.length > 0 ? [...prev, ...fresh] : prev;
    });
  }, []));

  const updatePrompt = useUpdatePromptLifecycle();

  const handleLogin = async (user: SessionUser) => {
    setLocked(false);
    setSessionUser(user);
    if (migrationMode) return;
    if (ownerUpdateIntent && user.role_name === "owner") {
      setOwnerUpdateIntent(false);
      setOfficeAiInitialTab("settings");
      setOfficeAiMaintenance(true);
      setView("office_ai");
      return;
    }
    setOwnerUpdateIntent(false);
    setView("shift_check");
    try {
      const active = await shiftGetActive(DEVICE.device_id, user.user_id);
      if (active) {
        updatePrompt.activateShift();
        setShift(active);
        setView("pos");
      } else {
        if (!(await updatePrompt.prepareNewShift())) return;
        // T10: auto-open a 0-float shift so cashiers skip the ShiftModal step on cold start.
        // Fall back to ShiftModal if the auto-open fails (e.g. UNIQUE conflict — T11).
        try {
          const opened = await shiftOpen(DEVICE.branch_id, DEVICE.device_id, user.user_id, 0);
          setShift(opened);
          setView("pos");
        } catch {
          // Auto-open can fail with "already open" if shiftGetActive missed a race
          // (e.g. a shift opened by another terminal since the check above).
          // Retry before falling back to the ShiftModal.
          try {
            const retryActive = await shiftGetActive(DEVICE.device_id, user.user_id);
            if (retryActive) { setShift(retryActive); setView("pos"); return; }
          } catch { /* ignore */ }
          setView("shift_open");
        }
      }
    } catch {
      // shiftGetActive itself threw — still try auto-open before giving up.
      if (!(await updatePrompt.prepareNewShift())) return;
      try {
        const opened = await shiftOpen(DEVICE.branch_id, DEVICE.device_id, user.user_id, 0);
        setShift(opened);
        setView("pos");
      } catch {
        setView("shift_open");
      }
    }
  };

  const handleShiftOpened = (s: Shift) => {
    updatePrompt.activateShift();
    setShift(s);
    setView("pos");
  };

  const handleLogout = async () => {
    await revokeSession(sessionUser);
    setSessionUser(null);
    setShift(null);
    setView("login");
    setLocked(false);
  };

  const handleShiftClosed = async () => {
    updatePrompt.completeEod();
    await revokeSession(sessionUser);
    setShift(null);
    setSessionUser(null);
    setView("login");
  };

  const handleSetupComplete = (cfg: AppConfig) => {
    DEVICE.init(cfg);
    setAppConfig(cfg);
    // After setup the user lands on the login screen
    setView("login");
  };

  // ── Loading splash ─────────────────────────────────────────────────────────
  if (configLoading) {
    return (
      <div className="app-splash">
        <div className="app-splash-logo">ZAN<span>POS</span></div>
        <div className="app-splash-spinner" />
      </div>
    );
  }

  // ── DB load failure ────────────────────────────────────────────────────────
  if (!appConfig) {
    return (
      <div className="app-splash">
        <div className="app-splash-logo">ZAN<span>POS</span></div>
        <p className="app-splash-error">
          Failed to initialise the database.<br />
          Please restart the application.
        </p>
      </div>
    );
  }

  // ── Startup health check ──────────────────────────────────────────────────
  if (appConfig.setup_complete && !startupCheckDone) {
    return (
      <div className="app-splash">
        <div className="app-splash-logo">ZAN<span>POS</span></div>
        <div className="startup-checklist">
          {startupStatuses.map((s) => (
            <div key={s.component} className={`startup-item startup-${s.status}`}>
              <span className="startup-dot" />
              <span className="startup-label">{s.message}</span>
            </div>
          ))}
          {startupStatuses.length === 0 && (
            <div className="startup-item startup-checking">
              <span className="startup-dot" />
              <span className="startup-label">Checking subsystems…</span>
            </div>
          )}
        </div>
        {allComponentsGreen && (
          <p className="startup-ready">All systems ready &#10003;</p>
        )}
        {anyComponentError && startupTicker >= 14 && (
          <button className="startup-proceed" onClick={() => setStartupStatuses(s => s.map(i => ({...i, status: "ok"})))}>Proceed Anyway</button>
        )}
      </div>
    );
  }

  // ── First-run setup wizard ─────────────────────────────────────────────────
  if (!appConfig.setup_complete) {
    return (
      <ErrorBoundary>
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <SetupWizard
            initialConfig={appConfig}
            onComplete={handleSetupComplete}
            onMigrate={(cfg: AppConfig) => {
              DEVICE.init(cfg);
              setAppConfig(cfg);
              setMigrationMode(true);
            }}
          />
        </Suspense>
      </ErrorBoundary>
    );
  }

  // ── Migration agent (post-setup import flow) ───────────────────────────────
  if (migrationMode && sessionUser) {
    return (
      <ErrorBoundary>
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <MigrationAgentPage
            onDone={async () => {
              await revokeSession(sessionUser);
              setSessionUser(null);
              setMigrationMode(false);
              setView("login");
            }}
            sessionUserId={sessionUser.user_id}
            sessionToken={sessionUser.session_token}
          />
        </Suspense>
      </ErrorBoundary>
    );
  }

  // ── Lock screen ────────────────────────────────────────────────────────────
  if (locked && sessionUser) {
    return (
      <ErrorBoundary>
        <LockScreen
          user={sessionUser}
          onUnlock={(refreshedUser) => {
            setSessionUser(refreshedUser);
            setLocked(false);
          }}
          onLogout={handleLogout}
        />
      </ErrorBoundary>
    );
  }

  // ── Normal POS flow ────────────────────────────────────────────────────────
  return (
    <ErrorBoundary>
      <WindowControls />

      {(updatePrompt.decision === "normal-dismissible" ||
        updatePrompt.decision === "critical-deferrable") &&
        updatePrompt.version && (
        <UpdateBanner
          version={updatePrompt.version}
          critical={updatePrompt.decision === "critical-deferrable"}
          onDismiss={updatePrompt.dismissPostEod}
          actionLabel="Sign in as owner to update"
          onAction={() => {
            updatePrompt.dismissPostEod();
            setOwnerUpdateIntent(true);
          }}
        />
      )}

      {view === "shift_check" &&
        sessionUser &&
        updatePrompt.decision === "critical-required" &&
        updatePrompt.version && (
          <CriticalUpdateModal
            version={updatePrompt.version}
            requiredBeforeShift
            canManageUpdates={sessionUser.role_name === "owner"}
            onSignOut={handleLogout}
            onContinueToTill={() => {
              updatePrompt.allowCriticalShift();
              setView("shift_open");
            }}
            onGoToSettings={() => {
              setOfficeAiInitialTab("settings");
              setOfficeAiMaintenance(true);
              setView("office_ai");
            }}
          />
        )}

      {idleWarning && (
        <div role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); (e.target as HTMLElement).click(); } }}  className="idle-warning-banner" onClick={() => setIdleWarning(false)}>
          ⏱ Session locking in 60 seconds — tap anywhere to stay active
          <button className="idle-warning-dismiss" onClick={e => { e.stopPropagation(); setIdleWarning(false); }}>✕</button>
        </div>
      )}

      {(view === "login" || view === "shift_check") && (
        <>
          <LoginScreen onLogin={handleLogin} />
          {view === "shift_check" && updatePrompt.decision !== "critical-required" && (
            <div className="modal-overlay">
              <div className="checking-shift">Checking shift…</div>
            </div>
          )}
        </>
      )}

      {view === "shift_open" && sessionUser && (
        <ShiftModal
          mode="open"
          user={sessionUser}
          onShiftOpened={handleShiftOpened}
          onShiftClosed={handleShiftClosed}
        />
      )}

      {view === "pos" && sessionUser && shift && (
        <PosPage
          sessionUser={sessionUser}
          shift={shift}
          onLogout={handleLogout}
          onLock={handleLock}
          onShiftClose={async (updated) => {
            if (updated) await handleShiftClosed();
          }}
          onOpenOfficeAI={
            (sessionUser.role_name === "owner" || sessionUser.role_name === "manager")
              ? (prefill) => { setOfficeAiPrefill(prefill ?? null); setOfficeAiInitialMessage(null); setOfficeAiInitialTab(undefined); setOfficeAiMaintenance(false); setView("office_ai"); }
              : undefined
          }
          onAskOfficeAI={
            (sessionUser.role_name === "owner" || sessionUser.role_name === "manager")
              ? (handoff) => { setOfficeAiInitialMessage(handoff); setOfficeAiPrefill(null); setOfficeAiInitialTab(undefined); setOfficeAiMaintenance(false); setView("office_ai"); }
              : undefined
          }
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}

      {view === "office_ai" && sessionUser &&
       (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") && (
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <OfficeAIPage
            sessionUser={sessionUser}
            onBackToPOS={() => {
              setOfficeAiPrefill(null);
              setOfficeAiInitialMessage(null);
              setOfficeAiInitialTab(undefined);
              setOfficeAiMaintenance(false);
              setView(shift
                ? "pos"
                : updatePrompt.decision === "critical-required" ? "shift_check" : "login");
            }}
            initialProductPrefill={officeAiPrefill}
            initialAiMessage={officeAiInitialMessage}
            initialTab={officeAiInitialTab}
            initialMaintenancePane={officeAiMaintenance}
          />
        </Suspense>
      )}

      {/* Fallback: if none of the above matched, go to login */}
      {!["login", "shift_check", "shift_open", "pos", "office_ai"].includes(view) && (
        <LoginScreen onLogin={handleLogin} />
      )}

      {/* ── Reminder popup — rendered above everything ── */}
      {reminderQueue.length > 0 && (
        <ReminderPopup
          note={reminderQueue[0]}
          onClose={() => setReminderQueue(prev => prev.slice(1))}
        />
      )}
    </ErrorBoundary>
  );
}
