import type { SessionToken } from "../types";
import type {
  OfficeAiActionQueueItem,
  OfficeAiAuditTimelineItem,
  OfficeAiOverviewSnapshot,
  OfficeAiWorkflowInboxItem,
  OfficeAttentionItem,
  OfficePulseModel,
  OfficeTab,
} from "./officeAiTypes";
import type { ChatController } from "./useChatController";
import { officeAiFormat, type OfficeAiTranslator } from "../i18n/officeAiStrings";
import {
  adminGetAiConfig,
  adminGetAiEnabled,
  adminGetAlerts,
  adminGetFeatureToggles,
  adminGetProviderConfig,
  appConfigLoad,
  inventoryGetLevels,
  paymentConfirmationsList,
  reportToday,
  syncStatus,
  systemHealthCheck,
  whatsappListMessages,
  whatsappPollMessages,
  whatsappStatus,
} from "../tauri/commands";

function businessDate(): string {
  return new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
}

async function capture<T>(label: string, task: Promise<T>, errors: string[]): Promise<T | null> {
  try {
    return await task;
  } catch (e) {
    errors.push(`${label}: ${typeof e === "string" ? e : String(e)}`);
    return null;
  }
}

export async function officeAiOverview(
  actorUserId: string,
  sessionToken: SessionToken,
  branchId: string,
  t: OfficeAiTranslator,
): Promise<OfficeAiOverviewSnapshot> {
  const errors: string[] = [];
  const appConfig = await capture(t("storeSettings"), appConfigLoad(), errors);
  const [
    provider,
    aiEnabled,
    aiConfig,
    featureToggles,
    today,
    stock,
    sync,
    whatsapp,
    whatsappUnread,
    payments,
    health,
    alerts,
  ] = await Promise.all([
    capture(t("aiProvider"), adminGetProviderConfig(sessionToken), errors),
    capture(t("aiEnabled"), adminGetAiEnabled(sessionToken), errors),
    capture(t("aiConfig"), adminGetAiConfig(sessionToken), errors),
    capture(t("aiFeatures"), adminGetFeatureToggles(sessionToken), errors),
    branchId
      ? capture(t("todayReport"), reportToday(actorUserId, branchId, businessDate()), errors)
      : Promise.resolve(null),
    capture(t("inventory"), inventoryGetLevels(sessionToken), errors),
    capture(t("sync"), syncStatus(sessionToken), errors),
    capture(t("whatsapp"), whatsappStatus(actorUserId), errors),
    capture(t("whatsappInbox"), whatsappPollMessages(actorUserId), errors),
    capture(t("paymentConfirmations"), paymentConfirmationsList(actorUserId), errors),
    capture(t("systemHealth"), systemHealthCheck(actorUserId), errors),
    /* The proactive detector's output. It has been running every five minutes
       since the app started — margin erosion, dead stock, refund spikes, cash
       discrepancies — and until now the only place it surfaced was a sidebar
       inside the ZanAI chat. Today is where someone actually looks. */
    branchId
      ? capture(t("alerts"), adminGetAlerts(sessionToken, branchId), errors)
      : Promise.resolve(null),
  ]);

  const levels = stock ?? [];
  return {
    loading: false,
    refreshedAt: new Date().toISOString(),
    errors,
    today,
    lowStockCount: levels.filter(l => l.is_low_stock).length,
    outOfStockCount: levels.filter(l => l.is_out_of_stock).length,
    sync,
    whatsapp,
    whatsappUnread: whatsappUnread ?? 0,
    paymentConfirmations: payments ?? [],
    provider,
    aiEnabled,
    aiConfig,
    featureToggles,
    health,
    alerts: (alerts ?? []).filter(a => !a.dismissed_at),
    benefitNumber: appConfig?.whatsapp_benefit_number ?? null,
  };
}

function money(minor: number, exponent: number): string {
  return `BHD ${(minor / Math.pow(10, exponent)).toFixed(exponent)}`;
}

/**
 * Where each kind of proactive alert sends the operator, and what the button
 * says when it gets there.
 *
 * The detector reports thirteen conditions and every one of them has somewhere
 * to go: a margin problem is a catalogue job, a cash discrepancy is a shift
 * job, stuck sync is a health job. An alert that names a problem without a
 * destination is just bad news, which is most of why these were easy to leave
 * in a chat sidebar.
 */
