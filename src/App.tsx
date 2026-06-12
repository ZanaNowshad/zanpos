import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import type { AppConfig, SessionUser, Shift } from "./types";
import { DEVICE } from "./types";
import { appConfigLoad, appConfigGetTimeout, shiftGetActive, shiftOpen } from "./tauri/commands";
import LoginScreen from "./pages/LoginScreen";
import PosPage from "./pages/PosPage";
import ShiftModal from "./components/ShiftModal";
import LockScreen from "./components/LockScreen";
import WindowControls from "./components/WindowControls";
import ErrorBoundary from "./components/ErrorBoundary";
import ReminderPopup from "./components/ReminderPopup";
import { useIdleTimer } from "./hooks/useIdleTimer";
import { useReminderChecker } from "./hooks/useReminderChecker";
import { useTheme } from "./hooks/useTheme";
import type { StickyNote } from "./utils/stickyNotes";
import "./App.css";
import "./setup-styles.css";

const AdminChatPage = lazy(() => import("./pages/AdminChatPage"));
const OfficeAIPage = lazy(() => import("./officeai/OfficeAIPage"));
const SetupWizard = lazy(() => import("./pages/SetupWizard"));
const MigrationAgentPage = lazy(() => import("./pages/MigrationAgentPage"));

type View = "login" | "shift_check" | "shift_open" | "pos" | "admin_chat" | "office_ai";

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

  // ── Session state ──────────────────────────────────────────────────────────
  const [view, setView]               = useState<View>("login");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [shift, setShift]             = useState<Shift | null>(null);
  const [locked, setLocked]           = useState(false);
  const [idleWarning, setIdleWarning] = useState(false);

  // Idle timer — only when session is active
  const isSessionActive = sessionUser !== null && view !== "login";
  const handleIdle = useCallback(() => {
    if (isSessionActive) { setIdleWarning(false); setLocked(true); }
  }, [isSessionActive]);
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

  const handleLogin = async (user: SessionUser) => {
    setLocked(false);
    setSessionUser(user);
    setView("shift_check");
    try {
      const active = await shiftGetActive(DEVICE.device_id, user.user_id);
      if (active) {
        setShift(active);
        setView("pos");
      } else {
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
    setShift(s);
    setView("pos");
  };

  const handleLogout = () => {
    setSessionUser(null);
    setShift(null);
    setView("login");
    setLocked(false);
  };

  const handleShiftClosed = () => {
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

  // ── First-run setup wizard ─────────────────────────────────────────────────
  if (!appConfig.setup_complete) {
    return (
      <ErrorBoundary>
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <SetupWizard
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
  if (migrationMode) {
    return (
      <ErrorBoundary>
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <MigrationAgentPage
            onDone={() => setMigrationMode(false)}
            sessionUserId={sessionUser?.user_id ?? ""}
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
          onUnlock={() => setLocked(false)}
          onLogout={handleLogout}
        />
      </ErrorBoundary>
    );
  }

  // ── Normal POS flow ────────────────────────────────────────────────────────
  return (
    <ErrorBoundary>
      <WindowControls />

      {idleWarning && (
        <div className="idle-warning-banner" onClick={() => setIdleWarning(false)}>
          ⏱ Session locking in 60 seconds — tap anywhere to stay active
          <button className="idle-warning-dismiss" onClick={e => { e.stopPropagation(); setIdleWarning(false); }}>✕</button>
        </div>
      )}

      {(view === "login" || view === "shift_check") && (
        <>
          <LoginScreen onLogin={handleLogin} />
          {view === "shift_check" && (
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
          onLock={() => setLocked(true)}
          onShiftClose={(updated) => {
            if (updated) {
              setShift(null);
              setSessionUser(null);
              setView("login");
            }
          }}
          onOpenOfficeAI={
            (sessionUser.role_name === "owner" || sessionUser.role_name === "manager")
              ? () => setView("office_ai")
              : undefined
          }
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}

      {view === "admin_chat" && sessionUser &&
       (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") && (
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <AdminChatPage
            sessionUser={sessionUser}
            onBackToPOS={() => setView("pos")}
          />
        </Suspense>
      )}
      {view === "office_ai" && sessionUser &&
       (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") && (
        <Suspense fallback={<div className="app-splash"><div className="app-splash-spinner" /></div>}>
          <OfficeAIPage
            sessionUser={sessionUser}
            onBackToPOS={() => setView("pos")}
          />
        </Suspense>
      )}

      {/* Fallback: if none of the above matched, go to login */}
      {!["login", "shift_check", "shift_open", "pos", "admin_chat", "office_ai"].includes(view) && (
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
