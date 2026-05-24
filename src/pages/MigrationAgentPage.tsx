import React, { useState, useEffect, useRef } from "react";
import {
  migrationInspectFile,
  migrationAiMap,
  migrationExecute,
  adminGetProviderConfig,
  adminSetAnthropic,
  adminValidateOpenai,
  adminSetOpenai,
} from "../tauri/commands";
import type {
  FileSchema,
  MappingConfig,
  MigrationProgress,
  ModelInfo,
} from "../types";
import { Channel } from "@tauri-apps/api/core";
import { open as openFilePicker } from "@tauri-apps/plugin-dialog";

// ─── Constants ────────────────────────────────────────────────────────────────

const TARGET_TABLES = [
  "products",
  "categories",
  "customers",
  "sales",
  "sale_items",
  "stock_levels",
  "payments",
  "skip",
];

const TRANSFORMS = [
  "identity",
  "price_minor",
  "integer",
  "boolean",
  "date",
  "phone",
  "skip",
];

const WIZARD_PHASES: { id: WizardPhase; label: string }[] = [
  { id: "file",      label: "File" },
  { id: "mapping",   label: "Mapping" },
  { id: "confirm",   label: "Confirm" },
  { id: "executing", label: "Importing" },
  { id: "done",      label: "Done" },
];

const PHASE_ORDER: WizardPhase[] = ["file", "mapping", "confirm", "executing", "done"];

// ─── Types ────────────────────────────────────────────────────────────────────

type WizardPhase = "file" | "mapping" | "confirm" | "executing" | "done";
type AiProvider = "anthropic" | "openai";

interface SheetProgress {
  sheet: string;
  target: string;
  total: number;
  done: number;
  inserted?: number;
  skipped?: number;
  finished: boolean;
  error?: string;
}

interface Props {
  onDone: () => void;
}

// ─── Simple markdown renderer ─────────────────────────────────────────────────

function SimpleMarkdown({ text }: { text: string }) {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  const rendered = parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          style={{
            background: "var(--border)",
            padding: "1px 4px",
            borderRadius: 3,
            fontSize: "0.85em",
          }}
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    return part.split("\n").map((line, j, arr) => (
      <React.Fragment key={`${i}-${j}`}>
        {line}
        {j < arr.length - 1 && <br />}
      </React.Fragment>
    ));
  });
  return <>{rendered}</>;
}

// ─── AI Config Setup Panel ────────────────────────────────────────────────────

