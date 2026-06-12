import { Channel } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { clearAdminChat } from "../adminChatClear";
import type { DisplayMessage, KpiSnapshot, ToolCallEntry, ChatState } from "./officeAiTypes";
import type { ChatMessage, SessionUser, StreamEvent } from "../types";
import { aiSaveMessage, aiLoadHistory, aiClearHistory, aiChatStream, aiExecuteAction, aiCancelAction, aiUndoAction } from "../tauri/commands";
import { DEVICE } from "../types";

let _msgIdCounter = 1;

export interface ChatControllerOpts {
  sessionUser: SessionUser;
  getUiContext: () => string;
  onNavigate: (tab: string) => void;
  onMutationApplied: () => void;
}

export interface ChatController {
  messages: DisplayMessage[];
  input: string; setInput: (v: string) => void;
  chatState: ChatState;
  pendingAction: DisplayMessage["pendingAction"] | null;
  liveToolCalls: ToolCallEntry[];
  handleSend: (overrideText?: string) => Promise<void>;
  handleConfirm: () => Promise<void>;
  handleCancel: () => Promise<void>;
  handleUndo: (undoId: string, msgId: string) => Promise<void>;
  handleClear: () => Promise<void>;
  history: ChatMessage[];
  fetchKpi: () => Promise<KpiSnapshot>;
  kpi: KpiSnapshot;
}

