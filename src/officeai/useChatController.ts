import { Channel } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { clearAdminChat } from "../adminChatClear";
import type { DisplayMessage, KpiSnapshot } from "./officeAiTypes";
import type { ChatState, ToolCallEntry } from "./officeAiTypes";
import type { AiChatMessage, SessionUser, StreamEvent } from "../types";
import { aiSaveMessage, aiLoadHistory, aiClearHistory, aiChatStream, aiExecuteAction, aiCancelAction, aiUndoAction } from "../tauri/commands";
import type { ExecuteActionInput, ExecuteActionResult } from "../tauri/commands";
import { DEVICE } from "../types";

let _msgIdCounter = 1;

export interface ChatControllerOpts {
  sessionUser: SessionUser;
  getUiContext: () => string;
  onNavigate: (tab: string) => void;
  onMutationApplied: () => void;
  fetchKpi: () => Promise<KpiSnapshot>;
}

export interface ChatController {
  messages: DisplayMessage[];
  input: string;
  setInput: (v: string) => void;
  chatState: ChatState;
  pendingAction: DisplayMessage["pendingAction"] | null;
  setPendingAction: (a: DisplayMessage["pendingAction"] | null) => void;
  liveToolCalls: ToolCallEntry[];
  handleSend: (overrideText?: string) => Promise<void>;
  handleConfirm: () => Promise<void>;
  handleCancel: () => Promise<void>;
  handleUndo: (undoId: string) => Promise<void>;
  handleClear: () => Promise<void>;
  history: AiChatMessage[];
  configProvider: string | null;
  setConfigProvider: (v: string | null) => void;
}

