import { Bot, ClipboardCheck, HeartPulse, Wifi } from "lucide-react";
import type { OfficeAiTranslator } from "../i18n/officeAiStrings";
import type { OfficeAiActionQueueItem, OfficeAiOverviewSnapshot } from "./officeAiTypes";
import type { OfficePulseItem } from "./officeAiTypes";

interface PulseInput {
  overview: OfficeAiOverviewSnapshot;
  actionItems: OfficeAiActionQueueItem[];
  configured: boolean;
  isManager: boolean;
  t: OfficeAiTranslator;
}

export function buildOfficePagePulse({
  overview,
  actionItems,
  configured,
  isManager,
  t,
}: PulseInput): OfficePulseItem[] {
  const items: OfficePulseItem[] = [];
  const syncLabel = overview.sync
    ? overview.sync.online
      ? overview.sync.pending_events ? `${overview.sync.pending_events} ${t("syncing")}` : t("synced")
      : t("offline")
    : t("notChecked");

  if (overview.sync && (!overview.sync.online || overview.sync.pending_events > 0)) {
    items.push({
      id: "sync",
      label: syncLabel,
      level: overview.sync.online ? "warning" : "critical",
      icon: <Wifi size={11} />,
    });
  }

  const pendingCount = actionItems.filter(item => item.status === "pending").length;
  if (pendingCount > 0 && isManager) {
    items.push({
      id: "pending",
      label: `${pendingCount} ${t("pending")}`,
      level: "warning",
      icon: <ClipboardCheck size={11} />,
    });
  }

  if (overview.health && !overview.health.summary.ok) {
    items.push({
      id: "health",
      label: t("systemNeedsAttention"),
      level: "critical",
      icon: <HeartPulse size={11} />,
    });
  }

  if (!configured || overview.aiEnabled === false) {
    items.push({
      id: "ai",
      label: overview.aiEnabled === false ? t("aiDisabled") : t("aiSetupNeeded"),
      level: "warning",
      icon: <Bot size={11} />,
    });
  }
  return items;
}

export const EMPTY_OFFICE_OVERVIEW: OfficeAiOverviewSnapshot = {
  loading: true,
  refreshedAt: null,
  errors: [],
  today: null,
  lowStockCount: 0,
  outOfStockCount: 0,
  sync: null,
  whatsapp: null,
  whatsappUnread: 0,
  paymentConfirmations: [],
  provider: null,
  aiEnabled: null,
  aiConfig: null,
  featureToggles: null,
  health: null,
  benefitNumber: null,
};
