import type { ReactNode } from "react";
import CommandSidebar, { type CommandDomain } from "./CommandSidebar";
import CommandHeader, { type HeaderStatusPill } from "./CommandHeader";

interface Props {
  // Sidebar
  activeDomainId: string;
  collapsed: boolean;
  hasSectionNav?: boolean;
  storeName: string;
  roleName: string;
  domains: CommandDomain[];
  onSelectDomain: (domain: CommandDomain) => void;
  onToggleCollapsed: () => void;
  onBackToPOS: () => void;
  // Header
  notificationCount: number;
  onOpenAskBar: () => void;
  onToggleDock: () => void;
  dockOpen: boolean;
  onOpenNotifications: () => void;
  statusPills: HeaderStatusPill[];
  userName: string;
  userRole: string;
  themeLabel: string;
  onToggleTheme: () => void;
  languageLabel: string;
  onToggleLanguage: () => void;
  // Content
  children: ReactNode;
}

/**
 * Persistent application shell for the Command (back-office) interface.
 *
 * Renders the 7-domain sidebar, the global header with ask bar + status +
 * notifications + user menu, and a content slot for the active workspace.
 */
export default function CommandShell({
  activeDomainId,
  collapsed,
  hasSectionNav,
  storeName,
  roleName,
  domains,
  onSelectDomain,
  onToggleCollapsed,
  onBackToPOS,
  notificationCount,
  onOpenAskBar,
  onToggleDock,
  dockOpen,
  onOpenNotifications,
  statusPills,
  userName,
  userRole,
  themeLabel,
  onToggleTheme,
  languageLabel,
  onToggleLanguage,
  children,
}: Props) {
  return (
    <div className="oa-page">
      {/* ── Accessibility: skip-to-content link ── */}
      <a href="#command-content" className="oa-skip-link">
        Skip to content
      </a>

      <CommandSidebar
        activeDomainId={activeDomainId}
        collapsed={collapsed}
        hasSectionNav={hasSectionNav}
        storeName={storeName}
        roleName={roleName}
        domains={domains}
        onSelectDomain={onSelectDomain}
        onToggleCollapsed={onToggleCollapsed}
        onBackToPOS={onBackToPOS}
      />

      <main className="oa-main" id="command-content" aria-live="polite">
        <CommandHeader
          notificationCount={notificationCount}
          onOpenAskBar={onOpenAskBar}
          onToggleDock={onToggleDock}
          dockOpen={dockOpen}
          onOpenNotifications={onOpenNotifications}
          onBackToPOS={onBackToPOS}
          statusPills={statusPills}
          userName={userName}
          userRole={userRole}
          themeLabel={themeLabel}
          onToggleTheme={onToggleTheme}
          languageLabel={languageLabel}
          onToggleLanguage={onToggleLanguage}
        />

        {children}
      </main>
    </div>
  );
}
