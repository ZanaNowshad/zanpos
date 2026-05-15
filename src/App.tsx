import { useCallback, useState } from "react";
import type { SessionUser, Shift } from "./types";
import { DEVICE } from "./types";
import { shiftGetActive } from "./tauri/commands";
import LoginScreen from "./pages/LoginScreen";
import PosPage from "./pages/PosPage";
import AdminChatPage from "./pages/AdminChatPage";
import ShiftModal from "./components/ShiftModal";
import LockScreen from "./components/LockScreen";
import { useIdleTimer } from "./hooks/useIdleTimer";
import "./App.css";

type View = "login" | "shift_check" | "shift_open" | "pos" | "admin_chat";

/** Auto-lock after 5 minutes of inactivity when a session is active */
const IDLE_TIMEOUT_MS = 5 * 60 * 1000;

export default function App() {
  const [view, setView] = useState<View>("login");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [shift, setShift] = useState<Shift | null>(null);
  const [locked, setLocked] = useState(false);

  // Only run idle timer when a session is active (not on login screen)
  const isSessionActive = sessionUser !== null && view !== "login";
  const handleIdle = useCallback(() => {
    if (isSessionActive) setLocked(true);
  }, [isSessionActive]);
  useIdleTimer(isSessionActive ? IDLE_TIMEOUT_MS : 0, handleIdle);

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

  // Lock screen — overlays whatever view is active
  if (locked && sessionUser) {
    return (
      <LockScreen
        user={sessionUser}
        onUnlock={() => setLocked(false)}
        onLogout={handleLogout}
      />
    );
  }

  if (view === "login" || view === "shift_check") {
    return (
      <>
        <LoginScreen onLogin={handleLogin} />
        {view === "shift_check" && (
          <div className="modal-overlay">
            <div className="checking-shift">Checking shift…</div>
          </div>
        )}
      </>
    );
  }

  if (view === "shift_open" && sessionUser) {
    return (
      <ShiftModal
        mode="open"
        user={sessionUser}
        onShiftOpened={handleShiftOpened}
        onShiftClosed={handleShiftClosed}
      />
    );
  }

  const canAccessAdminChat = sessionUser &&
    (sessionUser.role_name === "owner" || sessionUser.role_name === "manager");

  if (view === "pos" && sessionUser && shift) {
    return (
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
        onOpenAdminChat={canAccessAdminChat ? () => setView("admin_chat") : undefined}
      />
    );
  }

  if (view === "admin_chat" && sessionUser && canAccessAdminChat) {
    return (
      <AdminChatPage
        sessionUser={sessionUser}
        onBackToPOS={() => setView("pos")}
      />
    );
  }

  return <LoginScreen onLogin={handleLogin} />;
}
