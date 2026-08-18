import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BatchPendingAction, ChatMessage, ProactiveAlert, SessionUser, StreamEvent, StockLevel } from "../types";
import { DEVICE } from "../types";
import {
  aiChatStream, aiCancelChat, aiExecuteAction, aiExecuteBatchActions, aiCancelAction, aiUndoAction,
  aiRunExecute, aiRunUndo, aiRunCancel,
  aiLoadHistory, aiGetTaskLedgerResume, aiClearHistory, aiSubmitFeedback,
  reportToday, inventoryGetLevels, syncStatus,
  adminGetAlerts, adminDismissAlert,
} from "../tauri/commands";
import { clearAdminChat } from "../adminChatClear";
import type { ChatState, DisplayMessage, KpiSnapshot, RunState, ToolCallEntry } from "./officeAiTypes";
import type { ZanAiSurfaceContext } from "../zanai/zanAiTypes";
import { selectSendContext, serializeSurfaceContext } from "../zanai/zanAiState";
import { appendBoundedMessages, boundLoadedMessages } from "../zanai/messageRetention";
import { shouldAutoExecuteRun } from "../zanai/confirmationPolicy";

const MAX_HISTORY = 40;

export interface ChatControllerOpts {
  sessionUser: SessionUser;
  /** Active-tab context string sent with every message (read per send). */
  getUiContext: () => string;
  /** Called when the AI invokes the open_tab tool (already RBAC-validated by caller). */
  onNavigate: (tab: string) => void;
  /** Called after a confirmed mutation or undo lands — bumps the tab epoch. */
  onMutationApplied: () => void;
}

export interface ImageAttachment {
  base64: string;
  mediaType: string;
  previewUrl: string;
}

export interface BulkProgress {
  tool: string;
  done: number;
  total: number;
}

export interface ChatController {
  messages: DisplayMessage[];
  input: string;
  setInput: (v: string) => void;
  imageAttachment: ImageAttachment | null;
  setImageAttachment: (a: ImageAttachment | null) => void;
  chatState: ChatState;
  pendingAction: DisplayMessage["pendingAction"] | null;
  pendingBatchActions: BatchPendingAction[] | null;
  liveToolCalls: ToolCallEntry[];
  streamingMsgId: string | null;
  tokenCount: number;
  streamStartTime: number | null;
  canStop: boolean;
  kpi: KpiSnapshot;
  runState: RunState | null;
  bulkProgress: BulkProgress | null;
  errorMessage: string | null;
  dismissError: () => void;
  fetchKpi: () => Promise<void>;
  dismissAlert: (alertId: string) => Promise<void>;
  handleSend: (overrideText?: string, sendContext?: ZanAiSurfaceContext) => Promise<void>;
  handleStop: () => Promise<void>;
  handleConfirm: () => Promise<void>;
  handleCancel: () => Promise<void>;
  handleUndo: (undoId: string, msgId: string) => Promise<void>;
  handleFeedback: (messageId: string, rating: "up" | "down", aiSessionId?: string) => Promise<void>;
  handleClearChat: () => void;
  handleRunExecute: () => Promise<void>;
  handleRunCancel: () => void;
  handleRunUndo: () => Promise<void>;
}

/**
 * Rewrite cryptic provider/transport errors into a clear, admin-friendly line.
 * The most common confusing case: the active model (or its endpoint) can't accept
 * images, which surfaces as low-level text like "no endpoint for images" or a
 * provider vision/multimodal error. Anything we don't recognise passes through
 * unchanged so we never hide a genuinely useful message.
 */
function friendlyError(raw: string): string {
  const lower = raw.toLowerCase();
  const looksLikeVision =
    lower.includes("no endpoint for images") ||
    lower.includes("no endpoints found that support image") ||
    lower.includes("image input") ||
    lower.includes("does not support image") ||
    lower.includes("vision") ||
    lower.includes("multimodal") ||
    (lower.includes("image") && (lower.includes("unsupported") || lower.includes("not support")));
  if (looksLikeVision) {
    return "The selected AI model can't read images. Switch to a vision-capable model in " +
      "Admin Settings, or describe the product (name and new price) in text and I'll handle it.";
  }
  return raw;
}

