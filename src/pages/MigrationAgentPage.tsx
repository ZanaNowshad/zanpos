import React, { useState, useRef, useEffect } from "react";
import type { MigrationContext, MigrationChatResponse, MigrationPreview } from "../types";
import {
  migrationAgentChat, migrationConfirmExecute,
  adminGetProviderConfig, adminSetAnthropic,
  adminValidateOpenai, adminSetOpenai,
} from "../tauri/commands";
import type { ModelInfo } from "../types";
import { open as openFilePicker } from "@tauri-apps/plugin-dialog";

// ─── Types ────────────────────────────────────────────────────────────────────

type MigrationPhase = "discovery" | "connected" | "mapping" | "preview" | "executing" | "done";

interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
}

interface Props {
  onDone: () => void;
}

// ─── Phase config ──────────────────────────────────────────────────────────────

const PHASES: { id: MigrationPhase; label: string }[] = [
  { id: "discovery", label: "Discovery" },
  { id: "connected", label: "Connect" },
  { id: "mapping",   label: "Mapping" },
  { id: "preview",   label: "Preview" },
  { id: "executing", label: "Confirm" },
  { id: "done",      label: "Done" },
];

const PHASE_ORDER: MigrationPhase[] = ["discovery", "connected", "mapping", "preview", "executing", "done"];

// ─── Initial welcome message ──────────────────────────────────────────────────

const WELCOME: DisplayMessage = {
  id: "welcome",
  role: "assistant",
  text: `👋 Welcome to the **ZANPOS Migration Agent**!

I'll help you import your existing POS data into ZANPOS. Here's what I can migrate:
- 📦 Products & Categories
- 👤 Customers
- 🛒 Sales history
- 📦 Stock levels / Inventory

I support **SQLite**, **MySQL/MariaDB**, **SQL Server**, and **CSV/Excel** files.

To get started, tell me about your old POS system:
- What software were you using? (e.g., "Square POS", "custom SQLite database")
- Is the old POS software currently running on this computer?
- Or if you know the database file path, just paste it here.

Type your answer below, or attach a CSV/Excel file using the 📎 button.`,
};

// ─── Simple markdown renderer ─────────────────────────────────────────────────

function SimpleMarkdown({ text }: { text: string }) {
  // Very lightweight: bold (**text**), inline code (`code`), newlines → <br>
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  const rendered = parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          style={{ background: "var(--border)", padding: "1px 4px", borderRadius: 3, fontSize: "0.85em" }}
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    // Split on newlines and insert <br>
    return part.split("\n").map((line, j, arr) => (
      <React.Fragment key={`${i}-${j}`}>
        {line}
        {j < arr.length - 1 && <br />}
      </React.Fragment>
    ));
  });
  return <>{rendered}</>;
}

// ─── Migration Preview Card ───────────────────────────────────────────────────