export function useChatController(opts: ChatControllerOpts): ChatController {
  const { sessionUser, getUiContext, onNavigate, onMutationApplied, fetchKpi } = opts;
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [chatState, setChatState] = useState<ChatState>("idle");
  const [history, setHistory] = useState<AiChatMessage[]>([]);
  const [pendingAction, setPendingAction] = useState<DisplayMessage["pendingAction"] | null>(null);
  const [liveToolCalls, setLiveToolCalls] = useState<ToolCallEntry[]>([]);
  const [configProvider, setConfigProvider] = useState<string | null>(null);
  const sessionIdRef = useRef(crypto.randomUUID());
  const channelRef = useRef<Channel<StreamEvent> | null>(null);
  const abortRef = useRef<(() => void) | null>(null);
  const hasLoadedHistoryRef = useRef(false);

  const addMessage = useCallback((m: { role: "user" | "assistant" | "system"; text: string; toolCalls?: ToolCallEntry[]; pendingAction?: DisplayMessage["pendingAction"]; undoId?: string }) => {
    const id = String(++_msgIdCounter);
    setMessages(prev => [...prev, { ...m, id, timestamp: new Date() }]);
    return id;
  }, []);

  // Load history on first mount
  useEffect(() => {
    if (hasLoadedHistoryRef.current) return;
    hasLoadedHistoryRef.current = true;
    aiLoadHistory(DEVICE.branch_id, sessionUser.user_id).then(loaded => {
      if (loaded.length === 0) return;
      setMessages(loaded.filter(m => m.role === "user" || m.role === "assistant").map(m => ({
        id: String(++_msgIdCounter), role: m.role as "user" | "assistant",
        text: typeof m.content === "string" ? m.content : JSON.stringify(m.content),
        timestamp: new Date(m.created_at ?? Date.now()),
      })));
      setHistory(loaded.map(m => ({ role: m.role as "user" | "assistant", content: typeof m.content === "string" ? m.content : JSON.stringify(m.content) })));
    }).catch(() => {});
  }, [sessionUser.user_id]);

  const handleSend = useCallback(async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    addMessage({ role: "user", text });
    aiSaveMessage(sessionIdRef.current, DEVICE.branch_id, sessionUser.user_id, "user", text, "text").catch(() => {});
    const rawHistory: AiChatMessage[] = [...history, { role: "user", content: text }].slice(-60);

    setChatState("thinking");
    setLiveToolCalls([]);

    const ch = new Channel<StreamEvent>();
    channelRef.current = ch;
    let aborted = false;
    abortRef.current = () => { aborted = true; };

    let assistantText = "";
    let msgId = "";
    let toolCallMap = new Map<string, ToolCallEntry>();

    ch.onmessage = (ev: StreamEvent) => {
      if (aborted) return;
      switch (ev.type) {
        case "text_delta":
          assistantText += ev.text;
          if (!msgId) msgId = addMessage({ role: "assistant", text: ev.text });
          else setMessages(prev => prev.map(m => m.id === msgId ? { ...m, text: assistantText } : m));
          break;
        case "tool_start":
          toolCallMap.set(ev.id, { id: ev.id, name: ev.name, startTime: Date.now(), status: "running" });
          setLiveToolCalls(Array.from(toolCallMap.values()));
          break;
        case "tool_done":
          const tc = toolCallMap.get(ev.id);
          if (tc) { tc.status = "done"; tc.duration = Date.now() - tc.startTime; }
          setLiveToolCalls(Array.from(toolCallMap.values()));
          break;
        case "navigate":
          onNavigate(ev.tab);
          break;
        case "tool_result":
          break;
        case "stream_end":
          if (assistantText) {
            aiSaveMessage(sessionIdRef.current, DEVICE.branch_id, sessionUser.user_id, "assistant", assistantText, "text").catch(() => {});
            setHistory(prev => [...prev, { role: "assistant", content: assistantText }].slice(-60));
          }
          setChatState("idle"); setLiveToolCalls([]);
          break;
        case "stream_error":
          addMessage({ role: "system", text: ev.error });
          setChatState("idle"); setLiveToolCalls([]);
          break;
        case "action_required":
          setPendingAction({
            action_id: ev.action_id, tool_name: ev.tool_name,
            preview: ev.preview, expires_at: ev.expires_at, assistant_text: assistantText,
          });
          setChatState("confirm");
          break;
        default: break;
      }
    };

    try {
      await aiChatStream(sessionUser.user_id, sessionIdRef.current, DEVICE.branch_id, {
        user_message: text, history: rawHistory,
        ui_context: getUiContext(),
      }, ch);
    } catch (e: unknown) {
      if (!aborted) {
        addMessage({ role: "system", text: String(e) });
        setChatState("idle");
      }
    }
    abortRef.current = null;
    channelRef.current = null;
  }, [input, chatState, history, addMessage, sessionUser, getUiContext, onNavigate]);

  const handleConfirm = useCallback(async () => {
    if (!pendingAction) return;
    const captured = pendingAction;
    setPendingAction(null); setChatState("thinking");
    try {
      const result = await aiExecuteAction({
        action_id: captured.action_id, confirmed: true,
        actor_user_id: sessionUser.user_id,
      } as ExecuteActionInput);
      if (result.undo_id) {
        setMessages(prev => prev.map(m => m.id === captured.action_id ? { ...m, undoId: result.undo_id } : m));
      }
      onMutationApplied();
    } catch (e: unknown) {
      addMessage({ role: "system", text: String(e) });
    } finally { setChatState("idle"); }
  }, [pendingAction, addMessage, sessionUser, onMutationApplied]);

  const handleCancel = useCallback(async () => {
    const captured = pendingAction;
    setPendingAction(null); setChatState("idle");
    if (captured) {
      await aiCancelAction(captured.action_id, sessionUser.user_id).catch(() => {});
    }
  }, [pendingAction, sessionUser]);

  const handleUndo = useCallback(async (undoId: string) => {
    try {
      const result = await aiUndoAction(undoId, sessionUser.user_id, DEVICE.currency_exponent, sessionUser.user_id);
      if (result.undone) {
        addMessage({ role: "system", text: "✓ Undone." });
        onMutationApplied();
      }
    } catch (e: unknown) { addMessage({ role: "system", text: String(e) }); }
  }, [addMessage, sessionUser, onMutationApplied]);

  const handleClear = useCallback(async () => {
    setMessages([]); setHistory([]);
    sessionIdRef.current = crypto.randomUUID();
    await clearAdminChat(aiClearHistory, sessionUser.user_id);
  }, [sessionUser]);

  return {
    messages, input, setInput, chatState, pendingAction, setPendingAction,
    liveToolCalls, handleSend, handleConfirm, handleCancel, handleUndo, handleClear,
    history, configProvider, setConfigProvider,
  };
}