/**
 * Single owner of ALL chat/stream state for the OfficeAI workspace.
 * The docked copilot and the fullscreen Assistant tab are pure renderers of
 * this controller — switching tabs or dock↔fullscreen never remounts the chat
 * or orphans the Tauri Channel (its callbacks close over refs/setters only).
 *
 * Logic is extracted verbatim from the original AdminChatPage; fix comments
 * are load-bearing (confirm-state guard, newHistory capture, synthetic turn).
 */
export function useChatController(opts: ChatControllerOpts): ChatController {
  const { sessionUser, getUiContext, onNavigate, onMutationApplied } = opts;

  // Session tracking for history persistence.
  // Start null so handleSend blocks until history loads — prevents the first
  // message from being saved to a brand-new (wrong) session.
  const [sessionId, setSessionId] = useState<string | null>(null);
  const historyLoadedRef = useRef(false);
  const ledgerResumeShownRef = useRef(false);

  // Chat state
  const [messages, setMessages]             = useState<DisplayMessage[]>([]);
  const [input, setInput]                   = useState("");
  const [chatState, setChatState]           = useState<ChatState>("idle");
  const [runState, setRunState]             = useState<RunState | null>(null);
  const [history, setHistory]               = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction]   = useState<DisplayMessage["pendingAction"] | null>(null);
  const [pendingBatchActions, setPendingBatchActions] = useState<BatchPendingAction[] | null>(null);
  const [liveToolCalls, setLiveToolCalls]   = useState<ToolCallEntry[]>([]);
  const liveToolCallsRef                    = useRef<ToolCallEntry[]>([]);
  const [streamingMsgId, setStreamingMsgId] = useState<string | null>(null);
  const [tokenCount, setTokenCount]         = useState(0);
  const tokenCountRef                       = useRef(0);
  const [streamStartTime, setStreamStartTime] = useState<number | null>(null);
  const assistantMsgIdRef                   = useRef<string>("");
  // Prevent React state updates after unmount (Tauri Channels outlive the
  // component and fire callbacks into closed-over setters).
  const isMountedRef                        = useRef(true);
  const streamActiveRef                     = useRef(false);
  const activeRequestIdRef                  = useRef<string | null>(null);
  const [canStop, setCanStop]               = useState(false);
  // Re-arm in the effect BODY (not just the useRef init) so StrictMode's dev
  // mount→unmount→remount cycle restores the flag. Without this, the cleanup
  // leaves isMountedRef.current === false for the live component and every
  // Tauri Channel event is dropped by the guards below — the chat hangs on
  // "thinking" forever even though the backend streams fine.
  useEffect(() => {
    isMountedRef.current = true;
    return () => { isMountedRef.current = false; };
  }, []);

  const alertsRef = useRef<ProactiveAlert[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const dismissError = useCallback(() => setErrorMessage(null), []);

  // Expiry countdown for pending mutations — auto-cancel when the action expires.
  const expiryTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  useEffect(() => {
    if (pendingAction?.expires_at) {
      expiryTimerRef.current = setInterval(() => {
        const remaining = new Date(pendingAction.expires_at).getTime() - Date.now();
        if (remaining <= 0) {
          if (expiryTimerRef.current) { clearInterval(expiryTimerRef.current); expiryTimerRef.current = null; }
          setPendingAction(null);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          setChatState("idle");
          setMessages(prev => appendBoundedMessages(prev, [{ id: crypto.randomUUID(), role: "system" as const, text: "Action expired.", timestamp: new Date() }]));
        }
      }, 1000);
    }
    return () => {
      if (expiryTimerRef.current) { clearInterval(expiryTimerRef.current); expiryTimerRef.current = null; }
    };
  }, [pendingAction?.expires_at]);

  const [kpi, setKpi] = useState<KpiSnapshot>({
    loading: false, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null, alerts: [],
  });

  const [imageAttachment, setImageAttachmentState] = useState<ImageAttachment | null>(null);
  const imageAttachmentRef = useRef<ImageAttachment | null>(null);
  const setImageAttachment = useCallback((a: ImageAttachment | null) => {
    imageAttachmentRef.current = a;
    setImageAttachmentState(a);
  }, []);

  const addMessage = useCallback((msg: Omit<DisplayMessage, "id" | "timestamp">): DisplayMessage => {
    const full = { ...msg, id: crypto.randomUUID(), timestamp: new Date() };
    setMessages(prev => appendBoundedMessages(prev, [full]));
    return full;
  }, []);

  // ── Load persisted history on mount ─────────────────────────────────────────
  useEffect(() => {
    let cancelled = false;
    aiLoadHistory(sessionUser.session_token, sessionUser.branch_id)
      .then(loaded => {
        if (cancelled) return;
        historyLoadedRef.current = true;
        if (loaded.length === 0) {
          // No history yet — seed sessionId so the first send works.
          setSessionId(crypto.randomUUID());
          return;
        }
        setMessages(boundLoadedMessages(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({
            id: m.message_id,
            role: m.role as "user" | "assistant",
            text: m.content,
            timestamp: new Date(m.created_at.replace(" ", "T")),
            feedbackReady: m.role === "assistant",
            aiSessionId: m.session_id,
          }))
        ));
        setHistory(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({ role: m.role as "user" | "assistant", content: m.content }))
        );
        const lastSessionId = loaded[loaded.length - 1]?.session_id;
        if (lastSessionId) setSessionId(lastSessionId);
      })
      .catch(() => {
        if (cancelled) return;
        historyLoadedRef.current = true;
        setSessionId(prev => prev || crypto.randomUUID());
      })
      .finally(() => {
        if (cancelled) return;
        void aiGetTaskLedgerResume(sessionUser.session_token, sessionUser.branch_id)
          .then(ledger => {
            if (cancelled || !ledger || ledgerResumeShownRef.current) return;
            ledgerResumeShownRef.current = true;
            addMessage({
              role: "system",
              text: `ZanAI was part-way through ${ledger.description} — resume?`,
              suggestedLabel: "Resume task",
              suggestedPrompt:
                "Continue the saved task. Call get_task_ledger first, show me the saved progress, and wait for confirmation before any mutation.",
            });
          })
          .catch(() => {
            // Resume discovery is advisory; it must never block OfficeAI opening.
          });
      });
    return () => { cancelled = true; };
  }, [addMessage, sessionUser.branch_id, sessionUser.session_token]);

  // ── KPI snapshot ────────────────────────────────────────────────────────────
  // Sequential (not parallel) to avoid spiking Rust thread pool + SQLite
  // connections all at once on page open, which stresses WebView2 memory.
  const hasShownStockAlertRef = useRef(false);
  const fetchKpi = useCallback(async () => {
    setKpi(prev => ({ ...prev, loading: true, error: null }));
    const failures: string[] = [];
    const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
    const todaySummary = await reportToday(sessionUser.user_id, sessionUser.branch_id, today)
      .catch(error => { failures.push(`sales: ${String(error)}`); return null; });
    const levels = await inventoryGetLevels(sessionUser.user_id)
      .catch(error => { failures.push(`inventory: ${String(error)}`); return [] as StockLevel[]; });
    const syncStat = await syncStatus(sessionUser.user_id)
      .catch(error => { failures.push(`sync: ${String(error)}`); return null; });
    const lowStockCount   = levels.filter(l => l.is_low_stock && !l.is_out_of_stock).length;
    const outOfStockCount = levels.filter(l => l.is_out_of_stock).length;
    setKpi({
      loading: false,
      error: failures.length > 0 ? `Some live data is unavailable (${failures.join("; ")})` : null,
      today: todaySummary,
      lowStockCount,
      outOfStockCount,
      sync: syncStat,
      alerts: alertsRef.current,
    });
    // Proactive alert — fires once per session on initial KPI load
    if (!hasShownStockAlertRef.current && (outOfStockCount > 0 || lowStockCount >= 3)) {
      hasShownStockAlertRef.current = true;
      const parts: string[] = [];
      if (outOfStockCount > 0) parts.push(`**${outOfStockCount}** product${outOfStockCount > 1 ? "s" : ""} out of stock`);
      if (lowStockCount > 0)   parts.push(`**${lowStockCount}** running low`);
      addMessage({ role: "system", text: `⚠️ Stock alert: ${parts.join(" · ")}. Ask me to review or create a purchase order.` });
    }
  }, [sessionUser.branch_id, sessionUser.user_id, addMessage]);

  useEffect(() => {
    // Fetch KPI after a short delay to avoid spiking memory on page open.
    const t = setTimeout(fetchKpi, 300);
    return () => clearTimeout(t);
  }, [fetchKpi]);

  // ── Proactive alerts — poll on mount + live SSE push from backend loop ──────
  const fetchAlerts = useCallback(async () => {
    let delay = 1000;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const a = await adminGetAlerts(sessionUser.session_token, sessionUser.branch_id);
        alertsRef.current = a;
        setKpi(prev => ({ ...prev, alerts: a }));
        return;
      } catch {
        if (attempt < 2) {
          await new Promise(r => setTimeout(r, delay));
          delay *= 2;
        }
      }
    }
  }, [sessionUser.branch_id, sessionUser.session_token]);

  useEffect(() => {
    fetchAlerts();
    const unlisten = listen<ProactiveAlert[]>("proactive-alerts", (event) => {
      const a = event.payload;
      alertsRef.current = a;
      setKpi(prev => ({ ...prev, alerts: a }));
    });
    return () => { unlisten.then(fn => fn()); };
  }, [fetchAlerts]);

  // ── Bulk operation per-row progress (backend "bulk-progress" events) ────────
  const [bulkProgress, setBulkProgress] = useState<BulkProgress | null>(null);
  const bulkClearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    const unlisten = listen<BulkProgress>("bulk-progress", (event) => {
      if (!isMountedRef.current) return;
      const p = event.payload;
      setBulkProgress(p);
      if (bulkClearTimerRef.current) clearTimeout(bulkClearTimerRef.current);
      // Hide the bar shortly after completion (or if events stop arriving).
      bulkClearTimerRef.current = setTimeout(
        () => setBulkProgress(null),
        p.done >= p.total ? 1500 : 10_000
      );
    });
    return () => {
      unlisten.then(fn => fn());
      if (bulkClearTimerRef.current) clearTimeout(bulkClearTimerRef.current);
    };
  }, []);

  const dismissAlert = useCallback(async (alertId: string) => {
    try {
      await adminDismissAlert(sessionUser.session_token, alertId);
      const next = alertsRef.current.filter(a => a.alert_id !== alertId);
      alertsRef.current = next;
      setKpi(k => ({ ...k, alerts: next }));
    } catch { /* ignore */ }
  }, [sessionUser.session_token]);

  const handleClearChat = useCallback(() => {
    clearAdminChat({
      sessionToken: sessionUser.session_token,
      branchId: sessionUser.branch_id,
      newSessionId: () => crypto.randomUUID(),
      clearHistory: aiClearHistory,
      setMessages: () => setMessages([]),
      setHistory: () => setHistory([]),
      setSessionId,
    });
  }, [sessionUser.branch_id, sessionUser.session_token]);

  const executeRun = useCallback(async (run: RunState) => {
    setRunState(run);
    setChatState("run_executing");
    const onEvent = new Channel<StreamEvent>();
    onEvent.onmessage = (event: StreamEvent) => {
      if (!isMountedRef.current) return;
      if (event.type === "run_progress") {
        setRunState(prev => prev ? { ...prev, done: event.done, phase: "executing" } : prev);
      } else if (event.type === "run_done") {
        setRunState(prev => prev ? { ...prev, phase: "done" } : prev);
        setChatState("idle");
        setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
      } else if (event.type === "run_failed") {
        setRunState(prev => prev ? { ...prev, phase: "failed", error: event.error } : prev);
        setChatState("idle");
      }
    };
    try {
      await aiRunExecute(sessionUser.session_token, run.runId, onEvent);
    } catch (error) {
      setRunState(prev => prev ? { ...prev, phase: "failed", error: String(error) } : prev);
      setChatState("idle");
    }
  }, [fetchKpi, onMutationApplied, sessionUser.session_token]);

  // ── Send ────────────────────────────────────────────────────────────────────
  const handleSend = useCallback(async (overrideText?: string, sendContext?: ZanAiSurfaceContext) => {
    // Block until history loads so we never save the first message to a
    // brand-new session that hasn't been seeded with the historical sessionId.
    if (!sessionId) return;
    const text = (overrideText ?? input).trim();
    const attachment = imageAttachmentRef.current;
    if ((!text && !attachment) || chatState !== "idle") return;
    setInput("");
    setImageAttachment(null);
    addMessage({ role: "user", text, imagePreviewUrl: attachment?.previewUrl });
    const rawHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    // Kept bounded — history is the API context (role-only, no UI metadata).
    // messages[] is the display array. They share user+assistant entries but
    // history gets extra synthetic turns during confirmations to preserve
    // alternating role sequences required by Anthropic.
    const newHistory = rawHistory.length > MAX_HISTORY ? rawHistory.slice(rawHistory.length - MAX_HISTORY) : rawHistory;
    setHistory(newHistory);
    setChatState("thinking");
    // Reset live tool calls + streaming metrics for this new response
    liveToolCallsRef.current = [];
    setLiveToolCalls([]);
    tokenCountRef.current = 0;
    setTokenCount(0);
    setStreamStartTime(Date.now());

    // Push empty assistant bubble (will be filled by tokens)
    const assistantMsg = addMessage({ role: "assistant", text: "" });
    assistantMsgIdRef.current = assistantMsg.id;
    setStreamingMsgId(assistantMsg.id);

    try {
      const onEvent = new Channel<StreamEvent>();
      let finalText = "";

      onEvent.onmessage = (event: StreamEvent) => {
        if (!isMountedRef.current || !streamActiveRef.current) return;
        if (event.type === "started") {
          setCanStop(true);
        } else if (event.type === "token") {
          finalText += event.text;
          tokenCountRef.current += 1;
          setTokenCount(tokenCountRef.current);
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m => m.id === currentId ? { ...m, text: finalText } : m)
          );
        } else if (event.type === "tool_start") {
          const entry: ToolCallEntry = {
            id: `${event.name}-${Date.now()}`,
            name: event.name,
            startTime: Date.now(),
            status: "running",
          };
          liveToolCallsRef.current = [...liveToolCallsRef.current, entry];
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "tool_done") {
          const now = Date.now();
          liveToolCallsRef.current = liveToolCallsRef.current.map(e =>
            e.name === event.name && e.status === "running"
              ? { ...e, status: "done" as const, duration: now - e.startTime }
              : e
          );
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "navigate") {
          // AI steered the workspace via the open_tab tool — caller validates RBAC.
          onNavigate(event.tab);
        } else if (event.type === "run_preview") {
          const run: RunState = {
            runId: event.run_id, opId: event.op_id,
            description: event.description, count: event.count,
            done: 0, phase: "preview",
          };
          if (shouldAutoExecuteRun(event.requires_confirmation)) {
            void executeRun({ ...run, phase: "executing" });
          } else {
            setRunState(run);
            setChatState("run_confirm");
          }
        } else if (event.type === "run_progress") {
          setRunState(prev => prev ? { ...prev, done: event.done, phase: "executing" } : prev);
        } else if (event.type === "run_done") {
          setRunState(prev => prev ? { ...prev, phase: "done" } : prev);
          setChatState("idle");
          setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
        } else if (event.type === "run_failed") {
          setRunState(prev => prev ? { ...prev, phase: "failed", error: event.error } : prev);
          setChatState("idle");
        } else if (event.type === "mutation_pending") {
          const actionData = {
            action_id: event.action_id,
            tool_name: event.tool_name,
            preview: event.preview,
            expires_at: event.expires_at,
            assistant_text: event.assistant_text,
          };
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, text: event.assistant_text || "I'd like to make the following change:", pendingAction: actionData }
                : m
            )
          );
          setPendingAction(actionData);
          setChatState("confirm");
        } else if (event.type === "mutation_batch_pending") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, text: event.assistant_text || "I'd like to make the following changes:", pendingBatchActions: event.actions }
                : m
            )
          );
          setPendingBatchActions(event.actions);
          setChatState("confirm");
        } else if (event.type === "mutation_executed") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(message => message.id === currentId
            ? { ...message, undoId: event.undo_id ?? undefined }
            : message));
          setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
        } else if (event.type === "done") {
          // Stamp stored tool calls into the assistant message, reset live state
          const storedCalls = [...liveToolCallsRef.current];
          const currentId = assistantMsgIdRef.current;
          if (storedCalls.length > 0) {
            setMessages(prev =>
              prev.map(m => m.id === currentId ? { ...m, toolCalls: storedCalls } : m)
            );
          }
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          streamActiveRef.current = false;
          activeRequestIdRef.current = null;
          setCanStop(false);
          // Do NOT override "confirm" state — a mutation_pending event may have set it
          // just before the stream ended. Resetting to "idle" would hide the ConfirmActionModal
          // before the user can approve or deny the pending action.
          setChatState(prev => (
            prev === "confirm" || prev === "run_confirm" || prev === "run_executing"
          ) ? prev : "idle");
          if (finalText) {
            // Keep in-memory context bounded at 60 entries (30 exchanges)
            setHistory(prev => {
              const next = [...prev, { role: "assistant" as const, content: finalText }];
              return next.length > MAX_HISTORY ? next.slice(next.length - MAX_HISTORY) : next;
            });
          }
        } else if (event.type === "message_persisted") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(message => message.id === currentId
            ? { ...message, id: event.message_id, aiSessionId: event.session_id, feedbackReady: true }
            : message));
        } else if (event.type === "cancelled") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(message => message.id === currentId
            ? { ...message, text: finalText || "Response stopped." }
            : message));
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          streamActiveRef.current = false;
          activeRequestIdRef.current = null;
          setCanStop(false);
          setChatState("idle");
        } else if (event.type === "error") {
          const currentId = assistantMsgIdRef.current;
          const friendly = friendlyError(event.message);
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, role: "system" as const, text: `Error: ${friendly}` }
                : m
            )
          );
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          streamActiveRef.current = false;
          activeRequestIdRef.current = null;
          setCanStop(false);
          setPendingAction(null);
          setPendingBatchActions(null);
          setChatState("idle");
          setErrorMessage(friendly);
        }
      };

      // Cap history before sending to API (one cap, applied here)
      const cappedHistory = newHistory;
      const requestId = crypto.randomUUID();
      activeRequestIdRef.current = requestId;
      streamActiveRef.current = true;
      setCanStop(false);
      await aiChatStream(
        sessionUser.session_token,
        {
          request_id: requestId,
          history: cappedHistory,
          message: text,
          branch_id: sessionUser.branch_id,
          currency_exponent: DEVICE.currency_exponent,
          ui_context: serializeSurfaceContext(selectSendContext(
            { surface: "office", summary: getUiContext() },
            sendContext,
          )),
          ...(attachment ? { image_base64: attachment.base64, image_media_type: attachment.mediaType } : {}),
        },
        onEvent
      );
    } catch (e) {
      if (!isMountedRef.current) return;
      streamActiveRef.current = false;
      activeRequestIdRef.current = null;
      setCanStop(false);
      const currentId = assistantMsgIdRef.current;
      const friendly = friendlyError(String(e));
      setMessages(prev =>
        prev.map(m =>
          m.id === currentId
            ? { ...m, role: "system" as const, text: `Error: ${friendly}` }
            : m
        )
      );
      liveToolCallsRef.current = [];
      setLiveToolCalls([]);
      setStreamingMsgId(null);
      setStreamStartTime(null);
      setChatState("idle");
      setErrorMessage(friendly);
    }
  }, [input, chatState, history, sessionId, sessionUser.branch_id, sessionUser.session_token, addMessage, executeRun, getUiContext, onNavigate, fetchKpi, onMutationApplied, setImageAttachment]);

  const handleStop = useCallback(async () => {
    const requestId = activeRequestIdRef.current;
    if (!requestId || !streamActiveRef.current || !canStop) return;
    try {
      await aiCancelChat(sessionUser.session_token, requestId);
    } catch (error) {
      setErrorMessage(friendlyError(String(error)));
    }
  }, [canStop, sessionUser.session_token]);

  // ── Confirm / Cancel / Undo ─────────────────────────────────────────────────
  const handleConfirm = useCallback(async () => {
    // ── Batch path ───────────────────────────────────────────────────────────
    if (pendingBatchActions) {
      const capturedBatch = pendingBatchActions;
      setChatState("thinking");
      setPendingBatchActions(null);
      setStreamingMsgId(null);
      setStreamStartTime(null);
      try {
        const result = await aiExecuteBatchActions(sessionUser.session_token, {
          action_ids: capturedBatch.map(a => a.action_id),
          history,
          assistant_text: "",
          currency_exponent: DEVICE.currency_exponent,
        });
        addMessage({ role: "assistant", text: result.followup });
        setHistory(prev => [
          ...prev,
          { role: "user", content: "Yes, please proceed with all changes." },
          { role: "assistant", content: result.followup },
        ]);
        setChatState("idle");
        setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
      } catch (e) {
        addMessage({ role: "system", text: `Batch execution failed: ${String(e)}` });
        setChatState("idle");
      }
      return;
    }

    // ── Single-action path ───────────────────────────────────────────────────
    if (!pendingAction) return;
    const captured = pendingAction;
    setChatState("thinking");
    setPendingAction(null);
    // FIX: clear streaming state that OpenAI/Gemini path never clears via a Done event
    setStreamingMsgId(null);
    setStreamStartTime(null);
    try {
      const result = await aiExecuteAction(sessionUser.session_token, {
        action_id: captured.action_id,
        history,
        assistant_text: captured.assistant_text,
        currency_exponent: DEVICE.currency_exponent,
      });
      addMessage({
        role: "assistant",
        text: result.followup,
        undoId: result.undo_id ?? undefined,
      });
      // FIX: insert synthetic user confirmation turn to avoid consecutive assistant roles
      // which causes Anthropic 400 "invalid_request_error" on the next message
      setHistory(prev => [
        ...prev,
        { role: "assistant", content: captured.assistant_text },
        { role: "user", content: "Yes, please proceed." },
        { role: "assistant", content: result.followup },
      ]);
      setChatState("idle");
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Execution failed: ${String(e)}` });
      setChatState("idle");
    }
  }, [pendingBatchActions, pendingAction, history, sessionUser.session_token, addMessage, fetchKpi, onMutationApplied]);

  const handleCancel = useCallback(async () => {
    if (pendingBatchActions) {
      await Promise.all(
        pendingBatchActions.map(a => aiCancelAction(sessionUser.session_token, a.action_id).catch(() => {}))
      );
      addMessage({ role: "system", text: "Changes cancelled." });
      setPendingBatchActions(null);
      setStreamingMsgId(null);
      setStreamStartTime(null);
      setChatState("idle");
      return;
    }
    if (!pendingAction) return;
    const capturedCancel = pendingAction;
    await aiCancelAction(sessionUser.session_token, capturedCancel.action_id).catch(() => {});
    addMessage({ role: "system", text: "Action cancelled." });
    setPendingAction(null);
    // FIX: clear streaming state that OpenAI/Gemini never clears via Done event
    setStreamingMsgId(null);
    setStreamStartTime(null);
    setChatState("idle");
  }, [pendingBatchActions, pendingAction, sessionUser.session_token, addMessage]);

  const handleFeedback = useCallback(async (messageId: string, rating: "up" | "down", aiSessionId?: string) => {
    if (!aiSessionId) throw new Error("Message is not yet persisted");
    await aiSubmitFeedback(sessionUser.session_token, aiSessionId, messageId, rating);
  }, [sessionUser.session_token]);

  const handleUndo = useCallback(async (undoId: string, msgId: string) => {
    try {
      const result = await aiUndoAction(sessionUser.session_token, undoId, DEVICE.currency_exponent);
      setMessages(prev => prev.map(m => m.id === msgId ? { ...m, undoId: undefined } : m));
      addMessage({ role: "system", text: result.followup });
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Undo failed: ${String(e)}` });
    }
  }, [sessionUser.session_token, addMessage, fetchKpi, onMutationApplied]);

  // ── Run handlers ────────────────────────────────────────────────────────────
  const handleRunExecute = useCallback(async () => {
    if (!runState || runState.phase !== "preview") return;
    await executeRun({ ...runState, phase: "executing" });
  }, [executeRun, runState]);

  const handleRunCancel = useCallback(async () => {
    if (!runState || runState.phase === "executing") return;
    try {
      await aiRunCancel(sessionUser.session_token, runState.runId);
    } catch { /* best-effort; clear local state regardless */ }
    setRunState(null);
    setChatState("idle");
  }, [runState, sessionUser.session_token]);

  const handleRunUndo = useCallback(async () => {
    if (!runState || runState.phase !== "done" || runState.opId !== "bulk_price_adjust") return;
    try {
      const result = await aiRunUndo(sessionUser.session_token, runState.runId);
      setRunState(null);
      addMessage({ role: "system", text: result.followup });
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Run undo failed: ${String(e)}` });
    }
  }, [runState, sessionUser.session_token, addMessage, fetchKpi, onMutationApplied]);

  return {
    messages, input, setInput, imageAttachment, setImageAttachment,
    chatState, pendingAction, pendingBatchActions, liveToolCalls,
    streamingMsgId, tokenCount, streamStartTime, canStop,
    kpi, runState, bulkProgress, errorMessage, dismissError, fetchKpi, dismissAlert,
    handleSend, handleStop, handleConfirm, handleCancel, handleUndo, handleFeedback, handleClearChat,
    handleRunExecute, handleRunCancel, handleRunUndo,
  };
}
