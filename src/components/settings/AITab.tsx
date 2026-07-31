import { useCallback, useEffect, useState } from "react";
import type { AiConfigPayload, FeatureToggles, ProviderConfig, SystemHealthReport, ValidateProviderResult } from "../../types";
import {
  adminGetProviderConfig,
  adminSetAnthropic,
  adminSetOpenai,
  adminSetGemini,
  adminValidateOpenai,
  adminValidateGemini,
  adminValidateAnthropic,
  adminDeleteProvider,
  adminSetAnthropicModel,
  adminGetAiConfig,
  adminSaveAiConfig,
  adminGetFeatureToggles,
  adminSaveFeatureToggles,
  systemHealthApplyFix,
  systemHealthCheck,
} from "../../tauri/commands";
import AppConfirmModal from "../AppConfirmModal";

interface Props { sessionUserId: string; sessionToken: string; }

type AiSetupStep = "summary" | "pick_provider" | "anthropic_key" | "anthropic_pick_model"
  | "openai_url_key" | "gemini_key" | "openai_pick_model" | "gemini_pick_model";

export default function AITab({ sessionUserId, sessionToken }: Props) {
  const [cfg, setCfg] = useState<ProviderConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [step, setStep] = useState<AiSetupStep>("summary");
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [aiConfig, setAiConfig] = useState<AiConfigPayload | null>(null);
  const [configSaving, setConfigSaving] = useState(false);
  const [configSaved, setConfigSaved] = useState(false);
  const [toggles, setToggles] = useState<FeatureToggles | null>(null);
  const [togglesSaving, setTogglesSaving] = useState(false);
  const [healthReport, setHealthReport] = useState<SystemHealthReport | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);
  const [healthFixing, setHealthFixing] = useState<string | null>(null);
  const [healthMessage, setHealthMessage] = useState<string | null>(null);
  const [healthFixConfirm, setHealthFixConfirm] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const c = await adminGetProviderConfig(sessionToken);
      setCfg(c);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionToken]);

  useEffect(() => { load(); }, [load]);

  const loadAiConfig = useCallback(async () => {
    try {
      const c = await adminGetAiConfig(sessionToken);
      setAiConfig(c);
    } catch { /* non-critical; defaults apply */ }
  }, [sessionToken]);

  useEffect(() => { loadAiConfig(); }, [loadAiConfig]);

  const handleSaveAiConfig = async () => {
    if (!aiConfig) return;
    setConfigSaving(true); setError(null); setConfigSaved(false);
    try {
      await adminSaveAiConfig(sessionToken, aiConfig);
      setConfigSaved(true);
      setTimeout(() => setConfigSaved(false), 2000);
    } catch (e: unknown) { setError(String(e)); }
    finally { setConfigSaving(false); }
  };

  const updateAiConfig = (patch: Partial<AiConfigPayload>) => {
    setAiConfig(prev => prev ? { ...prev, ...patch } : null);
  };

  const loadToggles = useCallback(async () => {
    try {
      const t = await adminGetFeatureToggles(sessionToken);
      setToggles(t);
    } catch { /* non-critical */ }
  }, [sessionToken]);

  useEffect(() => { loadToggles(); }, [loadToggles]);

  const handleSaveToggles = async () => {
    if (!toggles) return;
    setTogglesSaving(true); setError(null);
    try {
      await adminSaveFeatureToggles(sessionToken, toggles);
    } catch (e: unknown) { setError(String(e)); }
    finally { setTogglesSaving(false); }
  };

  const toggleOne = (key: keyof FeatureToggles) => {
    setToggles(prev => prev ? { ...prev, [key]: !prev[key] } : null);
  };

  const handleSaveAnthropic = async () => {
    setSaving(true); setError(null);
    try {
      await adminSetAnthropic(sessionToken, apiKey.trim());
      if (model) await adminSetAnthropicModel(sessionToken, model);
      await load();
      setStep("summary");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleSaveOpenAI = async () => {
    setSaving(true); setError(null);
    try {
      await adminSetOpenai(sessionToken, baseUrl.trim(), apiKey.trim(), model);
      await load();
      setStep("summary");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleSaveGemini = async () => {
    setSaving(true); setError(null);
    try {
      await adminSetGemini(sessionToken, apiKey.trim(), model || "gemini-2.0-flash");
      await load();
      setStep("summary");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleValidateOpenAI = async () => {
    setSaving(true); setError(null);
    try {
      const r: ValidateProviderResult = await adminValidateOpenai(sessionToken, baseUrl.trim(), apiKey.trim());
      if (!r.success) { setError(r.error ?? "Validation failed"); return; }
      setModels(r.models?.map((m: { id: string }) => m.id) ?? []);
      setStep("openai_pick_model");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleValidateGemini = async () => {
    setSaving(true); setError(null);
    try {
      const r: ValidateProviderResult = await adminValidateGemini(sessionToken, apiKey.trim());
      if (!r.success) { setError(r.error ?? "Validation failed"); return; }
      setModels(r.models?.map((m: { id: string }) => m.id) ?? []);
      setStep("gemini_pick_model");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleValidateAnthropic = async () => {
    setSaving(true); setError(null);
    try {
      const r: ValidateProviderResult = await adminValidateAnthropic(sessionToken, apiKey.trim());
      if (!r.success) { setError(r.error ?? "Validation failed"); return; }
      setModels(r.models?.map((m: { id: string }) => m.id) ?? []);
      setModel(cfg?.anthropic_model ?? "");
      setStep("anthropic_pick_model");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleDelete = async () => {
    setSaving(true); setError(null);
    try {
      await adminDeleteProvider(sessionToken);
      await load();
      setDeleteConfirm(false);
      setStep("summary");
    } catch (e: unknown) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const handleRunHealthCheck = async () => {
    setHealthLoading(true);
    setHealthMessage(null);
    setError(null);
    try {
      const report = await systemHealthCheck(sessionUserId);
      setHealthReport(report);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setHealthLoading(false);
    }
  };

  const applyHealthFix = async (fixAction: string) => {
    setHealthFixing(fixAction);
    setHealthMessage(null);
    setError(null);
    try {
      const result = await systemHealthApplyFix(sessionUserId, fixAction);
      setHealthMessage(result.message);
      const report = await systemHealthCheck(sessionUserId);
      setHealthReport(report);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setHealthFixing(null);
    }
  };

  const handleApplyHealthFix = (fixAction: string) => {
    setHealthFixConfirm(fixAction);
  };

  const providerLabel = (p: string) => {
    switch (p) {
      case "anthropic": return "Claude (Anthropic)";
      case "openai": return "OpenAI";
      case "gemini": return "Gemini";
      default: return "Not configured";
    }
  };

  const providerBadge = (p: string) => {
    if (!p) return <span className="ai-provider-badge ai-provider-none">No provider</span>;
    return <span className={`ai-provider-badge ai-provider-${p}`}>{providerLabel(p)}</span>;
  };

  if (loading) return <div className="bo-empty">Loading AI settings…</div>;

  return (
    <div className="settings-page">
      {healthFixConfirm && (
        <AppConfirmModal
          title="Apply health fix"
          description={`Apply system health fix: ${healthFixConfirm}`}
          confirmLabel="Apply fix"
          onCancel={() => setHealthFixConfirm(null)}
          onConfirm={() => {
            const fix = healthFixConfirm;
            setHealthFixConfirm(null);
            void applyHealthFix(fix);
          }}
        />
      )}
      {/* ── Provider status section ────────────────────────────────────────── */}
      {step === "summary" && (
        <>
          <section>
            <h3 className="settings-page-title">AI Provider</h3>
            <p className="settings-hint">
              Configure the AI model that powers the AI Office chat, analytics, and automated tool execution.
            </p>
            <div className="ai-provider-card">
              <div className="ai-provider-row">
                <span className="ai-provider-label">Provider</span>
                {cfg && providerBadge(cfg.provider)}
              </div>
              {cfg?.provider === "anthropic" && (
                <>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Model</span>
                    <code className="ai-provider-value">{cfg.anthropic_model}</code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">API Key</span>
                    <code className="ai-provider-value">{cfg.anthropic_key_set ? "●●●●●●●● Configured" : "Not set"}</code>
                  </div>
                </>
              )}
              {cfg?.provider === "openai" && (
                <>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Base URL</span>
                    <code className="ai-provider-value">{cfg.openai_base_url}</code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Model</span>
                    <code className="ai-provider-value">{cfg.openai_model}</code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">API Key</span>
                    <code className="ai-provider-value">{cfg.openai_key_set ? "●●●●●●●● Configured" : "Not set"}</code>
                  </div>
                </>
              )}
              {cfg?.provider === "gemini" && (
                <>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Model</span>
                    <code className="ai-provider-value">{cfg.gemini_model || "gemini-2.0-flash"}</code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">API Key</span>
                    <code className="ai-provider-value">{cfg.gemini_key_set ? "●●●●●●●● Configured" : "Not set"}</code>
                  </div>
                </>
              )}
            </div>
            <div className="ai-provider-actions">
              <button className="btn-primary" onClick={() => setStep("pick_provider")}>
                {cfg?.provider ? "Change Provider" : "Configure Provider"}
              </button>
            </div>
          </section>

          {/* ── Danger zone ────────────────────────────────────────────────── */}
          {cfg?.provider && (
            <section className="ai-danger-section">
              <h3 className="settings-page-title">Danger Zone</h3>
              <p className="settings-hint">
                Removing the provider will disable AI Office chat and all AI-powered features.
              </p>
              {!deleteConfirm ? (
                <button className="btn-danger" onClick={() => setDeleteConfirm(true)}>
                  Remove Provider
                </button>
              ) : (
                <div className="ai-delete-confirm">
                  <p className="settings-hint" style={{ color: "var(--error)" }}>
                    This will delete all stored API keys and provider settings. This action cannot be undone.
                  </p>
                  <div className="ai-delete-actions">
                    <button className="btn-danger" onClick={handleDelete} disabled={saving}>
                      Yes, Delete Everything
                    </button>
                    <button className="btn-secondary" onClick={() => setDeleteConfirm(false)} disabled={saving}>
                      Cancel
                    </button>
                  </div>
                </div>
              )}
            </section>
          )}

          {/* ── Features section ─────────────────────────────────────────────── */}
          <section>
            <h3 className="settings-page-title">System Health Check</h3>
            <div className="ai-provider-card">
              <div className="ai-provider-row">
                <span className="ai-provider-label">Status</span>
                <code className="ai-provider-value">
                  {healthReport
                    ? healthReport.summary.ok ? "Healthy" : "Attention needed"
                    : "Not checked"}
                </code>
              </div>
              {healthReport && (
                <>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Database</span>
                    <code className="ai-provider-value">{healthReport.summary.db_integrity}</code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Hub / Devices</span>
                    <code className="ai-provider-value">
                      {healthReport.summary.hub_mode} / {healthReport.devices.length}
                    </code>
                  </div>
                  <div className="ai-provider-row">
                    <span className="ai-provider-label">Sync Queue</span>
                    <code className="ai-provider-value">
                      {healthReport.summary.pending_sync_rows} pending, {healthReport.summary.stuck_sync_rows} stuck
                    </code>
                  </div>
                </>
              )}
            </div>
            <div className="ai-provider-actions" style={{ marginTop: 12 }}>
              <button className="btn-primary" onClick={handleRunHealthCheck} disabled={healthLoading}>
                {healthLoading ? "Checking..." : "Run Complete Health Check"}
              </button>
            </div>
            {healthMessage && <p className="settings-hint">{healthMessage}</p>}
            {healthReport && (
              <div style={{ marginTop: 14 }}>
                {healthReport.findings.length === 0 ? (
                  <p className="settings-hint">No issues found.</p>
                ) : (
                  <div className="ai-toggles-grid">
                    {healthReport.findings.map((f) => (
                      <div key={`${f.code}-${f.title}`} className="ai-toggle-row" style={{ alignItems: "flex-start" }}>
                        <span style={{ flex: 1 }}>
                          <strong>{f.area}: {f.title}</strong><br />
                          <span className="settings-hint">{f.detail}</span>
                        </span>
                        {f.fix_action && (
                          <button
                            className="btn-secondary"
                            onClick={() => handleApplyHealthFix(f.fix_action!)}
                            disabled={healthFixing !== null}
                          >
                            {healthFixing === f.fix_action ? "Fixing..." : "Fix"}
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
                {healthReport.devices.length > 0 && (
                  <div style={{ marginTop: 12 }}>
                    <h4 className="settings-page-title">Devices</h4>
                    <div className="ai-toggles-grid">
                      {healthReport.devices.map((d) => (
                        <div key={`${d.device_id}-${d.ip ?? ""}`} className="ai-toggle-row">
                          <span>
                            <strong>{d.label}</strong><br />
                            <span className="settings-hint">
                              {d.role} · {d.status} · {d.ip ?? "no IP"} · {d.last_seen ?? "not seen"}
                            </span>
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </section>

          {toggles && (
            <section>
              <h3 className="settings-page-title">Features</h3>
              <p className="settings-hint">
                Enable or disable groups of AI tools. Read-only tools (list/get/search) are always available. Changes take effect on the next chat message.
              </p>
              <div className="ai-toggles-grid">
                {([
                  ["web_search", "Web Search — let AI search the internet"],
                  ["web_fetch", "Web Fetch — let AI read web pages"],
                  ["compare_prices", "Compare Prices — cross-store price lookup"],
                  ["market_price", "Market Price — Bahrain market price check"],
                  ["smart_analytics", "Smart Analytics — reports, charts, dashboards"],
                  ["proactive", "Proactive — AI-initiated suggestions (experimental)"],
                  ["inventory_ops", "Inventory Ops — bulk stock & price adjustments"],
                  ["customer_insights", "Customer Insights — customer analysis tools"],
                  ["insights_engine", "Insights Engine — analytics dashboard & proactive intelligence (experimental)"],
                ] as [keyof FeatureToggles, string][]).map(([key, label]) => (
                  <label key={key} className="ai-toggle-row">
                    <input type="checkbox" checked={toggles[key]} onChange={() => toggleOne(key)} />
                    <span>{label}</span>
                  </label>
                ))}
              </div>
              <div className="ai-provider-actions" style={{ marginTop: 16 }}>
                <button className="btn-primary" onClick={handleSaveToggles} disabled={togglesSaving}>
                  {togglesSaving ? "Saving…" : "Save Features"}
                </button>
              </div>
            </section>
          )}

          {/* ── Parameters section ─────────────────────────────────────────── */}
          {aiConfig && (
            <section>
              <h3 className="settings-page-title">Parameters</h3>
              <p className="settings-hint">
                Fine-tune AI behaviour. Changes take effect on the next chat message — no restart needed.
              </p>
              <div className="ai-params-grid">
                <label className="ai-param-row">
                  <span>Anthropic Max Tokens</span>
                  <input type="number" className="field-input" min="1" max="32000" value={aiConfig.anthropic_max_tokens}
                    onChange={e => updateAiConfig({ anthropic_max_tokens: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>OpenAI Max Tokens</span>
                  <input type="number" className="field-input" min="1" max="128000" value={aiConfig.openai_max_tokens}
                    onChange={e => updateAiConfig({ openai_max_tokens: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Temperature</span>
                  <input type="number" className="field-input" step="0.1" min="0" max="2" value={aiConfig.temperature}
                    onChange={e => updateAiConfig({ temperature: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Max Tool Turns</span>
                  <input type="number" className="field-input" min="1" max="50" value={aiConfig.max_turns}
                    onChange={e => updateAiConfig({ max_turns: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Context Window (chars)</span>
                  <input type="number" className="field-input" min="1000" max="1000000" value={aiConfig.context_window_chars}
                    onChange={e => updateAiConfig({ context_window_chars: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Connect Timeout (secs)</span>
                  <input type="number" className="field-input" min="1" max="60" value={aiConfig.connect_timeout_secs}
                    onChange={e => updateAiConfig({ connect_timeout_secs: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Stream Timeout (secs)</span>
                  <input type="number" className="field-input" min="300" max="1800" value={aiConfig.stream_timeout_secs}
                    onChange={e => updateAiConfig({ stream_timeout_secs: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Action Expiry (minutes)</span>
                  <input type="number" className="field-input" min="1" max="1440" value={aiConfig.action_expiry_minutes}
                    onChange={e => updateAiConfig({ action_expiry_minutes: Number(e.target.value) })} />
                </label>
                <label className="ai-param-row">
                  <span>Bulk Batch Size</span>
                  <input type="number" className="field-input" min="1" max="500" value={aiConfig.bulk_batch_size}
                    onChange={e => updateAiConfig({ bulk_batch_size: Number(e.target.value) })} />
                </label>
              </div>
              <div className="ai-provider-actions" style={{ marginTop: 16 }}>
                <button className="btn-primary" onClick={handleSaveAiConfig} disabled={configSaving}>
                  {configSaving ? "Saving…" : configSaved ? "Saved!" : "Save Parameters"}
                </button>
              </div>
            </section>
          )}
        </>
      )}

      {/* ── Provider picker ────────────────────────────────────────────────── */}
      {step === "pick_provider" && (
        <section>
          <h3 className="settings-page-title">Choose AI Provider</h3>
          <div className="ai-provider-choices">
            <button className="btn-primary" onClick={() => { setApiKey(""); setModel(""); setError(null); setStep("anthropic_key"); }}>
              Claude (Anthropic)
            </button>
            <button className="btn-secondary" onClick={() => { setApiKey(""); setBaseUrl("https://api.openai.com/v1"); setModel(""); setError(null); setStep("openai_url_key"); }}>
              OpenAI
            </button>
            <button className="btn-secondary" onClick={() => { setApiKey(""); setModel(""); setError(null); setStep("gemini_key"); }}>
              Gemini
            </button>
          </div>
          <button className="btn-secondary" style={{ marginTop: 12 }} onClick={() => setStep("summary")}>
            <span className="icon-directional" aria-hidden="true">←</span> Back
          </button>
        </section>
      )}

      {/* ── Anthropic key ──────────────────────────────────────────────────── */}
      {step === "anthropic_key" && (
        <section>
          <h3 className="settings-page-title">Anthropic API Key</h3>
          <input className="field-input" type="password" placeholder="sk-ant-api03-…" value={apiKey}
            onChange={e => setApiKey(e.target.value)} />
          <div className="ai-provider-choices" style={{ marginTop: 12 }}>
            <button className="btn-primary" onClick={handleValidateAnthropic} disabled={saving || !apiKey.trim()}>
              Validate & Continue
            </button>
            <button className="btn-secondary" onClick={handleSaveAnthropic} disabled={saving || !apiKey.trim()}>
              Skip Validation — Save
            </button>
            <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={saving}>
              <span className="icon-directional" aria-hidden="true">←</span> Back
            </button>
          </div>
        </section>
      )}

      {/* ── OpenAI URL + key ───────────────────────────────────────────────── */}
      {step === "openai_url_key" && (
        <section>
          <h3 className="settings-page-title">OpenAI Configuration</h3>
          <input className="field-input" placeholder="Base URL" value={baseUrl}
            onChange={e => setBaseUrl(e.target.value)} />
          <div className="setup-url-examples" style={{ marginTop: 8, marginBottom: 8 }}>
            {["https://api.openai.com/v1", "https://api.groq.com/openai/v1", "https://openrouter.ai/api/v1", "https://api.deepseek.com/v1", "http://localhost:11434/v1"].map(u => (
              <button key={u} className="setup-url-chip" onClick={() => setBaseUrl(u)}>
                {u.replace(/https?:\/\//, "").split("/")[0]}
              </button>
            ))}
          </div>
          <input className="field-input" type="password" placeholder="API Key" value={apiKey}
            onChange={e => setApiKey(e.target.value)} />
          <div className="ai-provider-choices" style={{ marginTop: 12 }}>
            <button className="btn-primary" onClick={handleValidateOpenAI} disabled={saving || !apiKey.trim()}>
              Validate <span className="icon-directional" aria-hidden="true">→</span>
            </button>
            <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={saving}>
              <span className="icon-directional" aria-hidden="true">←</span> Back
            </button>
          </div>
        </section>
      )}

      {/* ── Gemini key ─────────────────────────────────────────────────────── */}
      {step === "gemini_key" && (
        <section>
          <h3 className="settings-page-title">Gemini API Key</h3>
          <input className="field-input" type="password" placeholder="AIza…" value={apiKey}
            onChange={e => setApiKey(e.target.value)} />
          <div className="ai-provider-choices" style={{ marginTop: 12 }}>
            <button className="btn-primary" onClick={handleValidateGemini} disabled={saving || !apiKey.trim()}>
              Validate <span className="icon-directional" aria-hidden="true">→</span>
            </button>
            <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={saving}>
              <span className="icon-directional" aria-hidden="true">←</span> Back
            </button>
          </div>
        </section>
      )}

      {/* ── Model picker (shared: Anthropic + OpenAI + Gemini) ───────────────── */}
      {(step === "anthropic_pick_model" || step === "openai_pick_model" || step === "gemini_pick_model") && (
        <section>
          <h3 className="settings-page-title">Select Model</h3>
          <select className="bo-select" value={model} onChange={e => setModel(e.target.value)}>
            <option value="">-- Choose a model --</option>
            {models.map(m => <option key={m}>{m}</option>)}
          </select>
          <div className="ai-provider-choices" style={{ marginTop: 12 }}>
            <button className="btn-primary"
              onClick={
                step === "anthropic_pick_model" ? handleSaveAnthropic :
                step === "openai_pick_model" ? handleSaveOpenAI : handleSaveGemini
              }
              disabled={saving || !model}>
              Save
            </button>
            <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={saving}>
              <span className="icon-directional" aria-hidden="true">←</span> Back
            </button>
          </div>
        </section>
      )}

      {error && <p className="setup-error" style={{ marginTop: 16 }}>{error}</p>}
    </div>
  );
}