const ALERT_ROUTE: Record<string, { destination: OfficeTab; labelKey: string }> = {
  margin_erosion:   { destination: "products",  labelKey: "reviewPricing" },
  negative_margin:  { destination: "products",  labelKey: "reviewPricing" },
  high_discounts:   { destination: "reports",   labelKey: "reviewSales" },
  refund_spike:     { destination: "reports",   labelKey: "reviewSales" },
  sales_drop:       { destination: "reports",   labelKey: "reviewSales" },
  dead_stock:       { destination: "inventory", labelKey: "reviewStock" },
  overstock:        { destination: "inventory", labelKey: "reviewStock" },
  near_expiry:      { destination: "inventory", labelKey: "reviewStock" },
  low_stock:        { destination: "inventory", labelKey: "reviewStock" },
  stock_out:        { destination: "inventory", labelKey: "reviewStock" },
  cash_discrepancy: { destination: "cashier",   labelKey: "reviewShifts" },
  shift_too_long:   { destination: "cashier",   labelKey: "reviewShifts" },
  sync_stuck:       { destination: "health",    labelKey: "openSystem" },
};

/** The detector's severity words, mapped onto the three the UI draws. */
function alertSeverity(raw: string): OfficeAttentionItem["severity"] {
  const s = raw.toLowerCase();
  if (s === "critical" || s === "high") return "critical";
  if (s === "info" || s === "low") return "info";
  return "warning";
}

export function buildOfficePulseModel(
  snapshot: OfficeAiOverviewSnapshot,
  pendingActionCount: number,
  currencyExp: number,
  t: OfficeAiTranslator,
): OfficePulseModel {
  const transactions = snapshot.today?.transaction_count ?? 0;
  const stockExceptions = snapshot.lowStockCount + snapshot.outOfStockCount;
  const attention: OfficePulseModel["attention"] = [];

  /* Proactive alerts lead the list.
     Everything below this loop is plumbing — sync lag, WhatsApp login, stock
     counts — and plumbing is not what a shopkeeper opens the app to find out.
     These are the money findings: margin gone, stock dead, refunds spiking,
     the drawer short. They were computed every five minutes and shown only in
     the chat sidebar. */
  for (const alert of snapshot.alerts ?? []) {
    const route = ALERT_ROUTE[alert.alert_type];
    attention.push({
      id: `alert:${alert.alert_id}`,
      title: alert.title,
      detail: alert.description,
      severity: alertSeverity(alert.severity),
      // An unrecognised alert type still shows; it just sends the operator to
      // Health rather than nowhere. New detectors must never render a dead row.
      destination: route?.destination ?? "health",
      actionLabel: t((route?.labelKey ?? "openSystem") as never),
    });
  }

  for (const finding of snapshot.health?.findings ?? []) {
    if (finding.severity !== "critical" && finding.severity !== "warning") continue;
    attention.push({
      // The area is carried in the id so Today can tell which capability owns
      // this row; it is deliberately not printed, because "whatsapp: 14
      // messages…" leaks an internal key into a shopkeeper's alert.
      id: `health:${finding.area}:${finding.code}`,
      title: finding.title,
      detail: finding.detail,
      severity: finding.severity,
      destination: "health",
      actionLabel: t("reviewHealth"),
    });
  }

  if (snapshot.sync && !snapshot.sync.online) {
    attention.push({
      id: "sync:offline",
      title: t("hubSyncOffline"),
      detail: officeAiFormat(t("eventsWaitingSync"), { count: snapshot.sync.pending_events ?? 0 }),
      severity: "critical",
      destination: "health",
      actionLabel: t("openSystem"),
    });
  } else if ((snapshot.sync?.pending_events ?? 0) > 0) {
    attention.push({
      id: "sync:pending",
      title: t("changesSyncing"),
      detail: officeAiFormat(t("eventsPending"), { count: snapshot.sync?.pending_events ?? 0 }),
      severity: "warning",
      destination: "health",
      actionLabel: t("viewSync"),
    });
  }

  if (snapshot.outOfStockCount > 0) {
    attention.push({
      id: "stock:out",
      title: officeAiFormat(t("productsOutOfStock"), { count: snapshot.outOfStockCount }),
      detail: officeAiFormat(t("productsBelowReorder"), { count: snapshot.lowStockCount }),
      severity: "critical",
      destination: "inventory",
      actionLabel: t("reviewStock"),
    });
  } else if (snapshot.lowStockCount > 0) {
    attention.push({
      id: "stock:low",
      title: officeAiFormat(t("productsNeedAttention"), { count: snapshot.lowStockCount }),
      detail: t("reviewReorderLevels"),
      severity: "warning",
      destination: "inventory",
      actionLabel: t("reviewStock"),
    });
  }

  const failedPayments = snapshot.paymentConfirmations.filter(payment => payment.status === "failed").length;
  if (failedPayments > 0) {
    attention.push({
      id: "payments:failed",
      title: officeAiFormat(t("paymentsNeedReview"), { count: failedPayments }),
      detail: t("paymentConfidenceLow"),
      severity: "warning",
      destination: "workflows",
      actionLabel: t("reviewPayments"),
    });
  }

  if (snapshot.whatsapp && !snapshot.whatsapp.connected) {
    attention.push({
      id: "whatsapp:offline",
      title: snapshot.whatsapp.qr ? t("whatsappLoginRequired") : t("whatsappDisconnected"),
      detail: t("whatsappWaitHint"),
      severity: "warning",
      destination: "workflows",
      actionLabel: t("openInbox"),
    });
  }

  if (snapshot.aiEnabled === false || !snapshot.provider?.provider) {
    attention.push({
      id: "ai:setup",
      title: snapshot.aiEnabled === false ? t("aiDisabled") : t("aiProviderNotConfigured"),
      detail: t("aiUnavailableHint"),
      severity: "info",
      destination: "settings",
      actionLabel: t("openSettings"),
    });
  }

  if (pendingActionCount > 0) {
    attention.push({
      id: "actions:pending",
      title: officeAiFormat(t("aiActionsAwaiting"), { count: pendingActionCount }),
      detail: t("approvalSafetyHint"),
      severity: "warning",
      destination: "actions",
      actionLabel: t("reviewActions"),
    });
  }

  const severityRank = { critical: 0, warning: 1, info: 2 } as const;
  attention.sort((a, b) => severityRank[a.severity] - severityRank[b.severity]);

  return {
    summary: transactions > 0
      ? officeAiFormat(t("transactionsSummary"), { transactions, exceptions: stockExceptions })
      : stockExceptions
        ? officeAiFormat(t("noTransactionsWithExceptions"), { exceptions: stockExceptions })
        : t("noTransactionsSummary"),
    signals: [
      { id: "sales", label: t("netSalesSignal"), value: money(snapshot.today?.net_total_minor ?? 0, currencyExp), detail: t("today") },
      { id: "transactions", label: t("transactions"), value: String(transactions), detail: `${snapshot.today?.refund_count ?? 0} ${t("refunds")}` },
      { id: "stock", label: t("stockExceptions"), value: String(stockExceptions), detail: `${snapshot.outOfStockCount} ${t("outOfStock")}` },
      { id: "approvals", label: t("pendingApprovals"), value: String(pendingActionCount), detail: t("aiSystemActions") },
    ],
    attention: attention.slice(0, 8),
    allSystemsNormal: attention.length === 0,
  };
}

