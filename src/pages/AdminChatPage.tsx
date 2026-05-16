import React, { useState, useRef, useEffect } from "react";
import type {
  SessionUser,
  ChatMessage,
  AiChatResponse,
  ToolPreview,
  ProviderConfig,
  ModelInfo,
} from "../types";
import { DEVICE } from "../types";
import {
  adminGetProviderConfig,
  adminSetAnthropic,
  adminValidateOpenai,
  adminSetOpenai,
  adminSetupSupabase,
  adminGetSupabaseStatus,
  aiChat,
  aiExecuteAction,
  aiCancelAction,
  aiUndoAction,
} from "../tauri/commands";
import ConfirmActionModal from "../components/ConfirmActionModal";

interface Props {
  sessionUser: SessionUser;
  onBackToPOS: () => void;
}

interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
  pendingAction?: {
    action_id: string;
    tool_name: string;
    preview: ToolPreview;
    expires_at: string;
    assistant_text: string;
  };
  undoId?: string;
}

type ChatState = "idle" | "thinking" | "confirm";

// ── Setup wizard state machine ────────────────────────────────────────────────
type SetupStep =
  | "loading"
  | "sync_setup"           // enter Supabase URL + service key + PAT (first run only)
  | "sync_migrating"       // running schema migration
  | "settings"             // gear icon → settings hub (sync + AI provider tabs)
  | "pick_provider"        // choose Anthropic or OpenAI-compatible
  | "anthropic_key"        // enter Anthropic API key
  | "openai_url_key"       // enter base URL + API key
  | "openai_validating"    // calling /models endpoint
  | "openai_pick_model"    // choose from model list
  | "done";                // provider is configured, show chat

