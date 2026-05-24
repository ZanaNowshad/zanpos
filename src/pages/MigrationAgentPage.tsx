import React, { useState, useEffect, useRef, useCallback } from "react";
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
  "products","categories","customers","sales","sale_items",
  "stock_levels","payments","skip",
];
const TRANSFORMS = [
  "identity","price_minor","integer","boolean","date","phone","skip",
];

// ─── Types ────────────────────────────────────────────────────────────────────

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

interface ChatMsg {
  id: string;
  role: "assistant" | "user";
  text: string;
  node?: React.ReactNode;   // rich widget rendered below text
  ts: number;
}

interface Props {
  onDone: () => void;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function uid() { return Math.random().toString(36).slice(2); }

function SimpleMarkdown({ text }: { text: string }) {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  return (
    <>
      {parts.map((p, i) => {
        if (p.startsWith("**") && p.endsWith("**"))
          return <strong key={i}>{p.slice(2, -2)}</strong>;
        if (p.startsWith("`") && p.endsWith("`"))
          return <code key={i} style={{ background:"var(--border)", padding:"1px 5px", borderRadius:3, fontSize:"0.85em" }}>{p.slice(1,-1)}</code>;
        return p.split("\n").map((line, j, arr) => (
          <React.Fragment key={`${i}-${j}`}>{line}{j < arr.length-1 && <br />}</React.Fragment>
        ));
      })}
    </>
  );
}

// ─── AI Config Setup Panel ────────────────────────────────────────────────────

function AiSetupPanel({ onConfigured }: { onConfigured: () => void }) {
  const [provider, setProvider]     = useState<AiProvider>("anthropic");
  const [anthropicKey, setAnthropicKey] = useState("");
  const [baseUrl, setBaseUrl]       = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]   = useState("");
  const [models, setModels]         = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating] = useState(false);
  const [saving, setSaving]         = useState(false);
  const [error, setError]           = useState<string | null>(null);

  const inp: React.CSSProperties = {
    width:"100%", padding:"9px 12px", borderRadius:7,
    border:"1px solid var(--border)", background:"var(--bg)",
    color:"var(--text)", fontSize:"0.875rem", boxSizing:"border-box", marginBottom:12,
  };
  const lbl: React.CSSProperties = {
    display:"block", fontSize:"0.8rem", fontWeight:600, marginBottom:5, color:"var(--text-dim)",
  };

  const handleSaveAnthropic = async () => {
    const key = anthropicKey.trim();
    if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
    setSaving(true); setError(null);
    try { await adminSetAnthropic(key); onConfigured(); }
    catch (e) { setError(String(e)); } finally { setSaving(false); }
  };

  const handleValidateOpenai = async () => {
    setError(null); setModels([]); setSelectedModel(""); setValidating(true);
    try {
      const res = await adminValidateOpenai(baseUrl.trim(), openaiKey.trim());
      if (res.success && res.models.length > 0) {
        setModels(res.models); setSelectedModel(res.models[0].id);
      } else setError(res.error ?? "Validation failed");
    } catch (e) { setError(String(e)); } finally { setValidating(false); }
  };

  const handleSaveOpenai = async () => {
    if (!selectedModel) return;
    setSaving(true); setError(null);
    try { await adminSetOpenai(baseUrl.trim(), openaiKey.trim(), selectedModel); onConfigured(); }
    catch (e) { setError(String(e)); } finally { setSaving(false); }
  };

  return (
    <div className="mig-page" style={{ alignItems:"center", justifyContent:"center" }}>
      <div style={{ maxWidth:500, width:"100%", padding:32, background:"var(--surface)", borderRadius:12, border:"1px solid var(--border)", margin:24 }}>
        <div style={{ fontSize:"2rem", marginBottom:10 }}>🤖</div>
        <h2 style={{ margin:"0 0 6px", fontSize:"1.1rem", fontWeight:700 }}>Set up AI before migrating</h2>
        <p style={{ margin:"0 0 20px", fontSize:"0.85rem", color:"var(--text-dim)", lineHeight:1.5 }}>
          Choose your AI provider. Keys are stored securely on this device.
        </p>
        <div style={{ display:"flex", gap:8, marginBottom:24 }}>
          {(["anthropic","openai"] as AiProvider[]).map(p => (
            <button key={p} onClick={() => { setProvider(p); setError(null); }}
              style={{ flex:1, padding:"8px 0", borderRadius:8, fontWeight:600, fontSize:"0.85rem", cursor:"pointer", transition:"all 0.15s",
                background:provider===p?"var(--accent)":"var(--surface)", color:provider===p?"#fff":"var(--text-dim)",
                border:`1.5px solid ${provider===p?"var(--accent)":"var(--border)"}` }}>
              {p==="anthropic"?"◆ Anthropic (Claude)":"⬡ OpenAI / Custom"}
            </button>
          ))}
        </div>
        {provider === "anthropic" && (
          <>
            <label style={lbl}>Anthropic API Key</label>
            <input type="password" placeholder="sk-ant-api03-…" value={anthropicKey} autoFocus
              onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
              onKeyDown={e => e.key==="Enter" && handleSaveAnthropic()} style={inp} />
            {error && <div style={{ color:"#ef4444", fontSize:"0.8rem", marginBottom:10 }}>{error}</div>}
            <button onClick={handleSaveAnthropic} disabled={saving || !anthropicKey.trim()}
              style={{ width:"100%", padding:10, background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.9rem",
                cursor:saving||!anthropicKey.trim()?"not-allowed":"pointer", opacity:saving||!anthropicKey.trim()?0.5:1 }}>
              {saving?"Saving…":"Save & Start Migration →"}
            </button>
            <p style={{ margin:"10px 0 0", fontSize:"0.75rem", color:"var(--text-dim)", textAlign:"center" }}>
              Get your key at <span style={{ color:"var(--accent)" }}>console.anthropic.com</span>
            </p>
          </>
        )}
        {provider === "openai" && (
          <>
            <label style={lbl}>Base URL</label>
            <input type="text" value={baseUrl}
              onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }} style={inp} />
            <label style={lbl}>API Key</label>
            <input type="password" placeholder="sk-…" value={openaiKey}
              onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }}
              style={{ ...inp, marginBottom:14 }} />
            {error && <div style={{ color:"#ef4444", fontSize:"0.8rem", marginBottom:10 }}>{error}</div>}
            {models.length === 0 ? (
              <button onClick={handleValidateOpenai} disabled={validating||!baseUrl.trim()||!openaiKey.trim()}
                style={{ width:"100%", padding:10, background:"var(--surface2,var(--surface))", color:"var(--text)", border:"1.5px solid var(--border)",
                  borderRadius:8, fontWeight:600, fontSize:"0.875rem", cursor:validating||!baseUrl.trim()||!openaiKey.trim()?"not-allowed":"pointer",
                  opacity:validating||!baseUrl.trim()||!openaiKey.trim()?0.5:1, marginBottom:8 }}>
                {validating?"Connecting…":"Validate & Fetch Models"}
              </button>
            ) : (
              <>
                <label style={lbl}>Model</label>
                <select value={selectedModel} onChange={e => setSelectedModel(e.target.value)} style={{ ...inp, marginBottom:14 }}>
                  {models.map(m => <option key={m.id} value={m.id}>{m.id}</option>)}
                </select>
                <button onClick={handleSaveOpenai} disabled={saving||!selectedModel}
                  style={{ width:"100%", padding:10, background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.9rem",
                    cursor:saving||!selectedModel?"not-allowed":"pointer", opacity:saving||!selectedModel?0.5:1 }}>
                  {saving?"Saving…":"Save & Start Migration →"}
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

function AiSettingsDrawer({ onClose, onSaved }: { onClose: () => void; onSaved: (label: string) => void }) {
  const [provider, setProvider]     = useState<AiProvider>("anthropic");
  const [anthropicKey, setAnthropicKey] = useState("");
  const [baseUrl, setBaseUrl]       = useState("https://api.openai.com/v1");
  const [openaiKey, setOpenaiKey]   = useState("");
  const [models, setModels]         = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [validating, setValidating] = useState(false);
  const [saving, setSaving]         = useState(false);
  const [error, setError]           = useState<string | null>(null);

  useEffect(() => {
    adminGetProviderConfig().then(cfg => {
      if (cfg.provider === "openai") {
        setProvider("openai");
        if (cfg.openai_base_url) setBaseUrl(cfg.openai_base_url);
        if (cfg.openai_model)    setSelectedModel(cfg.openai_model);
      }
    }).catch(() => {});
  }, []);

  const inp: React.CSSProperties = { width:"100%", padding:"8px 11px", borderRadius:7, border:"1px solid var(--border)", background:"var(--bg)", color:"var(--text)", fontSize:"0.85rem", boxSizing:"border-box", marginBottom:10 };
  const lbl: React.CSSProperties = { display:"block", fontSize:"0.78rem", fontWeight:600, marginBottom:4, color:"var(--text-dim)" };

  return (
    <>
      <div onClick={onClose} style={{ position:"fixed", inset:0, background:"rgba(0,0,0,0.45)", zIndex:40 }} />
      <div style={{ position:"fixed", top:0, right:0, bottom:0, width:340, background:"var(--surface)", borderLeft:"1px solid var(--border)", zIndex:41, display:"flex", flexDirection:"column", boxShadow:"-4px 0 24px rgba(0,0,0,0.3)" }}>
        <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", padding:"14px 16px", borderBottom:"1px solid var(--border)" }}>
          <span style={{ fontWeight:700, fontSize:"0.95rem" }}>⚙ AI Model Settings</span>
          <button onClick={onClose} style={{ background:"none", border:"none", cursor:"pointer", color:"var(--text-dim)", fontSize:"1.2rem", lineHeight:1, padding:2 }}>✕</button>
        </div>
        <div style={{ flex:1, overflowY:"auto", padding:16 }}>
          <div style={{ display:"flex", gap:6, marginBottom:18 }}>
            {(["anthropic","openai"] as AiProvider[]).map(p => (
              <button key={p} onClick={() => { setProvider(p); setError(null); setModels([]); }}
                style={{ flex:1, padding:"7px 0", borderRadius:7, fontWeight:600, fontSize:"0.8rem", cursor:"pointer", transition:"all 0.15s",
                  background:provider===p?"var(--accent)":"transparent", color:provider===p?"#fff":"var(--text-dim)",
                  border:`1.5px solid ${provider===p?"var(--accent)":"var(--border)"}` }}>
                {p==="anthropic"?"◆ Anthropic":"⬡ OpenAI / Custom"}
              </button>
            ))}
          </div>
          {provider === "anthropic" && (
            <>
              <label style={lbl}>Anthropic API Key</label>
              <input type="password" placeholder="sk-ant-api03-…" value={anthropicKey} autoFocus
                onChange={e => { setAnthropicKey(e.target.value); setError(null); }}
                onKeyDown={e => e.key==="Enter" && (async () => {
                  const key = anthropicKey.trim();
                  if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
                  setSaving(true); setError(null);
                  try { await adminSetAnthropic(key); onSaved("◆ Claude"); }
                  catch (ex) { setError(String(ex)); } finally { setSaving(false); }
                })()} style={inp} />
              {error && <div style={{ color:"#ef4444", fontSize:"0.78rem", marginBottom:8 }}>{error}</div>}
              <button onClick={async () => {
                const key = anthropicKey.trim();
                if (!key.startsWith("sk-ant-")) { setError("Key must start with sk-ant-"); return; }
                setSaving(true); setError(null);
                try { await adminSetAnthropic(key); onSaved("◆ Claude"); }
                catch (ex) { setError(String(ex)); } finally { setSaving(false); }
              }} disabled={saving||!anthropicKey.trim()}
                style={{ width:"100%", padding:9, background:"var(--accent)", color:"#fff", border:"none", borderRadius:7, fontWeight:700, fontSize:"0.875rem", cursor:saving||!anthropicKey.trim()?"not-allowed":"pointer", opacity:saving||!anthropicKey.trim()?0.5:1 }}>
                {saving?"Saving…":"Save Changes"}
              </button>
            </>
          )}
          {provider === "openai" && (
            <>
              <label style={lbl}>Base URL</label>
              <input type="text" value={baseUrl} onChange={e => { setBaseUrl(e.target.value); setModels([]); setSelectedModel(""); setError(null); }} style={inp} />
              <label style={lbl}>API Key</label>
              <input type="password" placeholder="sk-…" value={openaiKey} onChange={e => { setOpenaiKey(e.target.value); setModels([]); setSelectedModel(""); setError(null); }} style={{ ...inp, marginBottom:12 }} />
              {error && <div style={{ color:"#ef4444", fontSize:"0.78rem", marginBottom:8 }}>{error}</div>}
              {models.length === 0 ? (
                <button onClick={async () => {
                  setError(null); setModels([]); setSelectedModel(""); setValidating(true);
                  try {
                    const res = await adminValidateOpenai(baseUrl.trim(), openaiKey.trim());
                    if (res.success && res.models.length > 0) { setModels(res.models); setSelectedModel(res.models[0].id); }
                    else setError(res.error ?? "Validation failed");
                  } catch (ex) { setError(String(ex)); } finally { setValidating(false); }
                }} disabled={validating||!baseUrl.trim()||!openaiKey.trim()}
                  style={{ width:"100%", padding:9, background:"transparent", color:"var(--text)", border:"1.5px solid var(--border)", borderRadius:7, fontWeight:600, fontSize:"0.875rem", cursor:validating||!baseUrl.trim()||!openaiKey.trim()?"not-allowed":"pointer", opacity:validating||!baseUrl.trim()||!openaiKey.trim()?0.5:1 }}>
                  {validating?"Connecting…":"Validate & Fetch Models"}
                </button>
              ) : (
                <>
                  <label style={lbl}>Model</label>
                  <select value={selectedModel} onChange={e => setSelectedModel(e.target.value)} style={inp}>
                    {models.map(m => <option key={m.id} value={m.id}>{m.id}</option>)}
                  </select>
                  <button onClick={async () => {
                    if (!selectedModel) return;
                    setSaving(true); setError(null);
                    try { await adminSetOpenai(baseUrl.trim(), openaiKey.trim(), selectedModel); onSaved(`⬡ ${selectedModel}`); }
                    catch (ex) { setError(String(ex)); } finally { setSaving(false); }
                  }} disabled={saving||!selectedModel}
                    style={{ width:"100%", padding:9, background:"var(--accent)", color:"#fff", border:"none", borderRadius:7, fontWeight:700, fontSize:"0.875rem", cursor:saving||!selectedModel?"not-allowed":"pointer", opacity:saving||!selectedModel?0.5:1 }}>
                    {saving?"Saving…":"Save Changes"}
                  </button>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}

// ─── Mapping Drawer ───────────────────────────────────────────────────────────

function MappingDrawer({
  mapping,
  schema,
  onClose,
  onUpdate,
}: {
  mapping: MappingConfig;
  schema: FileSchema;
  onClose: () => void;
  onUpdate: (m: MappingConfig) => void;
}) {
  const [local, setLocal] = useState<MappingConfig>(mapping);

  const updateTargetTable = (si: number, val: string) => {
    setLocal(prev => {
      const sheets = [...prev.sheet_mappings];
      sheets[si] = { ...sheets[si], target_table: val };
      return { sheet_mappings: sheets };
    });
  };
  const updateTargetCol = (si: number, ci: number, val: string) => {
    setLocal(prev => {
      const sheets = [...prev.sheet_mappings];
      const cols = [...sheets[si].column_mappings];
      cols[ci] = { ...cols[ci], target_col: val };
      sheets[si] = { ...sheets[si], column_mappings: cols };
      return { sheet_mappings: sheets };
    });
  };
  const updateTransform = (si: number, ci: number, val: string) => {
    setLocal(prev => {
      const sheets = [...prev.sheet_mappings];
      const cols = [...sheets[si].column_mappings];
      cols[ci] = { ...cols[ci], transform: val };
      sheets[si] = { ...sheets[si], column_mappings: cols };
      return { sheet_mappings: sheets };
    });
  };

  const inp: React.CSSProperties = { padding:"3px 7px", borderRadius:5, border:"1px solid var(--border)", background:"var(--bg)", color:"var(--text)", fontSize:"0.8rem", fontFamily:"monospace", width:"100%" };

  return (
    <>
      <div onClick={onClose} style={{ position:"fixed", inset:0, background:"rgba(0,0,0,0.5)", zIndex:50 }} />
      <div style={{ position:"fixed", top:0, right:0, bottom:0, width:"min(720px, 95vw)", background:"var(--surface)", borderLeft:"1px solid var(--border)", zIndex:51, display:"flex", flexDirection:"column", boxShadow:"-6px 0 32px rgba(0,0,0,0.35)" }}>
        <div style={{ display:"flex", alignItems:"center", justifyContent:"space-between", padding:"14px 20px", borderBottom:"1px solid var(--border)", flexShrink:0 }}>
          <span style={{ fontWeight:700, fontSize:"1rem" }}>🗂 Review Column Mapping</span>
          <button onClick={onClose} style={{ background:"none", border:"none", cursor:"pointer", color:"var(--text-dim)", fontSize:"1.3rem", lineHeight:1, padding:2 }}>✕</button>
        </div>
        <p style={{ margin:"10px 20px 0", fontSize:"0.82rem", color:"var(--text-dim)", flexShrink:0 }}>
          Review and adjust how each source column maps to ZANPOS. Set to <strong>skip</strong> to ignore a column.
        </p>
        <div style={{ flex:1, overflowY:"auto", padding:"12px 20px 20px" }}>
          {local.sheet_mappings.map((sm, si) => {
            const sheetInfo = schema.sheets.find(s => s.name === sm.source_sheet);
            return (
              <div key={sm.source_sheet} style={{ marginBottom:22 }}>
                <div style={{ display:"flex", alignItems:"center", gap:10, marginBottom:8, paddingBottom:6, borderBottom:"1px solid var(--border2,var(--border))" }}>
                  <span style={{ fontWeight:700, fontSize:"0.9rem" }}>{sm.source_sheet}</span>
                  <span style={{ color:"var(--text-dim)", fontSize:"0.85rem" }}>→</span>
                  <select value={sm.target_table} onChange={e => updateTargetTable(si, e.target.value)}
                    style={{ padding:"3px 8px", borderRadius:6, fontSize:"0.82rem", fontWeight:600, border:"1.5px solid var(--accent)", background:"var(--bg)", color:"var(--text)", cursor:"pointer" }}>
                    {TARGET_TABLES.map(t => <option key={t} value={t}>{t}</option>)}
                  </select>
                  {sheetInfo && <span style={{ marginLeft:"auto", fontSize:"0.75rem", color:"var(--text-dim)" }}>{sheetInfo.row_count.toLocaleString()} rows</span>}
                  {sm.target_table === "skip" && <span style={{ fontSize:"0.74rem", color:"#f59e0b" }}>⏭ skipped</span>}
                </div>
                {sm.target_table !== "skip" && (
                  <table style={{ width:"100%", borderCollapse:"collapse", fontSize:"0.82rem" }}>
                    <thead>
                      <tr>
                        <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)", width:"30%" }}>Source Column</th>
                        <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)", width:"32%" }}>→ Target Column</th>
                        <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)", width:"20%" }}>Transform</th>
                        <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)" }}>Notes</th>
                      </tr>
                    </thead>
                    <tbody>
                      {sm.column_mappings.map((cm, ci) => (
                        <tr key={cm.source_col} style={{ opacity:cm.transform==="skip"?0.45:1 }}>
                          <td style={{ padding:"5px 8px", fontFamily:"monospace", borderBottom:"1px solid var(--border2,var(--border))" }}>{cm.source_col}</td>
                          <td style={{ padding:"5px 8px", borderBottom:"1px solid var(--border2,var(--border))" }}>
                            <input type="text" value={cm.target_col} onChange={e => updateTargetCol(si, ci, e.target.value)} style={inp} />
                          </td>
                          <td style={{ padding:"5px 8px", borderBottom:"1px solid var(--border2,var(--border))" }}>
                            <select value={cm.transform} onChange={e => updateTransform(si, ci, e.target.value)}
                              style={{ padding:"3px 7px", borderRadius:5, fontSize:"0.78rem", border:"1px solid var(--border)", background:"var(--bg)", color:"var(--text)", cursor:"pointer" }}>
                              {TRANSFORMS.map(t => <option key={t} value={t}>{t}</option>)}
                            </select>
                          </td>
                          <td style={{ padding:"5px 8px", fontSize:"0.74rem", color:"var(--text-dim)", borderBottom:"1px solid var(--border2,var(--border))" }}>{cm.notes ?? ""}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            );
          })}
        </div>
        <div style={{ padding:"14px 20px", borderTop:"1px solid var(--border)", display:"flex", gap:10, flexShrink:0 }}>
          <button onClick={onClose}
            style={{ padding:"9px 18px", background:"transparent", border:"1px solid var(--border)", borderRadius:8, cursor:"pointer", color:"var(--text-dim)", fontSize:"0.875rem" }}>
            Cancel
          </button>
          <button onClick={() => { onUpdate(local); onClose(); }}
            style={{ flex:1, padding:"10px 0", background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.9rem", cursor:"pointer" }}>
            ✅ Confirm Mapping
          </button>
        </div>
      </div>
    </>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function MigrationAgentPage({ onDone }: Props) {
  const [aiReady, setAiReady]               = useState<boolean | null>(null);
  const [providerLabel, setProviderLabel]   = useState("⚙ AI");
  const [showAiSettings, setShowAiSettings] = useState(false);

  const [messages, setMessages]             = useState<ChatMsg[]>([]);
  const [input, setInput]                   = useState("");
  const [isDragOver, setIsDragOver]         = useState(false);

  // Working state (kept separate from messages so widgets can reference live values)
  const [schema, setSchema]                 = useState<FileSchema | null>(null);
  const [mapping, setMapping]               = useState<MappingConfig | null>(null);
  const [showMappingDrawer, setShowMappingDrawer] = useState(false);
  const [progressItems, setProgressItems]   = useState<SheetProgress[]>([]);

  const bottomRef   = useRef<HTMLDivElement>(null);
  const inputRef    = useRef<HTMLInputElement>(null);
  const progressRef = useRef<SheetProgress[]>([]);
  // Stable ref so runExecute can read the current file path without stale closure
  const filePathRef = useRef<string | null>(null);

  // ── Scroll to bottom on new messages ──────────────────────────────────────
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // ── Helpers ────────────────────────────────────────────────────────────────
  const addMsg = useCallback((role: "assistant" | "user", text: string, node?: React.ReactNode): string => {
    const id = uid();
    setMessages(prev => [...prev, { id, role, text, node, ts: Date.now() }]);
    return id;
  }, []);

  const updateMsgNode = useCallback((id: string, node: React.ReactNode) => {
    setMessages(prev => prev.map(m => m.id === id ? { ...m, node } : m));
  }, []);

  // ── Check AI on mount ──────────────────────────────────────────────────────
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

  // ── Welcome message (once AI is ready) ────────────────────────────────────
  useEffect(() => {
    if (aiReady !== true) return;
    const fileBtnId = uid();
    setMessages([{
      id: fileBtnId,
      role: "assistant",
      text: "👋 Hi! I'm the **ZANPOS Migration Agent**.\n\nI'll help you import your existing POS data — products, categories, customers, sales history, and stock levels — into ZANPOS.\n\nI support **Excel** (.xlsx / .xls), **CSV**, and **SQLite** databases. Drop your file here, or click the button below to browse:",
      node: <WelcomeFilePicker onFile={handleFile} isDragOver={isDragOver} />,
      ts: Date.now(),
    }]);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aiReady]);

  // ── File handling ──────────────────────────────────────────────────────────
  const handleFile = useCallback(async (path: string) => {
    filePathRef.current = path;
    const name = path.replace(/\\/g, "/").split("/").pop() ?? path;
    addMsg("user", `📄 ${name}`);

    const thinkId = addMsg("assistant", "🔍 Reading your file structure…");

    try {
      const s = await migrationInspectFile(path);
      setSchema(s);
      updateMsgNode(thinkId, undefined);
      setMessages(prev => prev.map(m => m.id === thinkId ? {
        ...m,
        text: `✅ Got it! Here's what I found in **${name}**:`,
        node: <SchemaPreviewCard schema={s} onNext={() => runAiMap(s)} />,
      } : m));
    } catch (e) {
      setMessages(prev => prev.map(m => m.id === thinkId ? {
        ...m,
        text: `❌ Couldn't read that file: ${String(e)}\n\nTry another file:`,
        node: <WelcomeFilePicker onFile={handleFile} isDragOver={false} compact />,
      } : m));
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addMsg, updateMsgNode]);

  const handlePickFile = async () => {
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          { name: "Database / Spreadsheet", extensions: ["db","sqlite","sqlite3","csv","xlsx","xls"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (!selected) return;
      const path = Array.isArray(selected) ? selected[0] : selected as string;
      if (path) await handleFile(path);
    } catch { /* user cancelled */ }
  };

  // ── AI Mapping ─────────────────────────────────────────────────────────────
  const runAiMap = useCallback(async (s: FileSchema) => {
    const thinkId = addMsg("assistant", "✨ Analysing your schema and mapping to ZANPOS tables…\n\nThis usually takes 5–15 seconds.");
    try {
      const m = await migrationAiMap(s, 3);
      setMapping(m);
      const active = m.sheet_mappings.filter(sm => sm.target_table !== "skip");
      const totalRows = active.reduce((sum, sm) => {
        const sheet = s.sheets.find(sh => sh.name === sm.source_sheet);
        return sum + (sheet?.row_count ?? 0);
      }, 0);
      const tables = [...new Set(active.map(sm => sm.target_table))];
      setMessages(prev => prev.map(msg => msg.id === thinkId ? {
        ...msg,
        text: `✨ Mapping complete! I'll import **${totalRows.toLocaleString()} rows** into ${tables.join(", ")}.\n\nReview the mapping below before executing. You can adjust column assignments and transforms if needed:`,
        node: <MappingConfirmCard
          mapping={m}
          schema={s}
          onEdit={() => setShowMappingDrawer(true)}
          onExecute={() => runExecute(m, s)}
        />,
      } : msg));
    } catch (e) {
      setMessages(prev => prev.map(msg => msg.id === thinkId ? {
        ...msg,
        text: `❌ AI mapping failed: ${String(e)}\n\nWant to try again?`,
        node: <RetryButton label="♻ Retry AI Mapping" onClick={() => runAiMap(s)} />,
      } : msg));
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addMsg]);

  // ── Execute ────────────────────────────────────────────────────────────────
  const runExecute = useCallback(async (m: MappingConfig, _s: FileSchema) => {
    if (!m) return;
    const storedPath = filePathRef.current;
    if (!storedPath) {
      addMsg("assistant", "⚠️ Couldn't find the file path. Please re-select your file.");
      return;
    }

    const progressMsgId = addMsg("assistant", "🚀 Starting migration…");
    progressRef.current = [];
    setProgressItems([]);

    const channel = new Channel<MigrationProgress>();
    channel.onmessage = (event: MigrationProgress) => {
      if (event.type === "sheet_start") {
        const item: SheetProgress = { sheet: event.sheet, target: event.target, total: event.total_rows, done: 0, finished: false };
        progressRef.current = [...progressRef.current, item];
        setProgressItems([...progressRef.current]);
      } else if (event.type === "sheet_progress") {
        progressRef.current = progressRef.current.map(p => p.sheet === event.sheet ? { ...p, done: event.done } : p);
        setProgressItems([...progressRef.current]);
      } else if (event.type === "sheet_done") {
        progressRef.current = progressRef.current.map(p =>
          p.sheet === event.sheet ? { ...p, done: p.total, inserted: event.inserted, skipped: event.skipped, finished: true } : p
        );
        setProgressItems([...progressRef.current]);
      } else if (event.type === "done") {
        const items = [...progressRef.current];
        const total = items.reduce((n, p) => n + (p.inserted ?? 0), 0);
        setMessages(prev => prev.map(msg => msg.id === progressMsgId ? {
          ...msg,
          text: `🎉 **Migration complete!** ${total.toLocaleString()} records imported into ZANPOS.`,
          node: <DoneCard items={items} onLaunch={onDone} />,
        } : msg));
      } else if (event.type === "error") {
        setMessages(prev => prev.map(msg => msg.id === progressMsgId ? {
          ...msg,
          text: `❌ Migration failed: ${event.message}`,
        } : msg));
      }
    };

    try {
      await migrationExecute(storedPath, m, 3, channel);
    } catch (e) {
      setMessages(prev => prev.map(msg => msg.id === progressMsgId ? {
        ...msg,
        text: `❌ Migration error: ${String(e)}`,
      } : msg));
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addMsg, messages, onDone]);

  // ── Page drag-drop ─────────────────────────────────────────────────────────
  const handlePageDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const path = (file as File & { path?: string }).path;
    if (path) await handleFile(path);
  };

  // ── Text input ─────────────────────────────────────────────────────────────
  const handleSend = () => {
    const text = input.trim();
    if (!text) return;
    setInput("");
    addMsg("user", text);

    const lc = text.toLowerCase();
    if (/mysql|mssql|sql server|postgr|oracle|mongo/.test(lc)) {
      addMsg("assistant", "For MySQL, MSSQL, or other network databases, the easiest path is to export a CSV or Excel file from that system first, then import it here.\n\nMost POS systems have an **Export** or **Backup** option in their settings. Export your products, customers, and sales as CSV, then drop them here.");
    } else if (/help|what|how|support|format|type/.test(lc)) {
      addMsg("assistant", "I support these file formats:\n\n- **Excel** (.xlsx, .xls) — multi-sheet files work great\n- **CSV** (.csv) — one table per file\n- **SQLite** (.db, .sqlite, .sqlite3) — full database files\n\nDrop your file anywhere on this screen, or click **Browse File** below.");
    } else if (/skip|cancel|later|no/.test(lc)) {
      addMsg("assistant", "No problem! You can always run the migration later from the Settings panel. Click **Skip → Launch POS** in the top right to continue.");
    } else {
      addMsg("assistant", "Got it! To get started, just drop your file anywhere on this screen or click **Browse File** in my first message above. I'll take it from there. 👆");
    }
  };

  // ── Loading / AI gate ──────────────────────────────────────────────────────
  if (aiReady === null) {
    return (
      <div className="mig-page" style={{ alignItems:"center", justifyContent:"center" }}>
        <div style={{ color:"var(--text-dim)", fontSize:"0.9rem" }}>Checking AI configuration…</div>
      </div>
    );
  }
  if (aiReady === false) {
    return <AiSetupPanel onConfigured={() => setAiReady(true)} />;
  }

  return (
    <div
      className="mig-page"
      onDragOver={e => { e.preventDefault(); setIsDragOver(true); }}
      onDragLeave={e => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setIsDragOver(false); }}
      onDrop={handlePageDrop}
      style={{ position:"relative" }}
    >
      {/* Drag overlay */}
      {isDragOver && (
        <div style={{ position:"absolute", inset:0, zIndex:100, background:"rgba(240,165,0,0.12)", border:"3px dashed var(--accent)", borderRadius:12, display:"flex", alignItems:"center", justifyContent:"center", pointerEvents:"none" }}>
          <div style={{ fontSize:"2rem", fontWeight:700, color:"var(--accent)" }}>📂 Drop to import</div>
        </div>
      )}

      {/* Header */}
      <div className="mig-header">
        <span className="mig-header-title">🤖 Migration Agent</span>
        <div style={{ marginLeft:"auto", display:"flex", gap:6, alignItems:"center" }}>
          <button onClick={() => setShowAiSettings(true)}
            style={{ padding:"4px 10px", borderRadius:6, cursor:"pointer", background:"var(--accent)", color:"#fff", border:"none", fontSize:"0.78rem", fontWeight:600, opacity:0.9 }}>
            {providerLabel}
          </button>
          <button onClick={onDone}
            style={{ padding:"4px 12px", background:"transparent", border:"1px solid var(--border)", borderRadius:6, cursor:"pointer", color:"var(--text-dim)", fontSize:"0.8rem" }}>
            Skip → Launch POS
          </button>
        </div>
      </div>

      {/* AI Settings Drawer */}
      {showAiSettings && (
        <AiSettingsDrawer onClose={() => setShowAiSettings(false)} onSaved={label => { setProviderLabel(label); setShowAiSettings(false); }} />
      )}

      {/* Mapping Drawer */}
      {showMappingDrawer && mapping && schema && (
        <MappingDrawer
          mapping={mapping}
          schema={schema}
          onClose={() => setShowMappingDrawer(false)}
          onUpdate={m => { setMapping(m); setShowMappingDrawer(false); }}
        />
      )}

      {/* Chat messages */}
      <div style={{ flex:1, overflowY:"auto", padding:"20px 0" }}>
        {messages.map(msg => (
          <div key={msg.id} className={`mig-chat-row mig-chat-row--${msg.role}`}>
            {msg.role === "assistant" && (
              <div className="mig-avatar">🤖</div>
            )}
            <div className={`mig-bubble mig-bubble--${msg.role}`}>
              <div className="mig-bubble-text">
                <SimpleMarkdown text={msg.text} />
              </div>
              {msg.node && <div className="mig-bubble-widget">{msg.node}</div>}
            </div>
          </div>
        ))}

        {/* Live progress injected under last message if executing */}
        {progressItems.length > 0 && progressItems.some(p => !p.finished) && (
          <div style={{ padding:"0 20px 12px" }}>
            <LiveProgress items={progressItems} />
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Input bar */}
      <div className="mig-input-bar">
        <button onClick={handlePickFile} className="mig-browse-btn" title="Browse for file">
          📂
        </button>
        <input
          ref={inputRef}
          className="mig-input"
          placeholder="Ask a question or type a message…"
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
        />
        <button className="mig-send-btn" onClick={handleSend} disabled={!input.trim()}>
          Send ↵
        </button>
      </div>
    </div>
  );
}

// ─── Inline widgets ───────────────────────────────────────────────────────────

function WelcomeFilePicker({ onFile, isDragOver, compact }: { onFile: (p: string) => void; isDragOver: boolean; compact?: boolean }) {
  const handlePick = async () => {
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          { name: "Database / Spreadsheet", extensions: ["db","sqlite","sqlite3","csv","xlsx","xls"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (!selected) return;
      const path = Array.isArray(selected) ? selected[0] : selected as string;
      if (path) onFile(path);
    } catch { /* cancelled */ }
  };

  if (compact) {
    return (
      <button onClick={handlePick}
        style={{ marginTop:10, padding:"9px 18px", background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.875rem", cursor:"pointer" }}>
        📂 Browse File
      </button>
    );
  }

  return (
    <div style={{ marginTop:14 }}>
      <div
        onClick={handlePick}
        className={`mig-dropzone${isDragOver?" mig-dropzone--over":""}`}
        style={{ cursor:"pointer" }}
      >
        <div style={{ fontSize:"2rem", marginBottom:8 }}>📂</div>
        <div style={{ fontWeight:600, marginBottom:3 }}>Click to browse or drag & drop</div>
        <div style={{ fontSize:"0.78rem", color:"var(--text-dim)" }}>Excel · CSV · SQLite</div>
      </div>
    </div>
  );
}

function SchemaPreviewCard({ schema, onNext }: { schema: FileSchema; onNext: () => void }) {
  return (
    <div style={{ marginTop:12, display:"flex", flexDirection:"column", gap:8 }}>
      <div style={{ fontSize:"0.8rem", color:"var(--text-dim)", fontWeight:600, marginBottom:2 }}>
        {schema.file_type.toUpperCase()} · {schema.sheets.length} sheet{schema.sheets.length !== 1 ? "s" : ""}
      </div>
      {schema.sheets.map(sheet => (
        <div key={sheet.name} className="mig-schema-sheet">
          <div style={{ display:"flex", justifyContent:"space-between", alignItems:"baseline", marginBottom:5 }}>
            <span style={{ fontWeight:700, fontSize:"0.88rem" }}>{sheet.name}</span>
            <span style={{ fontSize:"0.74rem", color:"var(--text-dim)" }}>{sheet.row_count.toLocaleString()} rows</span>
          </div>
          <div style={{ display:"flex", flexWrap:"wrap", gap:"3px 6px" }}>
            {sheet.columns.slice(0, 10).map(col => (
              <span key={col.name}
                style={{ padding:"1px 7px", borderRadius:10, background:"var(--bg)", border:"1px solid var(--border)", fontSize:"0.74rem", color:"var(--text)" }}
                title={col.samples.length ? `Samples: ${col.samples.join(", ")}` : undefined}>
                {col.name}
              </span>
            ))}
            {sheet.columns.length > 10 && <span style={{ fontSize:"0.74rem", color:"var(--text-dim)" }}>+{sheet.columns.length - 10} more</span>}
          </div>
        </div>
      ))}
      <button onClick={onNext}
        style={{ marginTop:6, padding:"10px 20px", background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.9rem", cursor:"pointer", alignSelf:"flex-start" }}>
        ✨ Map with AI
      </button>
    </div>
  );
}

function MappingConfirmCard({ mapping, schema, onEdit, onExecute }: { mapping: MappingConfig; schema: FileSchema; onEdit: () => void; onExecute: () => void }) {
  const active = mapping.sheet_mappings.filter(sm => sm.target_table !== "skip");
  const totalRows = active.reduce((sum, sm) => {
    const sheet = schema.sheets.find(s => s.name === sm.source_sheet);
    return sum + (sheet?.row_count ?? 0);
  }, 0);

  return (
    <div style={{ marginTop:12, padding:"14px 16px", background:"var(--surface2,var(--bg))", border:"1px solid var(--border)", borderRadius:10 }}>
      <table style={{ width:"100%", borderCollapse:"collapse", fontSize:"0.84rem", marginBottom:12 }}>
        <thead>
          <tr>
            <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)" }}>Sheet</th>
            <th style={{ textAlign:"left", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)" }}>→ Table</th>
            <th style={{ textAlign:"right", padding:"4px 8px", color:"var(--text-dim)", fontWeight:600, borderBottom:"1px solid var(--border)" }}>Rows</th>
          </tr>
        </thead>
        <tbody>
          {mapping.sheet_mappings.map(sm => {
            const sheet = schema.sheets.find(s => s.name === sm.source_sheet);
            return (
              <tr key={sm.source_sheet}>
                <td style={{ padding:"4px 8px", fontFamily:"monospace", fontSize:"0.8rem" }}>{sm.source_sheet}</td>
                <td style={{ padding:"4px 8px", fontWeight:600, color:sm.target_table==="skip"?"var(--text-muted)":"var(--text)" }}>{sm.target_table === "skip" ? "⏭ skip" : sm.target_table}</td>
                <td style={{ padding:"4px 8px", textAlign:"right", color:"var(--text-dim)" }}>{sm.target_table!=="skip" ? (sheet?.row_count ?? 0).toLocaleString() : "—"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <div style={{ fontSize:"0.8rem", color:"var(--text-dim)", marginBottom:12 }}>
        Total: <strong style={{ color:"var(--text)" }}>{totalRows.toLocaleString()} rows</strong>
      </div>
      <div style={{ display:"flex", gap:8 }}>
        <button onClick={onEdit}
          style={{ padding:"8px 14px", background:"transparent", border:"1px solid var(--border)", borderRadius:7, cursor:"pointer", color:"var(--text-dim)", fontSize:"0.82rem" }}>
          ✏ Edit Mapping
        </button>
        <button onClick={onExecute}
          style={{ flex:1, padding:"10px 0", background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.9rem", cursor:"pointer" }}>
          🚀 Execute Migration
        </button>
      </div>
      <p style={{ margin:"8px 0 0", fontSize:"0.74rem", color:"var(--text-dim)", textAlign:"center" }}>
        Inserts new records only — existing ZANPOS data is not modified.
      </p>
    </div>
  );
}

function LiveProgress({ items }: { items: SheetProgress[] }) {
  return (
    <div style={{ display:"flex", flexDirection:"column", gap:10 }}>
      {items.map(p => {
        const pct = p.total > 0 ? Math.round((p.done / p.total) * 100) : (p.finished ? 100 : 0);
        return (
          <div key={p.sheet}>
            <div style={{ display:"flex", justifyContent:"space-between", marginBottom:4, fontSize:"0.82rem" }}>
              <span style={{ fontWeight:600 }}>{p.sheet}<span style={{ color:"var(--text-dim)", fontWeight:400 }}> → {p.target}</span></span>
              <span style={{ color:p.finished?"#22c55e":"var(--text-dim)" }}>
                {p.finished
                  ? `✓ ${p.inserted ?? 0} inserted${(p.skipped ?? 0) > 0 ? `, ${p.skipped} skipped` : ""}`
                  : `${p.done.toLocaleString()} / ${p.total.toLocaleString()}`}
              </span>
            </div>
            <div className="mig-progress-bar">
              <div className="mig-progress-fill" style={{ width:`${pct}%`, background:p.finished?"#22c55e":"var(--accent)" }} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function DoneCard({ items, onLaunch }: { items: SheetProgress[]; onLaunch: () => void }) {
  return (
    <div style={{ marginTop:12, display:"flex", flexDirection:"column", gap:10, alignItems:"flex-start" }}>
      {items.length > 0 && (
        <div style={{ padding:"12px 16px", background:"color-mix(in srgb,#22c55e 10%,transparent)", border:"1px solid color-mix(in srgb,#22c55e 30%,transparent)", borderRadius:10, fontSize:"0.84rem", minWidth:240 }}>
          {items.map(p => (
            <div key={p.sheet} style={{ display:"flex", justifyContent:"space-between", gap:20, padding:"2px 0" }}>
              <span style={{ color:"var(--text-dim)" }}>{p.target}</span>
              <span style={{ fontWeight:700, color:"#22c55e" }}>{(p.inserted ?? 0).toLocaleString()} rows</span>
            </div>
          ))}
        </div>
      )}
      <button onClick={onLaunch}
        style={{ padding:"11px 28px", background:"var(--accent)", color:"#fff", border:"none", borderRadius:8, fontWeight:700, fontSize:"0.95rem", cursor:"pointer" }}>
        Launch POS 🚀
      </button>
    </div>
  );
}

function RetryButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button onClick={onClick} style={{ marginTop:10, padding:"9px 18px", background:"transparent", border:"1.5px solid var(--accent)", borderRadius:8, color:"var(--accent)", fontWeight:700, fontSize:"0.875rem", cursor:"pointer" }}>
      {label}
    </button>
  );
}
