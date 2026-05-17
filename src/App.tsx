import { useCallback, useEffect, useState } from "react";
import type { AppConfig, SessionUser, Shift } from "./types";
import { DEVICE } from "./types";
import { appConfigLoad, appConfigGetTimeout, shiftGetActive } from "./tauri/commands";
import LoginScreen from "./pages/LoginScreen";
import PosPage from "./pages/PosPage";
import AdminChatPage from "./pages/AdminChatPage";
import SetupWizard from "./pages/SetupWizard";
import ShiftModal from "./components/ShiftModal";
import LockScreen from "./components/LockScreen";
import ErrorBoundary from "./components/ErrorBoundary";
import { useIdleTimer } from "./hooks/useIdleTimer";
import { useTheme } from "./hooks/useTheme";
import "./App.css";
import "./setup-styles.css";

type View = "login" | "shift_check" | "shift_open" | "pos" | "admin_chat";

/** Compute the cloud connectivity banner shown on every screen post-setup. */
function useCloudBanner(appConfig: import("./types").AppConfig | null): {
  level: "none" | "warn" | "danger";
  message: string;
} {
  if (!appConfig || !appConfig.setup_complete) return { level: "none", message: "" };
  if (appConfig.supabase_configured) return { level: "none", message: "" };

  const deadline = appConfig.cloud_grace_deadline
    ? new Date(appConfig.cloud_grace_deadline)
    : null;
  const now = new Date();

  if (!deadline) {
    return { level: "warn", message: "Store not connected to cloud. Open Back Office → Sync to connect Supabase." };
  }

  const daysLeft = Math.ceil((deadline.getTime() - now.getTime()) / 86_400_000);

  if (daysLeft <= 0) {
    return {
      level: "danger",
      message: "Cloud connection overdue. Sales are local-only and not backed up. Connect Supabase in Back Office → Sync.",
    };
  }
  return {
    level: "warn",
    message: `Store not connected to cloud. ${daysLeft} day${daysLeft !== 1 ? "s" : ""} remaining before sync is required. Open Back Office → Sync.`,
  };
}

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

  // ── Session state ──────────────────────────────────────────────────────────
  const [view, setView]               = useState<View>("login");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [shift, setShift]             = useState<Shift | null>(null);
  const [locked, setLocked]           = useState(false);

  // Idle timer — only when session is active
  const isSessionActive = sessionUser !== null && view !== "login";
  const handleIdle = useCallback(() => {
    if (isSessionActive) setLocked(true);
  }, [isSessionActive]);
  useIdleTimer(isSessionActive ? idleTimeoutMs : 0, handleIdle);

  const handleLogin = async (user: SessionUser) => {
    setLocked(false);
    setSessionUser(user);
    setView("shift_check");
    try {
      const active = await shiftGetActive(DEVICE.device_id);
      if (active) {
        setShift(active);
        setView("pos");
      } else {
        setView("shift_open");
      }
    } catch {
      setView("shift_open");
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
        <SetupWizard onComplete={handleSetupComplete} />
      </ErrorBoundary>
    );
  }

  // ── Lock screen ────────────────────────────────────────────────────────────
  if (locked && sessionUser) {
    return (
      <LockScreen
        user={sessionUser}
        onUnlock={() => setLocked(false)}
        onLogout={handleLogout}
      />
    );
  }

  // ── Cloud connectivity warning banner ─────────────────────────────────────
  const cloudBanner = useCloudBanner(appConfig);

  // ── Normal POS flow ────────────────────────────────────────────────────────
  return (
    <ErrorBoundary>
      {cloudBanner.level !== "none" && (
        <div className={`cloud-banner cloud-banner-${cloudBanner.level}`}>
          {cloudBanner.level === "danger" ? "🔴" : "🟡"} {cloudBanner.message}
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
          onShiftClose={(updated) => {
            if (updated) {
              setShift(null);
              setSessionUser(null);
              setView("login");
            }
          }}
          onOpenAdminChat={
            (sessionUser.role_name === "owner" || sessionUser.role_name === "manager")
              ? () => setView("admin_chat")
              : undefined
          }
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}

      {view === "admin_chat" && sessionUser &&
       (sessionUser.role_name === "owner" || sessionUser.role_name === "manager") && (
        <AdminChatPage
          sessionUser={sessionUser}
          onBackToPOS={() => setView("pos")}
        />
      )}

      {/* Fallback: if none of the above matched, go to login */}
      {!["login", "shift_check", "shift_open", "pos", "admin_chat"].includes(view) && (
        <LoginScreen onLogin={handleLogin} />
      )}
    </ErrorBoundary>
  );
}