export function useChatController(opts: ChatControllerOpts): ChatController {
  const { sessionUser, getUiContext, onNavigate, onMutationApplied } = opts;
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [chatState, setChatState] = useState<ChatState>("idle");
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction] = useState<DisplayMessage["pendingAction"] | null>(null);
  const [liveToolCalls, setLiveToolCalls] = useState<ToolCallEntry[]>([]);
  const liveToolCallsRef = useRef<ToolCallEntry[]>([]);
  const [sessionId, setSessionId] = useState<string>(crypto.randomUUID());
  const assistantMsgIdRef = useRef<string>("");
  const [kpi, setKpi] = useState<KpiSnapshot>({ loading: true, error: null, today: null, lowStockCount: 0, outOfStockCount: 0, sync: null });

  const addMessage = useCallback((m: { role: "user" | "assistant" | "system"; text: string }): string => {
    const id = String(++_msgIdCounter);
    setMessages(prev => [...prev, { ...m, id, timestamp: new Date() }]);
    return id;
  }, []);

  // Load history on mount
  useEffect(() => {
    aiLoadHistory(DEVICE.branch_id, sessionUser.user_id).then(loaded => {
      if (loaded.length === 0) return;
      setMessages(loaded.filter(m => m.role === "user" || m.role === "assistant").map(m => ({
        id: crypto.randomUUID(), role: m.role as "user" | "assistant", text: m.content, timestamp: new Date(m.created_at.replace(" ", "T")),
      })));
      setHistory(loaded.filter(m => m.role === "user" || m.role === "assistant").map(m => ({ role: m.role as "user" | "assistant", content: m.content })));
      const lastSid = loaded[loaded.length - 1]?.session_id;
      if (lastSid) setSessionId(lastSid);
    }).catch(() => {});
  }, [sessionUser.user_id]);

  const handleSend = useCallback(async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    addMessage({ role: "user", text });
    aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "user", text, "text").catch(() => {});
    const rawHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    const newHistory = rawHistory.length > 60 ? rawHistory.slice(rawHistory.length - 60) : rawHistory;
    setHistory(newHistory);
    setChatState("thinking");
    liveToolCallsRef.current = [];
    setLiveToolCalls([]);
    let tokenCount = 0;
    const assistantMsgId = addMessage({ role: "assistant", text: "" });
    assistantMsgIdRef.current = assistantMsgId;

    try {
      const onEvent = new Channel<StreamEvent>();
      let finalText = "";
      onEvent.onmessage = (event: StreamEvent) => {
        if (event.type === "token") {
          finalText += event.text; tokenCount++;
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(m => m.id === currentId ? { ...m, text: finalText } : m));
        } else if (event.type === "tool_start") {
          const entry: ToolCallEntry = { id: `${event.name}-${Date.now()}`, name: event.name, startTime: Date.now(), status: "running" };
          liveToolCallsRef.current = [...liveToolCallsRef.current, entry];
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "tool_done") {
          const now = Date.now();
          liveToolCallsRef.current = liveToolCallsRef.current.map(e =>
            e.name === event.name && e.status === "running" ? { ...e, status: "done" as const, duration: now - e.startTime } : e);
          setLiveToolCalls([...liveToolCallsRef.current]);
        } else if (event.type === "mutation_pending") {
          const actionData = { action_id: event.action_id, tool_name: event.tool_name, preview: event.preview, expires_at: event.expires_at, assistant_text: event.assistant_text };
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(m => m.id === currentId ? { ...m, text: event.assistant_text || "I'd like to make the following change:", pendingAction: actionData } : m));
          setPendingAction(actionData); setChatState("confirm");
        } else if (event.type === "navigate") {
          onNavigate(event.tab);
        } else if (event.type === "done") {
          const storedCalls = [...liveToolCallsRef.current];
          if (storedCalls.length > 0) {
            const currentId = assistantMsgIdRef.current;
            setMessages(prev => prev.map(m => m.id === currentId ? { ...m, toolCalls: storedCalls } : m));
          }
          liveToolCallsRef.current = []; setLiveToolCalls([]);
          setChatState(prev => prev === "confirm" ? "confirm" : "idle");
          if (finalText) {
            setHistory(prev => { const next = [...prev, { role: "assistant" as const, content: finalText }]; return next.length > 60 ? next.slice(next.length - 60) : next; });
            aiSaveMessage(sessionId, DEVICE.branch_id, sessionUser.user_id, "assistant", finalText, "text").catch(() => {});
          }
        } else if (event.type === "error") {
          const currentId = assistantMsgIdRef.current;
          setMessages(prev => prev.map(m => m.id === currentId ? { ...m, role: "system" as const, text: `Error: ${event.message}` } : m));
          liveToolCallsRef.current = []; setLiveToolCalls([]); setPendingAction(null); setChatState("idle");
        }
      };
      const cappedHistory = newHistory.length > 40 ? newHistory.slice(newHistory.length - 40) : newHistory;
      await aiChatStream({
        history: cappedHistory as any, message: text, user_id: sessionUser.user_id,
        branch_id: DEVICE.branch_id, currency_exponent: DEVICE.currency_exponent,
        ui_context: getUiContext(),
      }, onEvent);
    } catch (e) {
      const currentId = assistantMsgIdRef.current;
      setMessages(prev => prev.map(m => m.id === currentId ? { ...m, role: "system" as const, text: `Error: ${String(e)}` } : m));
      liveToolCallsRef.current = []; setLiveToolCalls([]); setChatState("idle");
    }
  }, [input, chatState, history, addMessage, sessionUser, sessionId, getUiContext, onNavigate]);

  const handleConfirm = async () => {
    if (!pendingAction) return;
    const captured = pendingAction;
    setChatState("thinking"); setPendingAction(null);
    try {
      const result = await aiExecuteAction({
        action_id: captured.action_id, user_id: sessionUser.user_id,
        history: history as any, assistant_text: captured.assistant_text,
        currency_exponent: DEVICE.currency_exponent,
      }) as { followup: string; undo_id?: string | null };
      addMessage({ role: "assistant", text: result.followup });
      const messagesArray = messages as DisplayMessage[];
      const lastAssistantMsg = messagesArray.length > 0 ? messagesArray[messagesArray.length - 1] : null;
      const undoId = result.undo_id ?? undefined;
      if (lastAssistantMsg && undoId) {
        setMessages(prev => prev.map(m => m.id === lastAssistantMsg.id ? { ...m, undoId } : m));
      }
      setHistory(prev => [...prev, { role: "assistant", content: captured.assistant_text }, { role: "user", content: "Yes, please proceed." }, { role: "assistant", content: result.followup }]);
      setChatState("idle");
      setTimeout(() => { onMutationApplied(); }, 500);
    } catch (e) { addMessage({ role: "system", text: `Execution failed: ${String(e)}` }); setChatState("idle"); }
  };

  const handleCancel = async () => {
    if (!pendingAction) return;
    const capturedCancel = pendingAction;
    await aiCancelAction(capturedCancel.action_id, sessionUser.user_id).catch(() => {});
    addMessage({ role: "system", text: "Action cancelled." });
    setPendingAction(null); setChatState("idle");
  };

  const handleUndo = async (undoId: string, msgId: string) => {
    try {
      const result = await aiUndoAction(undoId, sessionUser.user_id, DEVICE.currency_exponent, sessionUser.user_id);
      setMessages(prev => prev.map(m => m.id === msgId ? { ...m, undoId: undefined } : m));
      addMessage({ role: "system", text: result.followup });
      setTimeout(() => { onMutationApplied(); }, 500);
    } catch (e) { addMessage({ role: "system", text: `Undo failed: ${String(e)}` }); }
  };

  const handleClear = useCallback(() => {
    clearAdminChat({
      branchId: DEVICE.branch_id, userId: sessionUser.user_id,
      newSessionId: () => crypto.randomUUID(),
      clearHistory: (bid, uid) => aiClearHistory(bid, uid),
      setMessages: () => setMessages([]), setHistory: () => setHistory([]),
      setSessionId: (v: string) => setSessionId(v),
    });
  }, [sessionUser]);

  const fetchKpi = useCallback(async (): Promise<KpiSnapshot> => {
    setKpi(prev => ({ ...prev, loading: true }));
    try {
      const { reportToday, inventoryGetLevels, syncStatus } = await import("../tauri/commands");
      const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
      const todaySummary = await reportToday(sessionUser.user_id, DEVICE.branch_id, today).catch(() => null);
      const levels = await inventoryGetLevels(sessionUser.user_id).catch(() => [] as import("../types").StockLevel[]);
      const syncStat = await syncStatus(sessionUser.user_id).catch(() => null);
      const low = levels.filter((l: { is_low_stock: boolean; is_out_of_stock: boolean }) => l.is_low_stock && !l.is_out_of_stock).length;
      const out = levels.filter((l: { is_out_of_stock: boolean }) => l.is_out_of_stock).length;
      const snap: KpiSnapshot = { loading: false, error: null, today: todaySummary, lowStockCount: low, outOfStockCount: out, sync: syncStat };
      setKpi(snap); return snap;
    } catch (e) { const err = String(e); setKpi(prev => ({ ...prev, loading: false, error: err })); return { ...kpi, loading: false, error: err }; }
  }, [sessionUser.user_id, kpi]);

  useEffect(() => { fetchKpi(); }, []); // eslint-disable-line

  return { messages, input, setInput, chatState, pendingAction, liveToolCalls,
    handleSend, handleConfirm, handleCancel, handleUndo, handleClear: handleClear as unknown as () => Promise<void>,
    history: history as ChatMessage[], fetchKpi, kpi };
}
