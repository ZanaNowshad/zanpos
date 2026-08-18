import { Bell, MessageSquare, Search } from "lucide-react";
import { useLanguage } from "../hooks/useLanguage";
import "./header.css";
import { commandTranslator, type CommandStringKey } from "../i18n/commandStrings";
import type { ReactNode } from "react";

export interface HeaderStatusPill {
  id: string;
  label: string;
  level: "ok" | "warning" | "critical" | "info";
  icon?: ReactNode;
  onClick?: () => void;
}

interface Props {
  notificationCount: number;
  onOpenAskBar: () => void;
  onToggleDock: () => void;
  dockOpen: boolean;
  onOpenNotifications: () => void;
  onBackToPOS: () => void;
  statusPills: HeaderStatusPill[];
  userName: string;
  userRole: string;
  themeLabel: string;
  onToggleTheme: () => void;
  languageLabel: string;
  onToggleLanguage: () => void;
}

export default function CommandHeader({
  notificationCount,
  onOpenAskBar,
  onToggleDock,
  dockOpen,
  onOpenNotifications,
  onBackToPOS,
  statusPills,
  userName,
  userRole,
  themeLabel,
  onToggleTheme,
  languageLabel,
  onToggleLanguage,
}: Props) {
  const { language } = useLanguage();
  const t = commandTranslator(language);

  return (
    /* paddingInlineEnd reserves the fixed window-control strip (3 × 46px);
       without it the native minimise/maximise/close buttons render on top of
       the Till button. Responsive behaviour lives in header.css. */
    <header
      className="oa-topbar zp-topbar"
      style={{ height: "var(--header-height, 56px)", flexShrink: 0, paddingInlineEnd: "150px" }}
    >
      {/* ── Ask bar ── */}
      <button className="oa-command-trigger" onClick={onOpenAskBar} title={t("searchZanAiShortcut" as CommandStringKey)}>
        <Search size={15} />
        <span>{t("searchZanAi" as CommandStringKey)}</span>
        <kbd>Ctrl K</kbd>
      </button>

      <div className="oa-topbar-actions">
        {/* ── Status pills ── */}
        {statusPills.map((pill) => (
          // An actionable pill renders as a real button. The span it replaces
          // only listened for Enter, so Space — which every user expects on
          // something announced as a button — did nothing.
          pill.onClick ? (
            <button
              key={pill.id}
              type="button"
              className={`oa-pulse-chip oa-pulse-${pill.level}`}
              title={pill.label}
              onClick={pill.onClick}
              aria-label={pill.label}
            >
              {pill.icon}
              <span>{pill.label}</span>
            </button>
          ) : (
            <span
              key={pill.id}
              className={`oa-pulse-chip oa-pulse-${pill.level}`}
              title={pill.label}
              role="status"
              aria-label={pill.label}
            >
              {pill.icon}
              <span>{pill.label}</span>
            </span>
          )
        ))}

        {/* ── ZanAI toggle ── */}
        <button className="oa-tool-btn" onClick={onToggleDock} title={dockOpen ? "Collapse ZanAI (Ctrl+/)" : "Open ZanAI (Ctrl+/)"}>
          <MessageSquare size={15} />
          <span>{dockOpen ? "Hide ZanAI" : "ZanAI"}</span>
        </button>

        {/* ── Notification bell ── */}
        <button className="oa-tool-btn" onClick={onOpenNotifications} title="Notifications" aria-label={`Notifications${notificationCount > 0 ? ` — ${notificationCount} pending` : ""}`}>
          <Bell size={15} />
          {notificationCount > 0 && (
            <span className="oa-pulse-chip oa-pulse-warning" style={{ marginLeft: 4 }}>{notificationCount}</span>
          )}
        </button>

        {/* ── User menu ── */}
        <div className="oa-user-card" style={{ display: "inline-flex", alignItems: "center", gap: 8, padding: "0 8px" }}>
          <div className="oa-user-avatar" aria-hidden="true">{userName.charAt(0).toUpperCase()}</div>
          <div className="oa-user-meta">
            <div className="oa-user-name">{userName}</div>
            <div className="oa-user-role">{userRole}</div>
          </div>
        </div>

        {/* ── Theme / Language toggles ── */}
        <button className="oa-tool-btn" onClick={onToggleTheme} title={themeLabel}>
          {themeLabel}
        </button>
        <button className="oa-tool-btn" onClick={onToggleLanguage} title={languageLabel}>
          {languageLabel}
        </button>

        {/* ── Till ── */}
        <button className="oa-back-btn" onClick={onBackToPOS} title="Back to POS (Esc)">
          Till
        </button>
      </div>
    </header>
  );
}