function MigrationPreviewCard({
  preview,
  description,
  onConfirm,
  executing,
}: {
  preview: MigrationPreview;
  description: string;
  onConfirm: () => void;
  executing: boolean;
}) {
  const hasSkipped = preview.tables.some((t) => t.skipped > 0);
  return (
    <div className="mig-preview-card">
      <div className="mig-preview-title">🗂️ Migration Preview</div>
      {description && (
        <p style={{ margin: "0 0 10px", fontSize: "0.85rem", color: "var(--text-dim)" }}>
          {description}
        </p>
      )}

      <table className="mig-preview-table">
        <thead>
          <tr>
            <th>Table</th>
            <th style={{ textAlign: "right" }}>Rows</th>
            {hasSkipped && <th style={{ textAlign: "right" }}>Skipped</th>}
          </tr>
        </thead>
        <tbody>
          {preview.tables.map((t, i) => (
            <tr key={i}>
              <td>{t.target_table}</td>
              <td style={{ textAlign: "right", fontWeight: 600 }}>{t.rows}</td>
              {hasSkipped && (
                <td style={{ textAlign: "right", color: "#f59e0b" }}>
                  {t.skipped > 0 ? t.skipped : "—"}
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>

      <div className="mig-preview-total">
        Total: {preview.total_rows} row{preview.total_rows !== 1 ? "s" : ""} to import
      </div>

      {preview.warnings.map((w, i) => (
        <div key={i} className="mig-warning-badge">
          ⚠️ {w}
        </div>
      ))}

      <button className="mig-confirm-btn" onClick={onConfirm} disabled={executing}>
        {executing ? "⏳ Migrating…" : "✅ CONFIRM — Execute Migration"}
      </button>
      <p style={{ margin: "8px 0 0", fontSize: "0.75rem", color: "var(--text-dim)", textAlign: "center" }}>
        This will INSERT data into your ZANPOS database. Existing data is not modified.
      </p>
    </div>
  );
}

// ─── AI Config Setup Panel ────────────────────────────────────────────────────

type AiProvider = "anthropic" | "openai";

function AiSetupPanel({ onConfigured }: { onConfigured: () => void }) {
  const [provider, setProvider]         = useState<AiProvider>("anthropic");
  // Anthropic
  const [anthropicKey, setAnthropicKey] = useState("");
  // OpenAI
  const [baseUrl, setBaseUrl]           = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]       = useState("");
  const [models, setModels]             = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating]     = useState(false);
  // Shared
  const [saving, setSaving]             = useState(false);
  const [error, setError]               = useState<string | null>(null);

  // ── Anthropic save ────────────────────────────────────────────────────────
  const handleSaveAnthropic = async () => {
    const key = anthropicKey.trim();
    if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
    setSaving(true); setError(null);
    try { await adminSetAnthropic(key); onConfigured(); }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  // ── OpenAI: validate → fetch models ───────────────────────────────────────
  const handleValidateOpenai = async () => {
    setError(null); setModels([]); setSelectedModel("");
    setValidating(true);
    try {
      const res = await adminValidateOpenai(baseUrl.trim(), openaiKey.trim());
      if (res.success && res.models.length > 0) {
        setModels(res.models);
        setSelectedModel(res.models[0].id);
      } else {
        setError(res.error ?? "Validation failed — check URL and key");
      }
    } catch (e) { setError(String(e)); }
    finally { setValidating(false); }
  };

  // ── OpenAI save ────────────────────────────────────────────────────────────
  const handleSaveOpenai = async () => {
    if (!selectedModel) return;
    setSaving(true); setError(null);
    try {
      await adminSetOpenai(baseUrl.trim(), openaiKey.trim(), selectedModel);
      onConfigured();
    } catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const card: React.CSSProperties = {
    maxWidth: 500, width: "100%", padding: 32,
    background: "var(--surface)", borderRadius: 12,
    border: "1px solid var(--border)", margin: 24,
  };
  const inp: React.CSSProperties = {
    width: "100%", padding: "9px 12px", borderRadius: 7,
    border: "1px solid var(--border)", background: "var(--bg)",
    color: "var(--text)", fontSize: "0.875rem",
    boxSizing: "border-box", marginBottom: 12,
  };
  const lbl: React.CSSProperties = {
    display: "block", fontSize: "0.8rem", fontWeight: 600,
    marginBottom: 5, color: "var(--text-dim)",
  };

  return (
    <div className="mig-page" style={{ alignItems: "center", justifyContent: "center" }}>
      <div style={card}>
        <div style={{ fontSize: "2rem", marginBottom: 10 }}>🤖</div>
        <h2 style={{ margin: "0 0 6px", fontSize: "1.1rem", fontWeight: 700 }}>Set up AI before migrating</h2>
        <p style={{ margin: "0 0 20px", fontSize: "0.85rem", color: "var(--text-dim)", lineHeight: 1.5 }}>
          Choose your AI provider. Keys are stored securely on this device and shared with Back Office AI.
        </p>

        {/* Provider toggle */}
        <div style={{ display: "flex", gap: 8, marginBottom: 24 }}>
          {(["anthropic", "openai"] as AiProvider[]).map(p => (
            <button key={p} onClick={() => { setProvider(p); setError(null); }} style={{
              flex: 1, padding: "8px 0", borderRadius: 8, fontWeight: 600, fontSize: "0.85rem",
              cursor: "pointer", transition: "all 0.15s",
              background: provider === p ? "var(--accent)" : "var(--surface)",
              color: provider === p ? "#fff" : "var(--text-dim)",
              border: `1.5px solid ${provider === p ? "var(--accent)" : "var(--border)"}`,
            }}>
              {p === "anthropic" ? "◆ Anthropic (Claude)" : "⬡ OpenAI / Custom"}
            </button>
          ))}
        </div>

        {/* ── Anthropic panel ── */}
        {provider === "anthropic" && (
          <>
            <label style={lbl}>Anthropic API Key</label>
            <input type="password" placeholder="sk-ant-api03-…" value={anthropicKey} autoFocus
              onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
              onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()}
              style={inp} />
            {error && <div style={{ color: "#ef4444", fontSize: "0.8rem", marginBottom: 10 }}>{error}</div>}
            <button onClick={handleSaveAnthropic} disabled={saving || !anthropicKey.trim()} style={{
              width: "100%", padding: 10, background: "var(--accent)", color: "#fff",
              border: "none", borderRadius: 8, fontWeight: 700, fontSize: "0.9rem",
              cursor: saving || !anthropicKey.trim() ? "not-allowed" : "pointer",
              opacity: saving || !anthropicKey.trim() ? 0.5 : 1,
            }}>
              {saving ? "Saving…" : "Save & Start Migration →"}
            </button>
            <p style={{ margin: "10px 0 0", fontSize: "0.75rem", color: "var(--text-dim)", textAlign: "center" }}>
              Get your key at <span style={{ color: "var(--accent)" }}>console.anthropic.com</span>
            </p>
          </>
        )}

        {/* ── OpenAI panel ── */}
        {provider === "openai" && (
          <>
            <label style={lbl}>Base URL</label>
            <input type="text" value={baseUrl}
              onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
              style={inp} />

            <label style={lbl}>API Key</label>
            <input type="password" placeholder="sk-…" value={openaiKey}
              onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
              style={{ ...inp, marginBottom: 14 }} />

            {error && <div style={{ color: "#ef4444", fontSize: "0.8rem", marginBottom: 10 }}>{error}</div>}

            {/* Step 1: validate & fetch models */}
            {models.length === 0 && (
              <button onClick={handleValidateOpenai}
                disabled={validating || !baseUrl.trim() || !openaiKey.trim()}
                style={{
                  width: "100%", padding: 10, background: "var(--surface2, var(--surface))",
                  color: "var(--text)", border: "1.5px solid var(--border)",
                  borderRadius: 8, fontWeight: 600, fontSize: "0.875rem",
                  cursor: validating || !baseUrl.trim() || !openaiKey.trim() ? "not-allowed" : "pointer",
                  opacity: validating || !baseUrl.trim() || !openaiKey.trim() ? 0.5 : 1,
                  marginBottom: 8,
                }}>
                {validating ? "Connecting…" : "Validate & Fetch Models"}
              </button>
            )}

            {/* Step 2: model select + save */}
            {models.length > 0 && (
              <>
                <label style={lbl}>Model</label>
                <select value={selectedModel} onChange={e => setSelectedModel(e.target.value)}
                  style={{ ...inp, marginBottom: 14 }}>
                  {models.map(m => (
                    <option key={m.id} value={m.id}>{m.id}</option>
                  ))}
                </select>
                <button onClick={handleSaveOpenai} disabled={saving || !selectedModel}
                  style={{
                    width: "100%", padding: 10, background: "var(--accent)", color: "#fff",
                    border: "none", borderRadius: 8, fontWeight: 700, fontSize: "0.9rem",
                    cursor: saving || !selectedModel ? "not-allowed" : "pointer",
                    opacity: saving || !selectedModel ? 0.5 : 1,
                  }}>
                  {saving ? "Saving…" : "Save & Start Migration →"}
                </button>
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}

// ─── AI Settings Drawer (in-chat model switcher) ──────────────────────────────

function AiSettingsDrawer({ onClose, onSaved }: { onClose: () => void; onSaved: (label: string) => void }) {
  const [provider, setProvider]           = useState<AiProvider>("anthropic");
  const [anthropicKey, setAnthropicKey]   = useState("");
  const [baseUrl, setBaseUrl]             = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]         = useState("");
  const [models, setModels]               = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating]       = useState(false);
  const [saving, setSaving]               = useState(false);
  const [error, setError]                 = useState<string | null>(null);

  // Pre-fill from current config
  useEffect(() => {
    adminGetProviderConfig().then(cfg => {
      if (cfg.provider === "openai") {
        setProvider("openai");
        if (cfg.openai_base_url) setBaseUrl(cfg.openai_base_url);
        if (cfg.openai_model)    setSelectedModel(cfg.openai_model);
      }
    }).catch(() => {});
  }, []);

  const inp: React.CSSProperties = {
    width: "100%", padding: "8px 11px", borderRadius: 7,
    border: "1px solid var(--border)", background: "var(--bg)",
    color: "var(--text)", fontSize: "0.85rem",
    boxSizing: "border-box", marginBottom: 10,
  };
  const lbl: React.CSSProperties = {
    display: "block", fontSize: "0.78rem", fontWeight: 600,
    marginBottom: 4, color: "var(--text-dim)",
  };

  const handleSaveAnthropic = async () => {
    const key = anthropicKey.trim();
    if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
    setSaving(true); setError(null);
    try { await adminSetAnthropic(key); onSaved("◆ Claude"); }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleValidate = async () => {
    setError(null); setModels([]); setSelectedModel(""); setValidating(true);
    try {
      const res = await adminValidateOpenai(baseUrl.trim(), openaiKey.trim());
      if (res.success && res.models.length > 0) {
        setModels(res.models); setSelectedModel(res.models[0].id);
      } else { setError(res.error ?? "Validation failed"); }
    } catch (e) { setError(String(e)); }
    finally { setValidating(false); }
  };

  const handleSaveOpenai = async () => {
    if (!selectedModel) return;
    setSaving(true); setError(null);
    try {
      await adminSetOpenai(baseUrl.trim(), openaiKey.trim(), selectedModel);
      onSaved(`⬡ ${selectedModel}`);
    } catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  return (
    <>
      {/* Backdrop */}
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, background: "rgba(0,0,0,0.45)", zIndex: 40,
      }} />
      {/* Drawer */}
      <div style={{
        position: "fixed", top: 0, right: 0, bottom: 0, width: 340,
        background: "var(--surface)", borderLeft: "1px solid var(--border)",
        zIndex: 41, display: "flex", flexDirection: "column",
        boxShadow: "-4px 0 24px rgba(0,0,0,0.3)",
      }}>
        {/* Drawer header */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "14px 16px", borderBottom: "1px solid var(--border)" }}>
          <span style={{ fontWeight: 700, fontSize: "0.95rem" }}>⚙ AI Model Settings</span>
          <button onClick={onClose} style={{
            background: "none", border: "none", cursor: "pointer",
            color: "var(--text-dim)", fontSize: "1.2rem", lineHeight: 1, padding: 2,
          }}>✕</button>
        </div>

        {/* Drawer body */}
        <div style={{ flex: 1, overflowY: "auto", padding: 16 }}>
          {/* Provider toggle */}
          <div style={{ display: "flex", gap: 6, marginBottom: 18 }}>
            {(["anthropic", "openai"] as AiProvider[]).map(p => (
              <button key={p} onClick={() => { setProvider(p); setError(null); setModels([]); }} style={{
                flex: 1, padding: "7px 0", borderRadius: 7, fontWeight: 600, fontSize: "0.8rem",
                cursor: "pointer", transition: "all 0.15s",
                background: provider === p ? "var(--accent)" : "transparent",
                color: provider === p ? "#fff" : "var(--text-dim)",
                border: `1.5px solid ${provider === p ? "var(--accent)" : "var(--border)"}`,
              }}>
                {p === "anthropic" ? "◆ Anthropic" : "⬡ OpenAI / Custom"}
              </button>
            ))}
          </div>

          {/* Anthropic */}
          {provider === "anthropic" && (
            <>
              <label style={lbl}>Anthropic API Key</label>
              <input type="password" placeholder="sk-ant-api03-…" value={anthropicKey}
                onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
                onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()}
                style={inp} autoFocus />
              {error && <div style={{ color: "#ef4444", fontSize: "0.78rem", marginBottom: 8 }}>{error}</div>}
              <button onClick={handleSaveAnthropic} disabled={saving || !anthropicKey.trim()} style={{
                width: "100%", padding: 9, background: "var(--accent)", color: "#fff",
                border: "none", borderRadius: 7, fontWeight: 700, fontSize: "0.875rem",
                cursor: saving || !anthropicKey.trim() ? "not-allowed" : "pointer",
                opacity: saving || !anthropicKey.trim() ? 0.5 : 1,
              }}>{saving ? "Saving…" : "Save Changes"}</button>
              <p style={{ margin: "8px 0 0", fontSize: "0.72rem", color: "var(--text-dim)", textAlign: "center" }}>
                console.anthropic.com
              </p>
            </>
          )}

          {/* OpenAI */}
          {provider === "openai" && (
            <>
              <label style={lbl}>Base URL</label>
              <input type="text" value={baseUrl}
                onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
                style={inp} />
              <label style={lbl}>API Key</label>
              <input type="password" placeholder="sk-…" value={openaiKey}
                onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
                style={{ ...inp, marginBottom: 12 }} />
              {error && <div style={{ color: "#ef4444", fontSize: "0.78rem", marginBottom: 8 }}>{error}</div>}
              {models.length === 0 ? (
                <button onClick={handleValidate}
                  disabled={validating || !baseUrl.trim() || !openaiKey.trim()} style={{
                    width: "100%", padding: 9, background: "transparent",
                    color: "var(--text)", border: "1.5px solid var(--border)",
                    borderRadius: 7, fontWeight: 600, fontSize: "0.875rem",
                    cursor: validating || !baseUrl.trim() || !openaiKey.trim() ? "not-allowed" : "pointer",
                    opacity: validating || !baseUrl.trim() || !openaiKey.trim() ? 0.5 : 1,
                  }}>{validating ? "Connecting…" : "Validate & Fetch Models"}</button>
              ) : (
                <>
                  <label style={lbl}>Model</label>
                  <select value={selectedModel} onChange={e => setSelectedModel(e.target.value)} style={inp}>
                    {models.map(m => <option key={m.id} value={m.id}>{m.id}</option>)}
                  </select>
                  <button onClick={handleSaveOpenai} disabled={saving || !selectedModel} style={{
                    width: "100%", padding: 9, background: "var(--accent)", color: "#fff",
                    border: "none", borderRadius: 7, fontWeight: 700, fontSize: "0.875rem",
                    cursor: saving || !selectedModel ? "not-allowed" : "pointer",
                    opacity: saving || !selectedModel ? 0.5 : 1,
                  }}>{saving ? "Saving…" : "Save Changes"}</button>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function MigrationAgentPage({ onDone }: Props) {
  const [messages, setMessages] = useState<DisplayMessage[]>([WELCOME]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [context, setContext] = useState<MigrationContext>({});
  const [phase, setPhase] = useState<MigrationPhase>("discovery");
  const [pendingScript, setPendingScript] = useState<string | null>(null);
  const [pendingPreview, setPendingPreview] = useState<MigrationPreview | null>(null);
  const [pendingDescription, setPendingDescription] = useState("");
  const [executing, setExecuting] = useState(false);
  // null = checking, false = not configured, true = ready
  const [aiReady, setAiReady] = useState<boolean | null>(null);
  const [providerLabel, setProviderLabel] = useState("⚙ AI");
  const [showAiSettings, setShowAiSettings] = useState(false);

  const chatEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Check AI config on mount
  useEffect(() => {
    adminGetProviderConfig()
      .then(cfg => {
        const ready = cfg.provider !== "" && (cfg.anthropic_key_set || cfg.openai_key_set);
        setAiReady(ready);
        if (cfg.provider === "anthropic") setProviderLabel("◆ Claude");
        else if (cfg.provider === "openai") setProviderLabel(`⬡ ${cfg.openai_model || "OpenAI"}`);
      })
      .catch(() => setAiReady(false));
  }, []);

  // Build history for backend (role + content only)
  const history = messages
    .filter((m) => m.role !== "system")
    .map((m) => ({ role: m.role === "user" ? "user" as const : "assistant" as const, content: m.text }));

  // Auto-scroll to bottom
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  const addMessage = (msg: Omit<DisplayMessage, "id">) => {
    setMessages((prev) => [...prev, { ...msg, id: Math.random().toString(36).slice(2) }]);
  };

  // Advance phase based on response content
  const inferPhase = (text: string, currentPhase: MigrationPhase): MigrationPhase => {
    const lower = text.toLowerCase();
    if (
      currentPhase === "discovery" &&
      (lower.includes("connected") ||
        lower.includes("connect ok") ||
        lower.includes("sqlite file ok") ||
        (lower.includes("mysql") && lower.includes("ok")))
    )
      return "connected";
    if (
      currentPhase === "connected" &&
      (lower.includes("schema") || lower.includes("tables") || lower.includes("mapping"))
    )
      return "mapping";
    if (currentPhase === "mapping" && lower.includes("preview")) return "preview";
    return currentPhase;
  };

  // ── Send message ──────────────────────────────────────────────────────────
  const handleSend = async () => {
    const msg = input.trim();
    if (!msg || loading) return;

    setInput("");
    setLoading(true);
    addMessage({ role: "user", text: msg });

    try {
      const response: MigrationChatResponse = await migrationAgentChat({
        history,
        message: msg,
        user_id: "migration_user",
        context,
      });

      if (response.type === "NoApiKey") {
        addMessage({
          role: "system",
          text: "⚠️ No AI provider configured. Please set up an AI API key in Back Office → AI Settings first.",
        });
      } else if (response.type === "PendingMigration") {
        setPendingScript(response.script);
        setPendingPreview(response.preview);
        setPendingDescription(response.description);
        setPhase("preview");
        addMessage({
          role: "assistant",
          text:
            response.description ||
            "I've analyzed your data and prepared a migration script. Please review the preview below and click CONFIRM to proceed.",
        });
      } else {
        const newPhase = inferPhase(response.content, phase);
        if (newPhase !== phase) setPhase(newPhase);
        addMessage({ role: "assistant", text: response.content });
      }
    } catch (err) {
      addMessage({ role: "system", text: `Error: ${String(err)}` });
    } finally {
      setLoading(false);
    }
  };

  // ── Confirm migration ─────────────────────────────────────────────────────
  const handleConfirm = async () => {
    if (!pendingScript) return;
    setExecuting(true);
    setPhase("executing");

    try {
      const result = await migrationConfirmExecute(pendingScript, "migration_user");
      setPhase("done");
      setPendingScript(null);
      setPendingPreview(null);
      addMessage({ role: "assistant", text: result });
      addMessage({
        role: "system",
        text: "✅ Migration complete! You can now start using ZANPOS with your imported data.",
      });
    } catch (err) {
      setPhase("preview");
      addMessage({ role: "system", text: `❌ Migration failed: ${String(err)}` });
    } finally {
      setExecuting(false);
    }
  };

  // ── File attachment ───────────────────────────────────────────────────────
  const handleAttach = async () => {
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          {
            name: "Database / Spreadsheet",
            extensions: ["db", "sqlite", "sqlite3", "csv", "xlsx", "xls"],
          },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      // selected is null | string | string[] in Tauri v2
      if (selected === null || selected === undefined) return;
      const filePath = Array.isArray(selected) ? selected[0] : selected;
      if (!filePath) return;

      const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
      const dbType: MigrationContext["db_type"] =
        ext === "csv" || ext === "xlsx" || ext === "xls" ? "csv" : "sqlite";

      setContext((prev) => ({
        ...prev,
        db_type: dbType,
        attached_file_path: filePath,
        path_or_connstr: filePath,
      }));
      addMessage({
        role: "system",
        text: `📎 Attached: ${filePath}`,
      });
      // Pre-fill input to trigger analysis
      setInput(`I've attached the file: ${filePath}`);
    } catch {
      // User cancelled picker — ignore
    }
  };

  // ── Keyboard handling ─────────────────────────────────────────────────────
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // ── Phase stepper ─────────────────────────────────────────────────────────
  const currentPhaseIdx = PHASE_ORDER.indexOf(phase);

  // ── AI config gate ────────────────────────────────────────────────────────
  if (aiReady === null) {
    return (
      <div className="mig-page" style={{ alignItems: "center", justifyContent: "center" }}>
        <div style={{ color: "var(--text-dim)", fontSize: "0.9rem" }}>Checking AI configuration…</div>
      </div>
    );
  }
  if (aiReady === false) {
    return <AiSetupPanel onConfigured={() => setAiReady(true)} />;
  }

  return (
    <div className="mig-page">
      {/* Phase stepper header */}
      <div className="mig-header">
        <span className="mig-header-title">Migration Agent</span>
        {PHASES.map((p, i) => {
          const idx = PHASE_ORDER.indexOf(p.id);
          const isDone = idx < currentPhaseIdx;
          const isActive = idx === currentPhaseIdx;
          return (
            <React.Fragment key={p.id}>
              {i > 0 && <span className="mig-phase-sep">›</span>}
              <span
                className={`mig-phase-step${isActive ? " mig-phase-step--active" : ""}${isDone ? " mig-phase-step--done" : ""}`}
              >
                {isDone ? "✓ " : ""}
                {p.label}
              </span>
            </React.Fragment>
          );
        })}

        {/* Right-side controls */}
        <div style={{ marginLeft: "auto", display: "flex", gap: 6, alignItems: "center" }}>
          {/* AI model badge — click to open settings drawer */}
          <button
            onClick={() => setShowAiSettings(true)}
            title="Change AI provider / model"
            style={{
              padding: "4px 10px", borderRadius: 6, cursor: "pointer",
              background: "var(--accent)", color: "#fff",
              border: "none", fontSize: "0.78rem", fontWeight: 600,
              opacity: 0.9,
            }}
          >
            {providerLabel}
          </button>
          <button
            onClick={onDone}
            style={{
              padding: "4px 12px", background: "transparent",
              border: "1px solid var(--border)", borderRadius: 6,
              cursor: "pointer", color: "var(--text-dim)", fontSize: "0.8rem",
            }}
          >
            Skip → Launch POS
          </button>
        </div>
      </div>

      {/* AI Settings Drawer */}
      {showAiSettings && (
        <AiSettingsDrawer
          onClose={() => setShowAiSettings(false)}
          onSaved={(label) => {
            setProviderLabel(label);
            setShowAiSettings(false);
            addMessage({ role: "system", text: `✅ AI switched to **${label}**` });
          }}
        />
      )}

      {/* Chat area */}
      <div className="mig-chat-area">
        {messages.map((msg) => (
          <div key={msg.id} className={`mig-bubble mig-bubble--${msg.role}`}>
            <SimpleMarkdown text={msg.text} />
          </div>
        ))}

        {/* Preview card — shown inline after preview response */}
        {pendingPreview && phase === "preview" && (
          <MigrationPreviewCard
            preview={pendingPreview}
            description={pendingDescription}
            onConfirm={handleConfirm}
            executing={executing}
          />
        )}

        {/* Thinking indicator */}
        {loading && (
          <div className="mig-thinking">
            <div className="mig-dot-pulse">
              <span />
              <span />
              <span />
            </div>
            Analysing…
          </div>
        )}

        {/* Done banner */}
        {phase === "done" && (
          <div className="mig-done-banner">
            🎉 Your data has been imported successfully!
            <br />
            <button className="mig-launch-btn" style={{ marginTop: 12 }} onClick={onDone}>
              Launch POS 🚀
            </button>
          </div>
        )}

        <div ref={chatEndRef} />
      </div>

      {/* Input bar */}
      {phase !== "done" && (
        <div className="mig-input-bar">
          <button className="mig-attach-btn" onClick={handleAttach} title="Attach CSV/Excel/SQLite file">
            📎
          </button>
          <textarea
            ref={inputRef}
            className="mig-input"
            placeholder={
              phase === "preview"
                ? "Ask a question about the migration, or click CONFIRM above…"
                : "Describe your old POS system or paste a file path…"
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
            disabled={loading || executing}
          />
          <button
            className="mig-send-btn"
            onClick={handleSend}
            disabled={loading || executing || !input.trim()}
          >
            {loading ? "…" : "Send"}
          </button>
        </div>
      )}
    </div>
  );
}
