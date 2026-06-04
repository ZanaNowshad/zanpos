import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

// ── Minimal inline SVGs ───────────────────────────────────────────────────────
const IcoMinimize = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

const IcoMaximize = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <rect x="1.5" y="1.5" width="7" height="7" rx="1" stroke="currentColor" strokeWidth="1.5"/>
  </svg>
);

const IcoRestore = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <rect x="3.5" y="1.5" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.25"/>
    <path d="M1.5 4v3.5A1 1 0 0 0 2.5 8.5H6" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round"/>
  </svg>
);

const IcoClose = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <line x1="2" y1="2" x2="8" y2="8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
    <line x1="8" y1="2" x2="2" y2="8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
  </svg>
);

// ── Component ─────────────────────────────────────────────────────────────────
export default function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();

    // Read initial state
    win.isMaximized().then(setMaximized).catch(() => {});

    // Listen for resize events to keep icon in sync
    let unlisten: (() => void) | null = null;
    win.onResized(() => {
      win.isMaximized().then(setMaximized).catch(() => {});
    }).then(fn => { unlisten = fn; }).catch(() => {});

    return () => { unlisten?.(); };
  }, []);

  const handleMinimize = () => getCurrentWindow().minimize().catch(() => {});
  const handleMaximize = () => getCurrentWindow().toggleMaximize().catch(() => {});
  const handleClose    = () => getCurrentWindow().close().catch(() => {});

  return (
    <div className="win-ctrls" data-tauri-drag-region="false">
      <button
        className="win-btn win-btn-min"
        onClick={handleMinimize}
        title="Minimize"
        aria-label="Minimize window"
        tabIndex={-1}
      >
        <IcoMinimize />
      </button>
      <button
        className="win-btn win-btn-max"
        onClick={handleMaximize}
        title={maximized ? "Restore" : "Maximize"}
        aria-label={maximized ? "Restore window" : "Maximize window"}
        tabIndex={-1}
      >
        {maximized ? <IcoRestore /> : <IcoMaximize />}
      </button>
      <button
        className="win-btn win-btn-close"
        onClick={handleClose}
        title="Close"
        aria-label="Close window"
        tabIndex={-1}
      >
        <IcoClose />
      </button>
    </div>
  );
}