export async function officeAiWorkflowInbox(
  actorUserId: string,
  t: OfficeAiTranslator,
): Promise<OfficeAiWorkflowInboxItem[]> {
  const [messages, payments] = await Promise.all([
    whatsappListMessages(actorUserId).catch(() => []),
    paymentConfirmationsList(actorUserId).catch(() => []),
  ]);

  const waItems: OfficeAiWorkflowInboxItem[] = messages.slice(0, 20).map(m => ({
    id: `wa:${m.id}`,
    kind: m.media_type === "image" ? "catalog" : "whatsapp",
    title: m.is_group ? (m.chat_name ?? t("whatsappGroup")) : (m.sender_name ?? m.chat_name ?? "WhatsApp"),
    detail: m.body || t(m.media_type === "image" ? "imageReady" : "messageReceived"),
    status: t(m.media_type === "image" ? "aiExtractionReady" : "messageImported"),
    timestamp: new Date(m.ts * 1000).toISOString(),
    unread: !m.read,
    mediaType: m.media_type,
    source: m,
  }));

  const paymentItems: OfficeAiWorkflowInboxItem[] = payments.slice(0, 20).map(p => ({
    id: `pay:${p.id}`,
    kind: "payment",
    title: t(p.status === "confirmed" ? "benefitConfirmed" : "benefitNeedsReview"),
    detail: `${p.receipt_number} · ${p.customer_name ?? p.customer_jid}${p.reason ? ` · ${p.reason}` : ""}`,
    status: t(p.status === "confirmed" ? "proposalApplied" : "aiExtractionReady"),
    timestamp: p.resolved_at ?? p.created_at,
    unread: !p.seen,
    source: p,
  }));

  return [...paymentItems, ...waItems].sort((a, b) =>
    (b.timestamp ?? "").localeCompare(a.timestamp ?? ""),
  );
}

