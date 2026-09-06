import type { SessionToken, SyncStatus } from "../../types";
import type { Theme } from "../../hooks/useTheme";
import { THEMES } from "../../hooks/useTheme";
import type { Language } from "../../hooks/useLanguage";
import { DEVICE } from "../../types";
import QuranToggle from "../QuranToggle";
import SyncChip from "../SyncChip";
import WhatsAppPill from "./WhatsAppPill";

interface Props {
  showSidebar: boolean;
  cashierName: string;
  theme?: Theme;
  language: Language;
  clockTime: string;
  syncStatus: SyncStatus | null;
  sessionToken: SessionToken;
  commerceEnabled: boolean;
  waConnected: boolean | null;
  waStale: boolean;
  lastReceiptNumber: string | null;
  onToggleSidebar: () => void;
  onHome: () => void;
  onToggleTheme?: () => void;
  onToggleLanguage: () => void;
  onOpenSyncDetails: () => void;
  onReprintLast: () => void;
}

export default function PosTopBar({
  showSidebar,
  cashierName,
  theme,
  language,
  clockTime,
  syncStatus,
  sessionToken,
  commerceEnabled,
  waConnected,
  waStale,
  lastReceiptNumber,
  onToggleSidebar,
  onHome,
  onToggleTheme,
  onToggleLanguage,
  onOpenSyncDetails,
  onReprintLast,
}: Props) {
  const themeMeta = theme ? THEMES.find(candidate => candidate.id === theme) : undefined;
  const themeIndex = theme ? THEMES.findIndex(candidate => candidate.id === theme) : -1;
  const nextTheme = themeIndex >= 0 ? THEMES[(themeIndex + 1) % THEMES.length] : undefined;

  return (
    <div className="top-bar" data-tauri-drag-region="true">
      <div className="top-bar-left" data-tauri-drag-region="true">
        <button
          className="top-bar-sidebar-toggle"
          onClick={onToggleSidebar}
          title={showSidebar ? "Hide sidebar" : "Show sidebar"}
          aria-label={showSidebar ? "Hide sidebar" : "Show sidebar"}
          data-tauri-drag-region="false"
        >☰</button>
        <button
          className="top-bar-logo top-bar-logo-btn"
          onClick={onHome}
          title="Back to POS"
          data-tauri-drag-region="false"
        >ZAN<span>POS</span></button>
        <div className="top-bar-store-stack" data-tauri-drag-region="true">
          <span className="top-bar-store-name">{DEVICE.branch_name}</span>
          <span className="top-bar-cashier-sub">{cashierName}</span>
        </div>
        {onToggleTheme && themeMeta && nextTheme && (
          <button
            className="top-bar-btn top-bar-theme"
            onClick={onToggleTheme}
            title={`Theme: ${themeMeta.label} — click for ${nextTheme.label}`}
            data-tauri-drag-region="false"
          >
            {themeMeta.icon} {themeMeta.label}
          </button>
        )}
        <button
          className="top-bar-btn"
          onClick={onToggleLanguage}
          title={language === "en" ? "التبديل إلى العربية" : "Switch to English"}
          data-tauri-drag-region="false"
        >
          {language === "en" ? "ع" : "EN"}
        </button>
      </div>

      <div className="top-bar-center" data-tauri-drag-region="true">
        <span className="top-bar-time" data-tauri-drag-region="true">{clockTime}</span>
        <QuranToggle />
      </div>

      <div className="top-bar-right" data-tauri-drag-region="true">
        <SyncChip status={syncStatus} sessionToken={sessionToken} onOpenDetails={onOpenSyncDetails} />
        {commerceEnabled && <WhatsAppPill connected={waConnected} stale={waStale} />}
        {lastReceiptNumber && (
          <button
            className="top-bar-btn"
            onClick={onReprintLast}
            title={`Reprint #${lastReceiptNumber} (Ctrl+P)`}
            data-tauri-drag-region="false"
          >
            Reprint
          </button>
        )}
      </div>
    </div>
  );
}
