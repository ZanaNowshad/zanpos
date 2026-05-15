import { useState } from "react";
import type { SessionUser, Shift } from "./types";
import { DEVICE } from "./types";
import { shiftGetActive } from "./tauri/commands";
import LoginScreen from "./pages/LoginScreen";
import PosPage from "./pages/PosPage";
import AdminChatPage from "./pages/AdminChatPage";
import ShiftModal from "./components/ShiftModal";
import "./App.css";

type View = "login" | "shift_check" | "shift_open" | "pos" | "admin_chat";

export default function App() {
  const [view, setView] = useState<View>("login");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [shift, setShift] = useState<Shift | null>(null);

  const handleLogin = async (user: SessionUser) => {
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
  };

  const handleShiftClosed = () => {
    setShift(null);
    setSessionUser(null);
    setView("login");
  };

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