export function officeAiActionQueue(ctrl: ChatController, t: OfficeAiTranslator): OfficeAiActionQueueItem[] {
  const items: OfficeAiActionQueueItem[] = [];
  if (ctrl.pendingBatchActions?.length) {
    items.push({
      id: "pending-batch",
      title: officeAiFormat(t("aiActionsAwaiting"), { count: ctrl.pendingBatchActions.length }),
      detail: ctrl.pendingBatchActions.map(a => a.preview.description).join(" · "),
      status: "pending",
      kind: "batch",
      count: ctrl.pendingBatchActions.length,
      canConfirm: true,
      canCancel: true,
      severity: "warning",
    });
  } else if (ctrl.pendingAction) {
    items.push({
      id: ctrl.pendingAction.action_id,
      title: ctrl.pendingAction.preview.description,
      detail: ctrl.pendingAction.preview.fields.map(f => `${f.label}: ${f.value}`).join(" · "),
      status: "pending",
      kind: "single",
      count: 1,
      canConfirm: true,
      canCancel: true,
      severity: "warning",
    });
  }

  if (ctrl.runState) {
    items.push({
      id: ctrl.runState.runId,
      title: ctrl.runState.description,
      detail: ctrl.runState.phase === "executing"
        ? officeAiFormat(t("completedCount"), { done: ctrl.runState.done, count: ctrl.runState.count })
        : ctrl.runState.error ?? officeAiFormat(t("productUpdates"), { count: ctrl.runState.count }),
      status: ctrl.runState.phase === "preview"
        ? "pending"
        : ctrl.runState.phase === "executing"
          ? "running"
          : ctrl.runState.phase,
      kind: "run",
      count: ctrl.runState.count,
      canConfirm: ctrl.runState.phase === "preview",
      canCancel: ctrl.runState.phase === "preview",
      canUndo: ctrl.runState.phase === "done" && ctrl.runState.opId === "bulk_price_adjust",
      severity: ctrl.runState.phase === "failed" ? "critical" : "info",
    });
  }

  for (const message of ctrl.messages) {
    if (message.undoId) {
      items.push({
        id: message.undoId,
        messageId: message.id,
        title: t("completedAiChange"),
        detail: message.text.slice(0, 160),
        status: "done",
        kind: "undo",
        canUndo: true,
        severity: "info",
      });
    }
  }
  return items;
}

export function officeAiAuditTimeline(ctrl: ChatController, t: OfficeAiTranslator): OfficeAiAuditTimelineItem[] {
  const timeline: OfficeAiAuditTimelineItem[] = [];
  for (const message of ctrl.messages) {
    if (message.pendingAction) {
      timeline.push({
        id: `proposed:${message.pendingAction.action_id}`,
        label: t("aiActionProposed"),
        detail: message.pendingAction.preview.description,
        status: "proposed",
        timestamp: message.timestamp.toISOString(),
      });
    }
    if (message.pendingBatchActions?.length) {
      timeline.push({
        id: `batch:${message.id}`,
        label: t("batchActionProposed"),
        detail: officeAiFormat(t("changesAwaitingApproval"), { count: message.pendingBatchActions.length }),
        status: "proposed",
        timestamp: message.timestamp.toISOString(),
      });
    }
    if (message.toolCalls?.length) {
      for (const tool of message.toolCalls) {
        timeline.push({
          id: `tool:${message.id}:${tool.id}`,
          label: tool.status === "done" ? t("toolCompleted") : t("toolRunning"),
          detail: tool.name,
          status: tool.status === "done" ? "applied" : "proposed",
          timestamp: message.timestamp.toISOString(),
        });
      }
    }
    if (message.undoId) {
      timeline.push({
        id: `undo:${message.undoId}`,
        label: t("undoAvailable"),
        detail: message.text.slice(0, 140),
        status: "applied",
        timestamp: message.timestamp.toISOString(),
      });
    }
  }
  return timeline.sort((a, b) => b.timestamp.localeCompare(a.timestamp)).slice(0, 30);
}
