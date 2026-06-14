import { useCallback, useEffect, useRef, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import type { ChatMessage, SessionUser, StreamEvent, StockLevel } from "../types";
import { DEVICE } from "../types";
import {
  aiChatStream, aiExecuteAction, aiCancelAction, aiUndoAction,
  aiRunExecute, aiRunUndo,
  aiSaveMessage, aiLoadHistory, aiClearHistory,
  reportToday, inventoryGetLevels, syncStatus,
} from "../tauri/commands";
import { clearAdminChat } from "../adminChatClear";
import type { ChatState, DisplayMessage, KpiSnapshot, RunState, ToolCallEntry } from "./officeAiTypes";

export interface ChatControllerOpts {
  sessionUser: SessionUser;
  /** Active-tab context string sent with every message (read per send). */
  getUiContext: () => string;
  /** Called when the AI invokes the open_tab tool (already RBAC-validated by caller). */
  onNavigate: (tab: string) => void;
  /** Called after a confirmed mutation or undo lands — bumps the tab epoch. */
  onMutationApplied: () => void;
}

export interface ChatController {
  messages: DisplayMessage[];
  input: string;
  setInput: (v: string) => void;
  chatState: ChatState;
  pendingAction: DisplayMessage["pendingAction"] | null;
  liveToolCalls: ToolCallEntry[];
  streamingMsgId: string | null;
  tokenCount: number;
  streamStartTime: number | null;
  kpi: KpiSnapshot;
  runState: RunState | null;
  fetchKpi: () => Promise<void>;
  handleSend: (overrideText?: string) => Promise<void>;
  handleConfirm: () => Promise<void>;
  handleCancel: () => Promise<void>;
  handleUndo: (undoId: string, msgId: string) => Promise<void>;
  handleClearChat: () => void;
  handleRunExecute: () => Promise<void>;
  handleRunCancel: () => void;
  handleRunUndo: () => Promise<void>;
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

  // Session tracking for history persistence
  const [sessionId, setSessionId] = useState<string>(() => crypto.randomUUID());

  // Chat state
  const [messages, setMessages]             = useState<DisplayMessage[]>([]);
  const [input, setInput]                   = useState("");
  const [chatState, setChatState]           = useState<ChatState>("idle");
  const [runState, setRunState]             = useState<RunState | null>(null);
  const [history, setHistory]               = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction]   = useState<DisplayMessage["pendingAction"] | null>(null);
  const [liveToolCalls, setLiveToolCalls]   = useState<ToolCallEntry[]>([]);
  const liveToolCallsRef                    = useRef<ToolCallEntry[]>([]);
  const [streamingMsgId, setStreamingMsgId] = useState<string | null>(null);
  const [tokenCount, setTokenCount]         = useState(0);
  const tokenCountRef                       = useRef(0);
  const [streamStartTime, setStreamStartTime] = useState<number | null>(null);
  const assistantMsgIdRef                   = useRef<string>("");

  const [kpi, setKpi] = useState<KpiSnapshot>({
    loading: false, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null,
  });

  const addMessage = useCallback((msg: Omit<DisplayMessage, "id" | "timestamp">): DisplayMessage => {
    const full = { ...msg, id: crypto.randomUUID(), timestamp: new Date() };
    setMessages(prev => [...prev, full]);
    return full;
  }, []);

  // ── Load persisted history on mount ─────────────────────────────────────────
  useEffect(() => {
    aiLoadHistory(DEVICE.branch_id, sessionUser.user_id)
      .then(loaded => {
        if (loaded.length === 0) return;
        setMessages(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({
            id: crypto.randomUUID(),
            role: m.role as "user" | "assistant",
            text: m.content,
            timestamp: new Date(m.created_at.replace(" ", "T")),
          }))
        );
        setHistory(loaded
          .filter(m => m.role === "user" || m.role === "assistant")
          .map(m => ({ role: m.role as "user" | "assistant", content: m.content }))
        );
        const lastSessionId = loaded[loaded.length - 1]?.session_id;
        if (lastSessionId) setSessionId(lastSessionId);
      })
      .catch(() => {});
  }, [sessionUser.user_id]);

  // ── KPI snapshot ────────────────────────────────────────────────────────────
  // Sequential (not parallel) to avoid spiking Rust thread pool + SQLite
  // connections all at once on page open, which stresses WebView2 memory.
  const fetchKpi = useCallback(async () => {
    setKpi(prev => ({ ...prev, loading: true, error: null }));
    try {
      const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
      const todaySummary = await reportToday(sessionUser.user_id, DEVICE.branch_id, today).catch(() => null);
      const levels = await inventoryGetLevels(sessionUser.user_id).catch(() => [] as StockLevel[]);
      const syncStat = await syncStatus(sessionUser.user_id).catch(() => null);
      const lowStockCount   = levels.filter(l => l.is_low_stock && !l.is_out_of_stock).length;
      const outOfStockCount = levels.filter(l => l.is_out_of_stock).length;
      setKpi({ loading: false, error: null, today: todaySummary, lowStockCount, outOfStockCount, sync: syncStat });
    } catch (e) {
      setKpi(prev => ({ ...prev, loading: false, error: String(e) }));
    }
  }, [sessionUser.user_id]);

  useEffect(() => {
    // Fetch KPI after a short delay to avoid spiking memory on page open.
    const t = setTimeout(fetchKpi, 300);
    return () => clearTimeout(t);
  }, [fetchKpi]);

  const handleClearChat = useCallback(() => {
    clearAdminChat({
      branchId: DEVICE.branch_id,
      userId: sessionUser.user_id,
      newSessionId: () => crypto.randomUUID(),
      clearHistory: aiClearHistory,
      setMessages: () => setMessages([]),
      setHistory: () => setHistory([]),
      setSessionId,
    });
  }, [sessionUser.user_id]);

  // ── Send ────────────────────────────────────────────────────────────────────
  const handleSend = useCallback(async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    addMessage({ role: "user", text });
    aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "user", text, "text").catch(() => {});
    const rawHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    // Keep bounded — 60 entries max
    const newHistory: ChatMessage[] = rawHistory.length > 60 ? rawHistory.slice(rawHistory.length - 60) : rawHistory;
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
        if (event.type === "token") {
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
          setRunState({
            runId: event.run_id, opId: event.op_id,
            description: event.description, count: event.count,
            done: 0, phase: "preview",
          });
          setChatState("run_confirm");
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
        } else if (event.type === "done") {
          // Stamp stored tool calls into the assistant message, reset live state
          const storedCalls = [...liveToolCallsRef.current];
          if (storedCalls.length > 0) {
            const currentId = assistantMsgIdRef.current;
            setMessages(prev =>
              prev.map(m => m.id === currentId ? { ...m, toolCalls: storedCalls } : m)
            );
          }
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          // Do NOT override "confirm" state — a mutation_pending event may have set it
          // just before the stream ended. Resetting to "idle" would hide the ConfirmActionModal
          // before the user can approve or deny the pending action.
          setChatState(prev => (prev === "confirm" || prev === "run_confirm") ? prev : "idle");
          if (finalText) {
            // Keep in-memory context bounded at 60 entries (30 exchanges)
            setHistory(prev => {
              const next = [...prev, { role: "assistant" as const, content: finalText }];
              return next.length > 60 ? next.slice(next.length - 60) : next;
            });
            aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "assistant", finalText, "text").catch(() => {});
          }
        } else if (event.type === "error") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev =>
            prev.map(m =>
              m.id === currentId
                ? { ...m, role: "system" as const, text: `Error: ${event.message}` }
                : m
            )
          );
          liveToolCallsRef.current = [];
          setLiveToolCalls([]);
          setStreamingMsgId(null);
          setStreamStartTime(null);
          setPendingAction(null);  // FIX: clear stale pending action on error
          setChatState("idle");
        }
      };

      // Cap history to last 40 messages to avoid unbounded IPC payload growth
      // FIX: use newHistory (includes the user's current message) not stale `history`
      const cappedHistory = newHistory.length > 40 ? newHistory.slice(newHistory.length - 40) : newHistory;
      await aiChatStream(
        {
          history: cappedHistory,
          message: text,
          user_id: sessionUser.user_id,
          branch_id: DEVICE.branch_id,
          currency_exponent: DEVICE.currency_exponent,
          ui_context: getUiContext(),
        },
        onEvent
      );
    } catch (e) {
      const currentId = assistantMsgIdRef.current;
      setMessages(prev =>
        prev.map(m =>
          m.id === currentId
            ? { ...m, role: "system" as const, text: `Error: ${String(e)}` }
            : m
        )
      );
      liveToolCallsRef.current = [];
      setLiveToolCalls([]);
      setStreamingMsgId(null);
      setStreamStartTime(null);
      setChatState("idle");
    }
  }, [input, chatState, history, sessionId, sessionUser.user_id, addMessage, getUiContext, onNavigate]);

  // ── Confirm / Cancel / Undo ─────────────────────────────────────────────────
  const handleConfirm = useCallback(async () => {
    if (!pendingAction) return;
    // Capture pendingAction before clearing it (state is async, pendingAction still valid here)
    const captured = pendingAction;
    setChatState("thinking");
    setPendingAction(null);
    // FIX: clear streaming state that OpenAI/Gemini path never clears via a Done event
    setStreamingMsgId(null);
    setStreamStartTime(null);
    try {
      const result = await aiExecuteAction({
        action_id: captured.action_id,
        user_id: sessionUser.user_id,
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
      // Refresh KPI + visible tab after a mutation
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Execution failed: ${String(e)}` });
      setChatState("idle");
    }
  }, [pendingAction, history, sessionUser.user_id, addMessage, fetchKpi, onMutationApplied]);

  const handleCancel = useCallback(async () => {
    if (!pendingAction) return;
    const capturedCancel = pendingAction;
    await aiCancelAction(capturedCancel.action_id, sessionUser.user_id).catch(() => {});
    addMessage({ role: "system", text: "Action cancelled." });
    setPendingAction(null);
    // FIX: clear streaming state that OpenAI/Gemini never clears via Done event
    setStreamingMsgId(null);
    setStreamStartTime(null);
    setChatState("idle");
  }, [pendingAction, sessionUser.user_id, addMessage]);

  const handleUndo = useCallback(async (undoId: string, msgId: string) => {
    try {
      const result = await aiUndoAction(undoId, sessionUser.user_id, DEVICE.currency_exponent, sessionUser.user_id);
      setMessages(prev => prev.map(m => m.id === msgId ? { ...m, undoId: undefined } : m));
      addMessage({ role: "system", text: result.followup });
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Undo failed: ${String(e)}` });
    }
  }, [sessionUser.user_id, addMessage, fetchKpi, onMutationApplied]);

  // ── Run handlers ────────────────────────────────────────────────────────────
  const handleRunExecute = useCallback(async () => {
    if (!runState || runState.phase !== "preview") return;
    setRunState(prev => prev ? { ...prev, phase: "executing" } : prev);
    setChatState("run_executing");
    const onEvent = new Channel<StreamEvent>();
    onEvent.onmessage = (event: StreamEvent) => {
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
      await aiRunExecute(runState.runId, sessionUser.user_id, onEvent);
    } catch (e) {
      setRunState(prev => prev ? { ...prev, phase: "failed", error: String(e) } : prev);
      setChatState("idle");
    }
  }, [runState, sessionUser.user_id, fetchKpi, onMutationApplied]);

  const handleRunCancel = useCallback(() => {
    setRunState(null);
    setChatState("idle");
  }, []);

  const handleRunUndo = useCallback(async () => {
    if (!runState || runState.phase !== "done") return;
    try {
      const result = await aiRunUndo(runState.runId, sessionUser.user_id);
      setRunState(null);
      addMessage({ role: "system", text: result.followup });
      setTimeout(() => { fetchKpi(); onMutationApplied(); }, 500);
    } catch (e) {
      addMessage({ role: "system", text: `Run undo failed: ${String(e)}` });
    }
  }, [runState, sessionUser.user_id, addMessage, fetchKpi, onMutationApplied]);

  return {
    messages, input, setInput, chatState, pendingAction, liveToolCalls,
    streamingMsgId, tokenCount, streamStartTime,
    kpi, runState, fetchKpi,
    handleSend, handleConfirm, handleCancel, handleUndo, handleClearChat,
    handleRunExecute, handleRunCancel, handleRunUndo,
  };
}