function AiSetupPanel({ onConfigured }: { onConfigured: () => void }) {
  const [provider, setProvider]           = useState<AiProvider>("anthropic");
  const [anthropicKey, setAnthropicKey]   = useState("");
  const [baseUrl, setBaseUrl]             = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]         = useState("");
  const [models, setModels]               = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating]       = useState(false);
  const [saving, setSaving]               = useState(false);
  const [error, setError]                 = useState<string | null>(null);

  const handleSaveAnthropic = async () => {
    const key = anthropicKey.trim();
    if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
    setSaving(true); setError(null);
    try { await adminSetAnthropic(key); onConfigured(); }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleValidateOpenai = async () => {
    setError(null); setModels([]); setSelectedModel(""); setValidating(true);
    try {
      const res = await adminValidateOpenai(baseUrl.trim(), openaiKey.trim());
      if (res.success && res.models.length > 0) {
        setModels(res.models); setSelectedModel(res.models[0].id);
      } else {
        setError(res.error ?? "Validation failed — check URL and key");
      }
    } catch (e) { setError(String(e)); }
    finally { setValidating(false); }
  };

  const handleSaveOpenai = async () => {
    if (!selectedModel) return;
    setSaving(true); setError(null);
    try { await adminSetOpenai(baseUrl.trim(), openaiKey.trim(), selectedModel); onConfigured(); }
    catch (e) { setError(String(e)); }
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
        <h2 style={{ margin: "0 0 6px", fontSize: "1.1rem", fontWeight: 700 }}>
          Set up AI before migrating
        </h2>
        <p style={{ margin: "0 0 20px", fontSize: "0.85rem", color: "var(--text-dim)", lineHeight: 1.5 }}>
          Choose your AI provider. Keys are stored securely on this device.
        </p>

        <div style={{ display: "flex", gap: 8, marginBottom: 24 }}>
          {(["anthropic", "openai"] as AiProvider[]).map(p => (
            <button
              key={p}
              onClick={() => { setProvider(p); setError(null); }}
              style={{
                flex: 1, padding: "8px 0", borderRadius: 8, fontWeight: 600, fontSize: "0.85rem",
                cursor: "pointer", transition: "all 0.15s",
                background: provider === p ? "var(--accent)" : "var(--surface)",
                color: provider === p ? "#fff" : "var(--text-dim)",
                border: `1.5px solid ${provider === p ? "var(--accent)" : "var(--border)"}`,
              }}
            >
              {p === "anthropic" ? "◆ Anthropic (Claude)" : "⬡ OpenAI / Custom"}
            </button>
          ))}
        </div>

        {provider === "anthropic" && (
          <>
            <label style={lbl}>Anthropic API Key</label>
            <input
              type="password" placeholder="sk-ant-api03-…" value={anthropicKey} autoFocus
              onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
              onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()}
              style={inp}
            />
            {error && <div style={{ color: "#ef4444", fontSize: "0.8rem", marginBottom: 10 }}>{error}</div>}
            <button
              onClick={handleSaveAnthropic}
              disabled={saving || !anthropicKey.trim()}
              style={{
                width: "100%", padding: 10, background: "var(--accent)", color: "#fff",
                border: "none", borderRadius: 8, fontWeight: 700, fontSize: "0.9rem",
                cursor: saving || !anthropicKey.trim() ? "not-allowed" : "pointer",
                opacity: saving || !anthropicKey.trim() ? 0.5 : 1,
              }}
            >
              {saving ? "Saving…" : "Save & Start Migration →"}
            </button>
            <p style={{ margin: "10px 0 0", fontSize: "0.75rem", color: "var(--text-dim)", textAlign: "center" }}>
              Get your key at <span style={{ color: "var(--accent)" }}>console.anthropic.com</span>
            </p>
          </>
        )}

        {provider === "openai" && (
          <>
            <label style={lbl}>Base URL</label>
            <input
              type="text" value={baseUrl}
              onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
              style={inp}
            />
            <label style={lbl}>API Key</label>
            <input
              type="password" placeholder="sk-…" value={openaiKey}
              onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
              style={{ ...inp, marginBottom: 14 }}
            />
            {error && <div style={{ color: "#ef4444", fontSize: "0.8rem", marginBottom: 10 }}>{error}</div>}
            {models.length === 0 ? (
              <button
                onClick={handleValidateOpenai}
                disabled={validating || !baseUrl.trim() || !openaiKey.trim()}
                style={{
                  width: "100%", padding: 10, background: "var(--surface2, var(--surface))",
                  color: "var(--text)", border: "1.5px solid var(--border)",
                  borderRadius: 8, fontWeight: 600, fontSize: "0.875rem",
                  cursor: validating || !baseUrl.trim() || !openaiKey.trim() ? "not-allowed" : "pointer",
                  opacity: validating || !baseUrl.trim() || !openaiKey.trim() ? 0.5 : 1,
                  marginBottom: 8,
                }}
              >
                {validating ? "Connecting…" : "Validate & Fetch Models"}
              </button>
            ) : (
              <>
                <label style={lbl}>Model</label>
                <select
                  value={selectedModel}
                  onChange={e => setSelectedModel(e.target.value)}
                  style={{ ...inp, marginBottom: 14 }}
                >
                  {models.map(m => <option key={m.id} value={m.id}>{m.id}</option>)}
                </select>
                <button
                  onClick={handleSaveOpenai}
                  disabled={saving || !selectedModel}
                  style={{
                    width: "100%", padding: 10, background: "var(--accent)", color: "#fff",
                    border: "none", borderRadius: 8, fontWeight: 700, fontSize: "0.9rem",
                    cursor: saving || !selectedModel ? "not-allowed" : "pointer",
                    opacity: saving || !selectedModel ? 0.5 : 1,
                  }}
                >
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

// ─── AI Settings Drawer ───────────────────────────────────────────────────────

function AiSettingsDrawer({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: (label: string) => void;
}) {
  const [provider, setProvider]           = useState<AiProvider>("anthropic");
  const [anthropicKey, setAnthropicKey]   = useState("");
  const [baseUrl, setBaseUrl]             = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]         = useState("");
  const [models, setModels]               = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating]       = useState(false);
  const [saving, setSaving]               = useState(false);
  const [error, setError]                 = useState<string | null>(null);

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
      <div
        onClick={onClose}
        style={{ position: "fixed", inset: 0, background: "rgba(0,0,0,0.45)", zIndex: 40 }}
      />
      <div style={{
        position: "fixed", top: 0, right: 0, bottom: 0, width: 340,
        background: "var(--surface)", borderLeft: "1px solid var(--border)",
        zIndex: 41, display: "flex", flexDirection: "column",
        boxShadow: "-4px 0 24px rgba(0,0,0,0.3)",
      }}>
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "14px 16px", borderBottom: "1px solid var(--border)",
        }}>
          <span style={{ fontWeight: 700, fontSize: "0.95rem" }}>⚙ AI Model Settings</span>
          <button
            onClick={onClose}
            style={{ background: "none", border: "none", cursor: "pointer",
              color: "var(--text-dim)", fontSize: "1.2rem", lineHeight: 1, padding: 2 }}
          >✕</button>
        </div>
        <div style={{ flex: 1, overflowY: "auto", padding: 16 }}>
          <div style={{ display: "flex", gap: 6, marginBottom: 18 }}>
            {(["anthropic", "openai"] as AiProvider[]).map(p => (
              <button
                key={p}
                onClick={() => { setProvider(p); setError(null); setModels([]); }}
                style={{
                  flex: 1, padding: "7px 0", borderRadius: 7, fontWeight: 600, fontSize: "0.8rem",
                  cursor: "pointer", transition: "all 0.15s",
                  background: provider === p ? "var(--accent)" : "transparent",
                  color: provider === p ? "#fff" : "var(--text-dim)",
                  border: `1.5px solid ${provider === p ? "var(--accent)" : "var(--border)"}`,
                }}
              >
                {p === "anthropic" ? "◆ Anthropic" : "⬡ OpenAI / Custom"}
              </button>
            ))}
          </div>

          {provider === "anthropic" && (
            <>
              <label style={lbl}>Anthropic API Key</label>
              <input
                type="password" placeholder="sk-ant-api03-…" value={anthropicKey} autoFocus
                onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
                onKeyDown={e => e.key === "Enter" && handleSaveAnthropic()}
                style={inp}
              />
              {error && <div style={{ color: "#ef4444", fontSize: "0.78rem", marginBottom: 8 }}>{error}</div>}
              <button
                onClick={handleSaveAnthropic}
                disabled={saving || !anthropicKey.trim()}
                style={{
                  width: "100%", padding: 9, background: "var(--accent)", color: "#fff",
                  border: "none", borderRadius: 7, fontWeight: 700, fontSize: "0.875rem",
                  cursor: saving || !anthropicKey.trim() ? "not-allowed" : "pointer",
                  opacity: saving || !anthropicKey.trim() ? 0.5 : 1,
                }}
              >{saving ? "Saving…" : "Save Changes"}</button>
              <p style={{ margin: "8px 0 0", fontSize: "0.72rem", color: "var(--text-dim)", textAlign: "center" }}>
                console.anthropic.com
              </p>
            </>
          )}

          {provider === "openai" && (
            <>
              <label style={lbl}>Base URL</label>
              <input
                type="text" value={baseUrl}
                onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
                style={inp}
              />
              <label style={lbl}>API Key</label>
              <input
                type="password" placeholder="sk-…" value={openaiKey}
                onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
                style={{ ...inp, marginBottom: 12 }}
              />
              {error && <div style={{ color: "#ef4444", fontSize: "0.78rem", marginBottom: 8 }}>{error}</div>}
              {models.length === 0 ? (
                <button
                  onClick={handleValidate}
                  disabled={validating || !baseUrl.trim() || !openaiKey.trim()}
                  style={{
                    width: "100%", padding: 9, background: "transparent",
                    color: "var(--text)", border: "1.5px solid var(--border)",
                    borderRadius: 7, fontWeight: 600, fontSize: "0.875rem",
                    cursor: validating || !baseUrl.trim() || !openaiKey.trim() ? "not-allowed" : "pointer",
                    opacity: validating || !baseUrl.trim() || !openaiKey.trim() ? 0.5 : 1,
                  }}
                >{validating ? "Connecting…" : "Validate & Fetch Models"}</button>
              ) : (
                <>
                  <label style={lbl}>Model</label>
                  <select value={selectedModel} onChange={e => setSelectedModel(e.target.value)} style={inp}>
                    {models.map(m => <option key={m.id} value={m.id}>{m.id}</option>)}
                  </select>
                  <button
                    onClick={handleSaveOpenai}
                    disabled={saving || !selectedModel}
                    style={{
                      width: "100%", padding: 9, background: "var(--accent)", color: "#fff",
                      border: "none", borderRadius: 7, fontWeight: 700, fontSize: "0.875rem",
                      cursor: saving || !selectedModel ? "not-allowed" : "pointer",
                      opacity: saving || !selectedModel ? 0.5 : 1,
                    }}
                  >{saving ? "Saving…" : "Save Changes"}</button>
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
  // ── AI config state ──────────────────────────────────────────────────────
  const [aiReady, setAiReady]             = useState<boolean | null>(null);
  const [providerLabel, setProviderLabel] = useState("⚙ AI");
  const [showAiSettings, setShowAiSettings] = useState(false);

  // ── Wizard phase ─────────────────────────────────────────────────────────
  const [phase, setPhase] = useState<WizardPhase>("file");

  // ── File phase ───────────────────────────────────────────────────────────
  const [filePath, setFilePath]     = useState<string | null>(null);
  const [schema, setSchema]         = useState<FileSchema | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [inspectError, setInspectError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);

  // ── Mapping phase ────────────────────────────────────────────────────────
  const [mapping, setMapping]       = useState<MappingConfig | null>(null);
  const [aiMapping, setAiMapping]   = useState(false);
  const [aiMapError, setAiMapError] = useState<string | null>(null);

  // ── Executing phase ──────────────────────────────────────────────────────
  const [progressItems, setProgressItems] = useState<SheetProgress[]>([]);
  const [executeError, setExecuteError]   = useState<string | null>(null);
  const [doneMessage, setDoneMessage]     = useState("");

  const contentRef = useRef<HTMLDivElement>(null);

  // ── Check AI config on mount ────────────────────────────────────────────
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

  // ── File inspection ─────────────────────────────────────────────────────
  const doInspect = async (path: string) => {
    setFilePath(path);
    setInspecting(true);
    setInspectError(null);
    setSchema(null);
    setMapping(null);
    try {
      const s = await migrationInspectFile(path);
      setSchema(s);
    } catch (e) {
      setInspectError(String(e));
      setFilePath(null);
    } finally {
      setInspecting(false);
    }
  };

  const handlePickFile = async () => {
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          { name: "Database / Spreadsheet", extensions: ["db", "sqlite", "sqlite3", "csv", "xlsx", "xls"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (!selected) return;
      const path = Array.isArray(selected) ? selected[0] : selected as string;
      if (path) await doInspect(path);
    } catch { /* user cancelled */ }
  };

  const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragOver(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    // In Tauri the path is available on the file object
    const path = (file as File & { path?: string }).path;
    if (path) await doInspect(path);
  };

  // ── AI mapping ───────────────────────────────────────────────────────────
  const handleAiMap = async () => {
    if (!schema) return;
    setAiMapping(true);
    setAiMapError(null);
    try {
      const m = await migrationAiMap(schema, 3); // BHD exponent = 3
      setMapping(m);
    } catch (e) {
      setAiMapError(String(e));
    } finally {
      setAiMapping(false);
    }
  };

  // ── Mapping editors ──────────────────────────────────────────────────────
  const updateTargetTable = (si: number, val: string) => {
    setMapping(prev => {
      if (!prev) return prev;
      const sheets = [...prev.sheet_mappings];
      sheets[si] = { ...sheets[si], target_table: val };
      return { sheet_mappings: sheets };
    });
  };

  const updateTargetCol = (si: number, ci: number, val: string) => {
    setMapping(prev => {
      if (!prev) return prev;
      const sheets = [...prev.sheet_mappings];
      const cols = [...sheets[si].column_mappings];
      cols[ci] = { ...cols[ci], target_col: val };
      sheets[si] = { ...sheets[si], column_mappings: cols };
      return { sheet_mappings: sheets };
    });
  };

  const updateTransform = (si: number, ci: number, val: string) => {
    setMapping(prev => {
      if (!prev) return prev;
      const sheets = [...prev.sheet_mappings];
      const cols = [...sheets[si].column_mappings];
      cols[ci] = { ...cols[ci], transform: val };
      sheets[si] = { ...sheets[si], column_mappings: cols };
      return { sheet_mappings: sheets };
    });
  };

  // ── Execute migration ────────────────────────────────────────────────────
  const handleExecute = async () => {
    if (!filePath || !mapping) return;
    setPhase("executing");
    setProgressItems([]);
    setExecuteError(null);

    const channel = new Channel<MigrationProgress>();
    channel.onmessage = (event: MigrationProgress) => {
      if (event.type === "started") {
        // nothing yet — sheets will register themselves
      } else if (event.type === "sheet_start") {
        setProgressItems(prev => [
          ...prev,
          { sheet: event.sheet, target: event.target, total: event.total_rows, done: 0, finished: false },
        ]);
      } else if (event.type === "sheet_progress") {
        setProgressItems(prev =>
          prev.map(p => p.sheet === event.sheet ? { ...p, done: event.done } : p)
        );
      } else if (event.type === "sheet_done") {
        setProgressItems(prev =>
          prev.map(p =>
            p.sheet === event.sheet
              ? { ...p, done: p.total, inserted: event.inserted, skipped: event.skipped, finished: true }
              : p
          )
        );
      } else if (event.type === "done") {
        setDoneMessage(event.message);
        setPhase("done");
      } else if (event.type === "error") {
        setExecuteError(event.message);
        setPhase("confirm");
      }
    };

    try {
      await migrationExecute(filePath, mapping, 3, channel);
    } catch (e) {
      setExecuteError(String(e));
      setPhase("confirm");
    }
  };

  // ── Derived stats for confirm phase ─────────────────────────────────────
  const confirmStats = (() => {
    if (!schema || !mapping) return null;
    const activeSheets = mapping.sheet_mappings.filter(sm => sm.target_table !== "skip");
    const totalRows = activeSheets.reduce((sum, sm) => {
      const sheet = schema.sheets.find(s => s.name === sm.source_sheet);
      return sum + (sheet?.row_count ?? 0);
    }, 0);
    const tables = [...new Set(activeSheets.map(sm => sm.target_table))];
    return { sheets: activeSheets.length, totalRows, tables };
  })();

  // ── AI config gate ───────────────────────────────────────────────────────
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

  // ── Phase index ──────────────────────────────────────────────────────────
  const currentPhaseIdx = PHASE_ORDER.indexOf(phase);

  return (
    <div className="mig-page">
      {/* ── Header / phase stepper ── */}
      <div className="mig-header">
        <span className="mig-header-title">Migration Agent</span>
        {WIZARD_PHASES.map((p, i) => {
          const idx = PHASE_ORDER.indexOf(p.id);
          const isDone   = idx < currentPhaseIdx;
          const isActive = idx === currentPhaseIdx;
          return (
            <React.Fragment key={p.id}>
              {i > 0 && <span className="mig-phase-sep">›</span>}
              <span className={
                `mig-phase-step${isActive ? " mig-phase-step--active" : ""}${isDone ? " mig-phase-step--done" : ""}`
              }>
                {isDone ? "✓ " : ""}{p.label}
              </span>
            </React.Fragment>
          );
        })}
        <div style={{ marginLeft: "auto", display: "flex", gap: 6, alignItems: "center" }}>
          <button
            onClick={() => setShowAiSettings(true)}
            title="Change AI provider / model"
            style={{
              padding: "4px 10px", borderRadius: 6, cursor: "pointer",
              background: "var(--accent)", color: "#fff",
              border: "none", fontSize: "0.78rem", fontWeight: 600, opacity: 0.9,
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

      {/* ── AI Settings Drawer ── */}
      {showAiSettings && (
        <AiSettingsDrawer
          onClose={() => setShowAiSettings(false)}
          onSaved={label => { setProviderLabel(label); setShowAiSettings(false); }}
        />
      )}

      {/* ── Content area ── */}
      <div
        ref={contentRef}
        style={{ flex: 1, overflowY: "auto", padding: "24px 32px", display: "flex", flexDirection: "column", gap: 20 }}
      >

        {/* ═══════════════ PHASE: FILE ═══════════════ */}
        {phase === "file" && (
          <div style={{ maxWidth: 680, width: "100%" }}>
            <h2 style={{ margin: "0 0 6px", fontSize: "1.15rem", fontWeight: 700 }}>
              Select your source file
            </h2>
            <p style={{ margin: "0 0 20px", fontSize: "0.875rem", color: "var(--text-dim)" }}>
              Supported: Excel (.xlsx / .xls), CSV (.csv), SQLite (.db / .sqlite)
            </p>

            {/* Drop zone */}
            <div
              className={`mig-dropzone${isDragOver ? " mig-dropzone--over" : ""}`}
              onClick={!inspecting ? handlePickFile : undefined}
              onDragOver={e => { e.preventDefault(); setIsDragOver(true); }}
              onDragLeave={() => setIsDragOver(false)}
              onDrop={handleDrop}
            >
              {inspecting ? (
                <>
                  <div className="mig-dot-pulse" style={{ justifyContent: "center", marginBottom: 8 }}>
                    <span /><span /><span />
                  </div>
                  <div style={{ fontSize: "0.875rem", color: "var(--text-dim)" }}>
                    Reading file structure…
                  </div>
                </>
              ) : (
                <>
                  <div style={{ fontSize: "2.5rem", marginBottom: 10 }}>📂</div>
                  <div style={{ fontWeight: 600, marginBottom: 4 }}>Click to browse or drag & drop</div>
                  <div style={{ fontSize: "0.8rem", color: "var(--text-dim)" }}>
                    Excel, CSV, or SQLite
                  </div>
                </>
              )}
            </div>

            {inspectError && (
              <div style={{
                marginTop: 12, padding: "10px 14px", borderRadius: 8,
                background: "color-mix(in srgb, #ef4444 10%, transparent)",
                border: "1px solid #ef444440", color: "#dc2626", fontSize: "0.85rem",
              }}>
                ❌ {inspectError}
                <button
                  onClick={() => { setInspectError(null); setFilePath(null); }}
                  style={{ marginLeft: 12, background: "none", border: "none", cursor: "pointer", color: "inherit", textDecoration: "underline" }}
                >
                  Try another file
                </button>
              </div>
            )}

            {/* Schema preview */}
            {schema && (
              <div style={{ marginTop: 20 }}>
                <div style={{ fontSize: "0.8rem", color: "var(--text-dim)", marginBottom: 10, fontWeight: 600 }}>
                  📋 {schema.file_type.toUpperCase()} — {schema.sheets.length} sheet{schema.sheets.length !== 1 ? "s" : ""} detected
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                  {schema.sheets.map(sheet => (
                    <div key={sheet.name} className="mig-schema-sheet">
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 6 }}>
                        <span style={{ fontWeight: 700, fontSize: "0.9rem" }}>{sheet.name}</span>
                        <span style={{ fontSize: "0.75rem", color: "var(--text-dim)" }}>
                          {sheet.row_count.toLocaleString()} rows
                        </span>
                      </div>
                      <div style={{ display: "flex", flexWrap: "wrap", gap: "4px 8px" }}>
                        {sheet.columns.slice(0, 12).map(col => (
                          <span
                            key={col.name}
                            style={{
                              padding: "2px 8px", borderRadius: 12,
                              background: "var(--bg)", border: "1px solid var(--border)",
                              fontSize: "0.75rem", color: "var(--text)",
                            }}
                            title={col.samples.length ? `Samples: ${col.samples.join(", ")}` : undefined}
                          >
                            {col.name}
                          </span>
                        ))}
                        {sheet.columns.length > 12 && (
                          <span style={{ fontSize: "0.75rem", color: "var(--text-dim)" }}>
                            +{sheet.columns.length - 12} more
                          </span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>

                <button
                  onClick={() => setPhase("mapping")}
                  style={{
                    marginTop: 20, padding: "11px 28px",
                    background: "var(--accent)", color: "#fff",
                    border: "none", borderRadius: 8, fontWeight: 700,
                    fontSize: "0.95rem", cursor: "pointer",
                  }}
                >
                  Next: Generate Mapping with AI ✨
                </button>
              </div>
            )}
          </div>
        )}

        {/* ═══════════════ PHASE: MAPPING ═══════════════ */}
        {phase === "mapping" && schema && (
          <div style={{ maxWidth: 900, width: "100%" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 16 }}>
              <div>
                <h2 style={{ margin: 0, fontSize: "1.1rem", fontWeight: 700 }}>Column mapping</h2>
                <p style={{ margin: "4px 0 0", fontSize: "0.82rem", color: "var(--text-dim)" }}>
                  Map each source column to a ZANPOS target. The AI pre-fills this — review and adjust as needed.
                </p>
              </div>
              <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
                <button
                  onClick={() => setPhase("file")}
                  style={{
                    padding: "8px 14px", background: "transparent",
                    border: "1px solid var(--border)", borderRadius: 7,
                    cursor: "pointer", color: "var(--text-dim)", fontSize: "0.82rem",
                  }}
                >
                  ← Back
                </button>
                <button
                  onClick={handleAiMap}
                  disabled={aiMapping}
                  style={{
                    padding: "8px 16px", background: "var(--surface)",
                    border: "1.5px solid var(--accent)", borderRadius: 7,
                    cursor: aiMapping ? "not-allowed" : "pointer",
                    color: "var(--accent)", fontSize: "0.85rem", fontWeight: 700,
                    opacity: aiMapping ? 0.6 : 1,
                  }}
                >
                  {aiMapping ? "⏳ AI Thinking…" : mapping ? "♻ Re-generate with AI" : "✨ Generate with AI"}
                </button>
              </div>
            </div>

            {aiMapError && (
              <div style={{
                marginBottom: 14, padding: "10px 14px", borderRadius: 8,
                background: "color-mix(in srgb, #ef4444 10%, transparent)",
                border: "1px solid #ef444440", color: "#dc2626", fontSize: "0.85rem",
              }}>
                ❌ AI error: {aiMapError}
              </div>
            )}

            {!mapping && !aiMapping && (
              <div style={{
                padding: 32, textAlign: "center", border: "2px dashed var(--border)",
                borderRadius: 12, color: "var(--text-dim)", fontSize: "0.875rem",
              }}>
                Click <strong>✨ Generate with AI</strong> to auto-map your columns, then review and adjust.
              </div>
            )}

            {aiMapping && !mapping && (
              <div style={{ padding: 32, textAlign: "center", color: "var(--text-dim)" }}>
                <div className="mig-dot-pulse" style={{ justifyContent: "center", marginBottom: 12 }}>
                  <span /><span /><span />
                </div>
                <div style={{ fontSize: "0.875rem" }}>
                  Analysing schema and generating column mapping…
                </div>
              </div>
            )}

            {mapping && (
              <>
                {mapping.sheet_mappings.map((sm, si) => (
                  <div key={sm.source_sheet} className="mig-schema-sheet" style={{ marginBottom: 16 }}>
                    {/* Sheet header */}
                    <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 10 }}>
                      <span style={{ fontWeight: 700, fontSize: "0.9rem", color: "var(--text)" }}>
                        {sm.source_sheet}
                      </span>
                      <span style={{ color: "var(--text-dim)", fontSize: "0.85rem" }}>→</span>
                      <select
                        value={sm.target_table}
                        onChange={e => updateTargetTable(si, e.target.value)}
                        style={{
                          padding: "4px 8px", borderRadius: 6, fontSize: "0.82rem", fontWeight: 600,
                          border: "1.5px solid var(--accent)", background: "var(--bg)", color: "var(--text)",
                          cursor: "pointer",
                        }}
                      >
                        {TARGET_TABLES.map(t => <option key={t} value={t}>{t}</option>)}
                      </select>
                      {sm.target_table === "skip" && (
                        <span style={{ fontSize: "0.75rem", color: "#f59e0b" }}>⏭ This sheet will be skipped</span>
                      )}
                    </div>

                    {sm.target_table !== "skip" && (
                      <div className="mig-mapping-table-wrapper">
                        <table className="mig-mapping-table">
                          <thead>
                            <tr>
                              <th>Source Column</th>
                              <th>→ Target Column</th>
                              <th>Transform</th>
                              <th>Notes</th>
                            </tr>
                          </thead>
                          <tbody>
                            {sm.column_mappings.map((cm, ci) => (
                              <tr key={cm.source_col} style={{ opacity: cm.transform === "skip" ? 0.45 : 1 }}>
                                <td style={{ fontFamily: "monospace", fontSize: "0.8rem" }}>
                                  {cm.source_col}
                                </td>
                                <td>
                                  <input
                                    type="text"
                                    value={cm.target_col}
                                    onChange={e => updateTargetCol(si, ci, e.target.value)}
                                    style={{
                                      width: "100%", padding: "3px 7px", borderRadius: 5,
                                      border: "1px solid var(--border)", background: "var(--bg)",
                                      color: "var(--text)", fontSize: "0.8rem", fontFamily: "monospace",
                                    }}
                                  />
                                </td>
                                <td>
                                  <select
                                    value={cm.transform}
                                    onChange={e => updateTransform(si, ci, e.target.value)}
                                    style={{
                                      padding: "3px 7px", borderRadius: 5, fontSize: "0.78rem",
                                      border: "1px solid var(--border)", background: "var(--bg)",
                                      color: "var(--text)", cursor: "pointer",
                                    }}
                                  >
                                    {TRANSFORMS.map(t => <option key={t} value={t}>{t}</option>)}
                                  </select>
                                </td>
                                <td style={{ fontSize: "0.75rem", color: "var(--text-dim)" }}>
                                  {cm.notes ?? ""}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </div>
                ))}

                <button
                  onClick={() => setPhase("confirm")}
                  style={{
                    padding: "11px 28px", background: "var(--accent)", color: "#fff",
                    border: "none", borderRadius: 8, fontWeight: 700,
                    fontSize: "0.95rem", cursor: "pointer", marginTop: 4,
                  }}
                >
                  Next: Review & Confirm →
                </button>
              </>
            )}
          </div>
        )}

        {/* ═══════════════ PHASE: CONFIRM ═══════════════ */}
        {phase === "confirm" && schema && mapping && confirmStats && (
          <div style={{ maxWidth: 600, width: "100%" }}>
            <h2 style={{ margin: "0 0 6px", fontSize: "1.1rem", fontWeight: 700 }}>
              Ready to import
            </h2>
            <p style={{ margin: "0 0 20px", fontSize: "0.875rem", color: "var(--text-dim)" }}>
              Review what will be imported, then click Execute.
            </p>

            <div className="mig-preview-card">
              <div className="mig-preview-title">📦 Import Summary</div>
              <table className="mig-preview-table" style={{ marginBottom: 14 }}>
                <thead>
                  <tr>
                    <th>Source Sheet</th>
                    <th>→ Target Table</th>
                    <th style={{ textAlign: "right" }}>Rows</th>
                  </tr>
                </thead>
                <tbody>
                  {mapping.sheet_mappings.filter(sm => sm.target_table !== "skip").map(sm => {
                    const sheet = schema.sheets.find(s => s.name === sm.source_sheet);
                    return (
                      <tr key={sm.source_sheet}>
                        <td style={{ fontFamily: "monospace", fontSize: "0.8rem" }}>{sm.source_sheet}</td>
                        <td style={{ fontWeight: 600 }}>{sm.target_table}</td>
                        <td style={{ textAlign: "right" }}>{(sheet?.row_count ?? 0).toLocaleString()}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>

              <div className="mig-preview-total">
                {confirmStats.sheets} sheet{confirmStats.sheets !== 1 ? "s" : ""} →{" "}
                {confirmStats.totalRows.toLocaleString()} rows →{" "}
                {confirmStats.tables.join(", ")}
              </div>

              {executeError && (
                <div style={{
                  marginTop: 10, padding: "10px 14px", borderRadius: 8,
                  background: "color-mix(in srgb, #ef4444 10%, transparent)",
                  border: "1px solid #ef444440", color: "#dc2626", fontSize: "0.84rem",
                }}>
                  ❌ {executeError}
                </div>
              )}

              <div style={{ display: "flex", gap: 10, marginTop: 14 }}>
                <button
                  onClick={() => setPhase("mapping")}
                  style={{
                    padding: "10px 18px", background: "transparent",
                    border: "1px solid var(--border)", borderRadius: 8,
                    cursor: "pointer", color: "var(--text-dim)", fontSize: "0.875rem",
                  }}
                >
                  ← Edit Mapping
                </button>
                <button className="mig-confirm-btn" onClick={handleExecute} style={{ flex: 1, margin: 0 }}>
                  🚀 Execute Migration
                </button>
              </div>
              <p style={{ margin: "8px 0 0", fontSize: "0.75rem", color: "var(--text-dim)", textAlign: "center" }}>
                Data will be inserted into ZANPOS. Existing records are not modified.
              </p>
            </div>
          </div>
        )}

        {/* ═══════════════ PHASE: EXECUTING ═══════════════ */}
        {phase === "executing" && (
          <div style={{ maxWidth: 600, width: "100%" }}>
            <h2 style={{ margin: "0 0 6px", fontSize: "1.1rem", fontWeight: 700 }}>
              Importing data…
            </h2>
            <p style={{ margin: "0 0 20px", fontSize: "0.875rem", color: "var(--text-dim)" }}>
              Please wait while ZANPOS imports your data.
            </p>

            {progressItems.length === 0 && (
              <div style={{ display: "flex", alignItems: "center", gap: 10, color: "var(--text-dim)", fontSize: "0.875rem" }}>
                <div className="mig-dot-pulse">
                  <span /><span /><span />
                </div>
                Starting…
              </div>
            )}

            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              {progressItems.map(p => {
                const pct = p.total > 0 ? Math.round((p.done / p.total) * 100) : (p.finished ? 100 : 0);
                return (
                  <div key={p.sheet}>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 5, fontSize: "0.84rem" }}>
                      <span style={{ fontWeight: 600 }}>
                        {p.sheet}
                        <span style={{ color: "var(--text-dim)", fontWeight: 400 }}> → {p.target}</span>
                      </span>
                      <span style={{ color: p.finished ? "#22c55e" : "var(--text-dim)" }}>
                        {p.finished
                          ? `✓ ${p.inserted ?? 0} inserted${(p.skipped ?? 0) > 0 ? `, ${p.skipped} skipped` : ""}`
                          : `${p.done.toLocaleString()} / ${p.total.toLocaleString()}`
                        }
                      </span>
                    </div>
                    <div className="mig-progress-bar">
                      <div
                        className="mig-progress-fill"
                        style={{
                          width: `${pct}%`,
                          background: p.finished ? "#22c55e" : "var(--accent)",
                        }}
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* ═══════════════ PHASE: DONE ═══════════════ */}
        {phase === "done" && (
          <div className="mig-done-screen" style={{ flex: 1 }}>
            <div style={{ fontSize: "3.5rem" }}>🎉</div>
            <h2 style={{ margin: 0, fontSize: "1.4rem", fontWeight: 700 }}>
              Migration Complete!
            </h2>
            {doneMessage && (
              <p style={{ margin: "8px 0 0", color: "var(--text-dim)", fontSize: "0.9rem", maxWidth: 500, textAlign: "center", lineHeight: 1.6 }}>
                <SimpleMarkdown text={doneMessage} />
              </p>
            )}

            {progressItems.length > 0 && (
              <div style={{
                marginTop: 16, padding: "14px 20px",
                background: "var(--surface)", border: "1px solid var(--border)",
                borderRadius: 10, fontSize: "0.84rem", minWidth: 280,
              }}>
                {progressItems.map(p => (
                  <div key={p.sheet} style={{ display: "flex", justifyContent: "space-between", padding: "3px 0" }}>
                    <span style={{ color: "var(--text-dim)" }}>{p.target}</span>
                    <span style={{ fontWeight: 600, color: "#22c55e" }}>
                      {(p.inserted ?? 0).toLocaleString()} rows
                    </span>
                  </div>
                ))}
              </div>
            )}

            <button className="mig-launch-btn" onClick={onDone} style={{ marginTop: 8 }}>
              Launch POS 🚀
            </button>
          </div>
        )}

      </div>
    </div>
  );
}
