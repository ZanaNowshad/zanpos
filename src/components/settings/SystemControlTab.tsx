import { useMemo, useState } from "react";
import { IcoAI, IcoSystem, IcoWA } from "./Icons";
import AITab from "./AITab";
import HubTab from "./HubTab";
import MaintenanceTab from "./MaintenanceTab";
import SystemTab from "./SystemTab";
import WhatsAppTab from "./WhatsAppTab";

export type SystemControlPane = "hub" | "whatsapp" | "ai" | "system" | "maintenance";

interface Props {
  sessionUserId: string;
  sessionToken: string;
  sessionRole: string;
  appVersion: string;
  backingUp: boolean;
  backupMsg: string | null;
  checkingUpdate: boolean;
  updateMsg: string | null;
  handleBackup: () => void;
  handleCheckUpdate: () => void;
  registerTimer: (id: ReturnType<typeof setTimeout>) => void;
  initialPane?: SystemControlPane;
}

export function systemControlPanes(role: string): Array<{
  id: SystemControlPane;
  label: string;
  description: string;
}> {
  const panes: Array<{ id: SystemControlPane; label: string; description: string }> = [
    { id: "hub", label: "Hub & devices", description: "LAN sync, terminals, store token" },
    { id: "whatsapp", label: "WhatsApp", description: "Sidecar, groups, BenefitPay messages" },
    { id: "ai", label: "AI control", description: "Provider, tools, health checks" },
    { id: "system", label: "Backup & updates", description: "Database backup and app version" },
  ];
  if (role === "owner") {
    panes.push({ id: "maintenance", label: "Maintenance", description: "Owner-gated install actions" });
  }
  return panes;
}

export default function SystemControlTab({
  sessionUserId,
  sessionToken,
  sessionRole,
  appVersion,
  backingUp,
  backupMsg,
  checkingUpdate,
  updateMsg,
  handleBackup,
  handleCheckUpdate,
  registerTimer,
  initialPane,
}: Props) {
  const panes = useMemo(() => systemControlPanes(sessionRole), [sessionRole]);
  const [active, setActive] = useState<SystemControlPane>(
    panes.some(({ id }) => id === initialPane) ? initialPane! : panes[0]?.id ?? "hub",
  );
  const activePane = panes.find(p => p.id === active) ?? panes[0];

  return (
    <div className="settings-system-control">
      <section className="settings-command-head">
        <div>
          <h3 className="settings-page-title">System Control</h3>
          <p className="settings-hint">
            Control the systems that keep this terminal, hub, AI, WhatsApp, and database reliable.
          </p>
        </div>
        <div className="settings-command-status">
          <span>{sessionRole === "owner" ? "Owner controls enabled" : "Manager controls"}</span>
        </div>
      </section>

      <div className="settings-command-nav" role="tablist" aria-label="System control sections">
        {panes.map(pane => (
          <button
            key={pane.id}
            role="tab"
            aria-selected={active === pane.id}
            className={`settings-command-tab${active === pane.id ? " active" : ""}`}
            onClick={() => setActive(pane.id)}
            title={pane.description}
          >
            <span>{pane.id === "whatsapp" ? <IcoWA /> : pane.id === "ai" ? <IcoAI /> : <IcoSystem />}</span>
            <strong>{pane.label}</strong>
          </button>
        ))}
      </div>

      {activePane && (
        <div className="settings-command-context">
          <strong>{activePane.label}</strong>
          <span>{activePane.description}</span>
        </div>
      )}

      {active === "hub" && <HubTab sessionUserId={sessionUserId} />}
      {active === "whatsapp" && (
        <WhatsAppTab
          sessionUserId={sessionUserId}
          sessionRole={sessionRole}
          registerTimer={registerTimer}
        />
      )}
      {active === "ai" && <AITab sessionUserId={sessionUserId} sessionToken={sessionToken} />}
      {active === "system" && (
        <SystemTab
          appVersion={appVersion}
          backingUp={backingUp}
          backupMsg={backupMsg}
          checkingUpdate={checkingUpdate}
          updateMsg={updateMsg}
          handleBackup={handleBackup}
          handleCheckUpdate={handleCheckUpdate}
        />
      )}
      {active === "maintenance" && <MaintenanceTab sessionUserId={sessionUserId} />}
    </div>
  );
}