export default function AdminChatPage({ sessionUser, onBackToPOS }: Props) {
  const [setupStep, setSetupStep] = useState<SetupStep>("loading");
  const [config, setConfig] = useState<ProviderConfig | null>(null);

  // Supabase sync setup
  const [supabaseUrl, setSupabaseUrl] = useState("");
  const [supabaseKey, setSupabaseKey] = useState("");
  const [supabasePat, setSupabasePat] = useState("");
  const [supabaseError, setSupabaseError] = useState("");
  const [supabaseMigrating, setSupabaseMigrating] = useState(false);
  const [settingsTab, setSettingsTab] = useState<"sync" | "ai">("sync");

  // Anthropic setup
  const [anthropicKey, setAnthropicKey] = useState("");
  const [savingAnthropic, setSavingAnthropic] = useState(false);
  const [anthropicError, setAnthropicError] = useState("");

  // OpenAI setup
  const [openaiBaseUrl, setOpenaiBaseUrl] = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey] = useState("");
  const [validating, setValidating] = useState(false);
  const [validateError, setValidateError] = useState("");
  const [modelList, setModelList] = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [savingOpenai, setSavingOpenai] = useState(false);

  // Chat state
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [chatState, setChatState] = useState<ChatState>("idle");
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [pendingAction, setPendingAction] = useState<DisplayMessage["pendingAction"] | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Check Supabase config first, then AI provider
    Promise.all([
      adminGetSupabaseStatus().catch(() => ({ configured: false })),
      adminGetProviderConfig().catch(() => null),
    ]).then(([supaStatus, cfg]) => {
      if (cfg) {
        setConfig(cfg);
        if (cfg.openai_base_url) setOpenaiBaseUrl(cfg.openai_base_url);
      }
      if (!supaStatus.configured) {
        // First run: show Supabase setup before AI provider
        setSetupStep("sync_setup");
      } else if (!cfg?.provider) {
        setSetupStep("pick_provider");
      } else {
        setSetupStep("done");
      }
    });
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const addMessage = (msg: Omit<DisplayMessage, "id">): DisplayMessage => {
    const full = { ...msg, id: crypto.randomUUID() };
    setMessages(prev => [...prev, full]);
    return full;
  };

  // ── Anthropic save ────────────────────────────────────────────────────────────
  const handleSaveAnthropic = async () => {
    if (!anthropicKey.trim()) return;
    setSavingAnthropic(true);
    setAnthropicError("");
    try {
      await adminSetAnthropic(anthropicKey.trim());
      const cfg = await adminGetProviderConfig();
      setConfig(cfg);
      setSetupStep("done");
    } catch (e) {
      setAnthropicError(String(e));
    } finally {
      setSavingAnthropic(false);
    }
  };

  // ── OpenAI validate ───────────────────────────────────────────────────────────
  const handleValidateOpenai = async () => {
    if (!openaiBaseUrl.trim() || !openaiKey.trim()) return;
    setValidating(true);
    setValidateError("");
    setModelList([]);
    setSetupStep("openai_validating");
    try {
      const result = await adminValidateOpenai(openaiBaseUrl.trim(), openaiKey.trim());
      if (result.success && result.models.length > 0) {
        setModelList(result.models);
        setSelectedModel(result.models[0].id);
        setSetupStep("openai_pick_model");
      } else if (result.success) {
        setValidateError("Connection successful but no models were returned. Enter the model name manually.");
        setModelList([]);
        setSelectedModel("");
        setSetupStep("openai_pick_model");
      } else {
        setValidateError(result.error ?? "Validation failed");
        setSetupStep("openai_url_key");
      }
    } catch (e) {
      setValidateError(String(e));
      setSetupStep("openai_url_key");
    } finally {
      setValidating(false);
    }
  };

  // ── OpenAI save ───────────────────────────────────────────────────────────────
  const handleSaveOpenai = async () => {
    if (!selectedModel.trim()) return;
    setSavingOpenai(true);
    try {
      await adminSetOpenai(openaiBaseUrl.trim(), openaiKey.trim(), selectedModel.trim());
      const cfg = await adminGetProviderConfig();
      setConfig(cfg);
      setSetupStep("done");
    } catch (e) {
      setValidateError(String(e));
    } finally {
      setSavingOpenai(false);
    }
  };

  // ── Supabase setup ────────────────────────────────────────────────────────────
  const handleSetupSupabase = async () => {
    if (!supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()) return;
    setSupabaseError("");
    setSupabaseMigrating(true);
    setSetupStep("sync_migrating");
    try {
      await adminSetupSupabase(supabaseUrl.trim(), supabaseKey.trim(), supabasePat.trim());
      // Clear PAT from state immediately after use
      setSupabasePat("");
      // Continue to AI provider setup
      const cfg = await adminGetProviderConfig().catch(() => null);
      if (cfg) setConfig(cfg);
      setSetupStep(cfg?.provider ? "done" : "pick_provider");
    } catch (e) {
      setSupabaseError(String(e));
      setSetupStep("sync_setup");
    } finally {
      setSupabaseMigrating(false);
    }
  };

  // ── Chat send ─────────────────────────────────────────────────────────────────
  const handleSend = async () => {
    const text = input.trim();
    if (!text || chatState !== "idle") return;
    setInput("");
    addMessage({ role: "user", text });
    const newHistory: ChatMessage[] = [...history, { role: "user", content: text }];
    setHistory(newHistory);
    setChatState("thinking");

    try {
      const resp: AiChatResponse = await aiChat({
        history,
        message: text,
        user_id: sessionUser.user_id,
        branch_id: DEVICE.branch_id,
        currency_exponent: DEVICE.currency_exponent,
      });

      if (resp.type === "no_api_key") {
        addMessage({ role: "system", text: "No provider configured. Please set up an AI provider." });
        const cfg = await adminGetProviderConfig().catch(() => null);
        if (cfg) setConfig(cfg);
        setSetupStep("pick_provider");
        setChatState("idle");
        return;
      }

      if (resp.type === "message") {
        addMessage({ role: "assistant", text: resp.content });
        setHistory(prev => [...prev, { role: "assistant", content: resp.content }]);
        setChatState("idle");
        return;
      }

      if (resp.type === "pending_action") {
        const actionData = {
          action_id: resp.action_id,
          tool_name: resp.tool_name,
          preview: resp.preview,
          expires_at: resp.expires_at,
          assistant_text: resp.assistant_text,
        };
        addMessage({
          role: "assistant",
          text: resp.assistant_text || "I'd like to make the following change:",
          pendingAction: actionData,
        });
        setPendingAction(actionData);
        setChatState("confirm");
        return;
      }
    } catch (e) {
      addMessage({ role: "system", text: `Error: ${String(e)}` });
      setChatState("idle");
    }
  };

  const handleConfirm = async () => {
    if (!pendingAction) return;
    setChatState("thinking");
    setPendingAction(null);
    try {
      const result = await aiExecuteAction({
        action_id: pendingAction.action_id,
        user_id: sessionUser.user_id,
        history,
        assistant_text: pendingAction.assistant_text,
        currency_exponent: DEVICE.currency_exponent,
      });
      addMessage({
        role: "assistant",
        text: result.followup,
        undoId: result.undo_id ?? undefined,
      });
      setHistory(prev => [
        ...prev,
        { role: "assistant", content: pendingAction.assistant_text },
        { role: "assistant", content: result.followup },
      ]);
      setChatState("idle");
    } catch (e) {
      addMessage({ role: "system", text: `Execution failed: ${String(e)}` });
      setChatState("idle");
    }
  };

  const handleCancel = async () => {
    if (!pendingAction) return;
    await aiCancelAction(pendingAction.action_id).catch(() => {});
    addMessage({ role: "system", text: "Action cancelled." });
    setPendingAction(null);
    setChatState("idle");
  };

  const handleUndo = async (undoId: string, msgId: string) => {
    try {
      const result = await aiUndoAction(undoId, sessionUser.user_id, DEVICE.currency_exponent);
      setMessages(prev => prev.map(m => m.id === msgId ? { ...m, undoId: undefined } : m));
      addMessage({ role: "system", text: result.followup });
    } catch (e) {
      addMessage({ role: "system", text: `Undo failed: ${String(e)}` });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); }
  };

  const providerLabel = config?.provider === "anthropic"
    ? "Anthropic (Claude)"
    : config?.provider === "openai"
    ? `OpenAI-compatible · ${config.openai_model}`
    : "";

  // ── Render: sync setup (first-run) ───────────────────────────────────────────
  if (setupStep === "sync_setup" || setupStep === "sync_migrating") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Store Setup — Sync" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <h2>Connect to Supabase</h2>
            <p className="setup-subtitle">
              ZanPOS uses <strong>Supabase</strong> to sync sales and catalog data across devices.
              All credentials are stored locally. The migration token is used once and discarded.
            </p>

            <label className="setup-label">Supabase Project URL
              <input type="text" className="setup-input"
                placeholder="https://xyz.supabase.co"
                value={supabaseUrl}
                onChange={e => setSupabaseUrl(e.target.value)}
                disabled={supabaseMigrating}
              />
            </label>
            <p className="setup-hint">Found in: Supabase Dashboard → Settings → API → Project URL</p>

            <label className="setup-label">Service Role Key (Secret)
              <input type="password" className="setup-input"
                placeholder="eyJhbGciOi…"
                value={supabaseKey}
                onChange={e => setSupabaseKey(e.target.value)}
                disabled={supabaseMigrating}
              />
            </label>
            <p className="setup-hint">Found in: Supabase Dashboard → Settings → API → service_role (secret key)</p>

            <label className="setup-label">Personal Access Token <span className="setup-hint-inline">(used once, never stored)</span>
              <input type="password" className="setup-input"
                placeholder="sbp_…"
                value={supabasePat}
                onChange={e => setSupabasePat(e.target.value)}
                disabled={supabaseMigrating}
                onKeyDown={e => e.key === "Enter" && handleSetupSupabase()}
              />
            </label>
            <p className="setup-hint">
              Generate at: <strong>supabase.com/dashboard/account/tokens</strong> — used once to create central tables, then discarded.
            </p>

            {supabaseError && <p className="setup-error">{supabaseError}</p>}
            {supabaseMigrating && <p className="setup-migrating">Creating central tables… this takes a few seconds.</p>}

            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")} disabled={supabaseMigrating}>
                Skip for now
              </button>
              <button className="btn-primary"
                onClick={handleSetupSupabase}
                disabled={supabaseMigrating || !supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()}
              >
                {supabaseMigrating ? "Migrating…" : "Connect & Set Up Database"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: settings hub (gear icon) ─────────────────────────────────────────
  if (setupStep === "settings") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Settings" user={sessionUser.display_name} onBack={() => setSetupStep("done")} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <div className="settings-tabs">
              <button
                className={`settings-tab ${settingsTab === "sync" ? "settings-tab-active" : ""}`}
                onClick={() => setSettingsTab("sync")}
              >☁ Sync</button>
              <button
                className={`settings-tab ${settingsTab === "ai" ? "settings-tab-active" : ""}`}
                onClick={() => setSettingsTab("ai")}
              >◆ AI Provider</button>
            </div>

            {settingsTab === "sync" && (
              <div>
                <h2>Supabase Sync</h2>
                <p className="setup-subtitle">Update connection credentials. Re-running migration is safe (idempotent).</p>
                <label className="setup-label">Supabase Project URL
                  <input type="text" className="setup-input"
                    placeholder="https://xyz.supabase.co"
                    value={supabaseUrl}
                    onChange={e => setSupabaseUrl(e.target.value)}
                    disabled={supabaseMigrating}
                  />
                </label>
                <label className="setup-label">Service Role Key
                  <input type="password" className="setup-input"
                    placeholder="eyJhbGciOi…"
                    value={supabaseKey}
                    onChange={e => setSupabaseKey(e.target.value)}
                    disabled={supabaseMigrating}
                  />
                </label>
                <label className="setup-label">Personal Access Token <span className="setup-hint-inline">(re-run migration, optional)</span>
                  <input type="password" className="setup-input"
                    placeholder="sbp_… (leave blank to update keys only)"
                    value={supabasePat}
                    onChange={e => setSupabasePat(e.target.value)}
                    disabled={supabaseMigrating}
                  />
                </label>
                {supabaseError && <p className="setup-error">{supabaseError}</p>}
                {supabaseMigrating && <p className="setup-migrating">Updating sync configuration…</p>}
                <div className="setup-actions">
                  <button className="btn-secondary" onClick={() => setSetupStep("done")}>Cancel</button>
                  <button className="btn-primary"
                    onClick={handleSetupSupabase}
                    disabled={supabaseMigrating || !supabaseUrl.trim() || !supabaseKey.trim() || !supabasePat.trim()}
                  >
                    {supabaseMigrating ? "Saving…" : "Save & Re-Migrate"}
                  </button>
                </div>
              </div>
            )}

            {settingsTab === "ai" && (
              <div>
                <h2>AI Provider</h2>
                <p className="setup-subtitle">Current: <strong>{providerLabel || "Not configured"}</strong></p>
                <div className="setup-actions" style={{ marginTop: "1rem" }}>
                  <button className="btn-secondary" onClick={() => setSetupStep("done")}>Cancel</button>
                  <button className="btn-primary" onClick={() => setSetupStep("pick_provider")}>Change Provider</button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── Render: loading ───────────────────────────────────────────────────────────
  if (setupStep === "loading") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Admin Chat" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center"><span className="setup-loading">Loading…</span></div>
      </div>
    );
  }

  // ── Render: pick provider ─────────────────────────────────────────────────────
  if (setupStep === "pick_provider") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Admin Chat — Setup" user={sessionUser.display_name} onBack={onBackToPOS} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>Choose AI Provider</h2>
            <p className="setup-subtitle">Select the AI service to power the admin assistant.</p>
            <div className="provider-choice-grid">
              <button className="provider-choice-btn" onClick={() => setSetupStep("anthropic_key")}>
                <span className="provider-choice-icon">◆</span>
                <span className="provider-choice-name">Anthropic</span>
                <span className="provider-choice-desc">Claude models — claude-sonnet-4-6</span>
              </button>
              <button className="provider-choice-btn" onClick={() => setSetupStep("openai_url_key")}>
                <span className="provider-choice-icon">⬡</span>
                <span className="provider-choice-name">OpenAI Compatible</span>
                <span className="provider-choice-desc">OpenAI, Groq, Ollama, LM Studio, vLLM, and any v1/chat/completions endpoint</span>
              </button>
            </div>
            {config?.provider && (
              <button className="setup-skip-btn" onClick={() => setSetupStep("done")}>
                Keep current: {providerLabel}
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── Render: Anthropic key entry ───────────────────────────────────────────────
  if (setupStep === "anthropic_key") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Admin Chat — Anthropic Setup" user={sessionUser.display_name} onBack={() => setSetupStep("pick_provider")} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>Anthropic API Key</h2>
            <p className="setup-subtitle">Enter your API key from <strong>console.anthropic.com</strong>. The model is fixed to <code>claude-sonnet-4-6</code>.</p>
            <label className="setup-label">API Key
              <input
                type="password"
                className="setup-input"
                placeholder="sk-ant-api03-…"
                value={anthropicKey}
                onChange={e => setAnthropicKey(e.target.value)}
                onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()}
                autoFocus
              />
            </label>
            {anthropicError && <p className="setup-error">{anthropicError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")}>Back</button>
              <button
                className="btn-primary"
                onClick={handleSaveAnthropic}
                disabled={savingAnthropic || !anthropicKey.trim()}
              >
                {savingAnthropic ? "Saving…" : "Save & Continue"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: OpenAI URL + key entry ────────────────────────────────────────────
  if (setupStep === "openai_url_key" || setupStep === "openai_validating") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Admin Chat — OpenAI Setup" user={sessionUser.display_name} onBack={() => setSetupStep("pick_provider")} />
        <div className="setup-center">
          <div className="setup-card">
            <h2>OpenAI-Compatible Provider</h2>
            <p className="setup-subtitle">Enter the base URL and API key for any OpenAI-compatible endpoint.</p>
            <label className="setup-label">Base URL
              <input
                type="text"
                className="setup-input"
                placeholder="https://api.openai.com/v1"
                value={openaiBaseUrl}
                onChange={e => setOpenaiBaseUrl(e.target.value)}
                disabled={validating}
              />
            </label>
            <div className="setup-url-examples">
              <span>Examples:</span>
              {["https://api.openai.com/v1", "https://api.groq.com/openai/v1", "http://localhost:11434/v1", "http://localhost:1234/v1"].map(u => (
                <button key={u} className="setup-url-chip" onClick={() => setOpenaiBaseUrl(u)} disabled={validating}>
                  {u.replace("https://", "").replace("http://", "").split("/")[0]}
                </button>
              ))}
            </div>
            <label className="setup-label">API Key
              <input
                type="password"
                className="setup-input"
                placeholder="sk-… or any token your provider requires"
                value={openaiKey}
                onChange={e => setOpenaiKey(e.target.value)}
                disabled={validating}
                onKeyDown={e => e.key === "Enter" && handleValidateOpenai()}
              />
            </label>
            {validateError && <p className="setup-error">{validateError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("pick_provider")} disabled={validating}>Back</button>
              <button
                className="btn-primary"
                onClick={handleValidateOpenai}
                disabled={validating || !openaiBaseUrl.trim() || !openaiKey.trim()}
              >
                {validating ? "Connecting…" : "Connect & Fetch Models"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: OpenAI model picker ───────────────────────────────────────────────
  if (setupStep === "openai_pick_model") {
    return (
      <div className="admin-chat-page">
        <TopBar label="Admin Chat — Select Model" user={sessionUser.display_name} onBack={() => setSetupStep("openai_url_key")} />
        <div className="setup-center">
          <div className="setup-card setup-card-wide">
            <h2>Select a Model</h2>
            <p className="setup-subtitle">
              {modelList.length > 0
                ? `${modelList.length} model${modelList.length !== 1 ? "s" : ""} available from ${openaiBaseUrl.replace(/https?:\/\//, "").split("/")[0]}`
                : "No models listed — enter a model name manually."}
            </p>

            {modelList.length > 0 ? (
              <div className="model-list">
                {modelList.map(m => (
                  <button
                    key={m.id}
                    className={`model-list-item ${selectedModel === m.id ? "model-list-item-active" : ""}`}
                    onClick={() => setSelectedModel(m.id)}
                  >
                    <span className="model-list-id">{m.id}</span>
                    {selectedModel === m.id && <span className="model-list-check">✓</span>}
                  </button>
                ))}
              </div>
            ) : (
              <label className="setup-label">Model name
                <input
                  type="text"
                  className="setup-input"
                  placeholder="gpt-4o, llama3, mistral, …"
                  value={selectedModel}
                  onChange={e => setSelectedModel(e.target.value)}
                  autoFocus
                />
              </label>
            )}

            {validateError && <p className="setup-error">{validateError}</p>}
            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setSetupStep("openai_url_key")}>Back</button>
              <button
                className="btn-primary"
                onClick={handleSaveOpenai}
                disabled={savingOpenai || !selectedModel.trim()}
              >
                {savingOpenai ? "Saving…" : "Use This Model"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ── Render: chat (done state) ─────────────────────────────────────────────────
  return (
    <div className="admin-chat-page">
      <div className="admin-chat-topbar">
        <button className="topbar-btn" onClick={onBackToPOS}>← POS</button>
        <span className="admin-chat-title">Admin Chat</span>
        <span className="admin-chat-provider-badge">{providerLabel}</span>
        <button
          className="topbar-btn topbar-btn-sm"
          onClick={() => { setSettingsTab("sync"); setSetupStep("settings"); }}
          title="Sync & AI settings"
        >
          ⚙
        </button>
        <span className="admin-chat-user">{sessionUser.display_name}</span>
      </div>

      <div className="chat-messages">
        {messages.length === 0 && (
          <div className="chat-welcome">
            <p>Hello, {sessionUser.display_name}. I can help you:</p>
            <ul>
              <li>View today's sales summary</li>
              <li>Look up and manage products</li>
              <li>Update prices, names, and availability</li>
            </ul>
            <p className="chat-welcome-hint">All changes require your confirmation before executing.</p>
          </div>
        )}

        {messages.map(msg => (
          <ChatBubble
            key={msg.id}
            msg={msg}
            onUndo={msg.undoId ? () => handleUndo(msg.undoId!, msg.id) : undefined}
          />
        ))}

        {chatState === "thinking" && (
          <div className="chat-bubble assistant thinking">
            <span className="thinking-dots"><span>.</span><span>.</span><span>.</span></span>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {chatState === "confirm" && pendingAction && (
        <ConfirmActionModal
          preview={pendingAction.preview}
          onConfirm={handleConfirm}
          onCancel={handleCancel}
        />
      )}

      <div className="chat-input-area">
        <textarea
          className="chat-input"
          placeholder="Ask anything about your business…"
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={chatState !== "idle"}
          rows={1}
        />
        <button
          className="chat-send-btn"
          onClick={handleSend}
          disabled={!input.trim() || chatState !== "idle"}
        >
          Send
        </button>
      </div>
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function TopBar({ label, user, onBack }: { label: string; user: string; onBack: () => void }) {
  return (
    <div className="admin-chat-topbar">
      <button className="topbar-btn" onClick={onBack}>← Back</button>
      <span className="admin-chat-title">{label}</span>
      <span className="admin-chat-user">{user}</span>
    </div>
  );
}

function ChatBubble({ msg, onUndo }: { msg: DisplayMessage; onUndo?: () => void }) {
  return (
    <div className={`chat-bubble ${msg.role}`}>
      <div className="chat-bubble-text">{msg.text}</div>
      {msg.pendingAction && <div className="chat-pending-badge">Waiting for confirmation…</div>}
      {onUndo && (
        <button className="chat-undo-btn" onClick={onUndo}>Undo this change</button>
      )}
    </div>
  );
}
