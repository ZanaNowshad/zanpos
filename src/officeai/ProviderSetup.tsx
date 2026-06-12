import { useState } from "react";
import type { SetupStep } from "./officeAiTypes";
import type { SessionUser, ProviderConfig } from "../types";
import { adminSetAnthropic, adminSetOpenai, adminSetGemini, adminValidateOpenai, adminValidateGemini } from "../tauri/commands";

interface Props {
  sessionUser: SessionUser;
  onDone: (cfg: ProviderConfig) => void;
  onBack: () => void;
}

export default function ProviderSetup({ sessionUser, onDone, onBack }: Props) {
  const [step, setStep] = useState<SetupStep>("pick_provider");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("");
  const [modelList, setModelList] = useState<string[]>([]);

  const handlePickAnthropic = () => setStep("anthropic_key");
  const handlePickOpenAI = () => setStep("openai_url_key");
  const handlePickGemini = () => setStep("gemini_key");

  const handleSaveAnthropic = async () => {
    if (!apiKey.trim()) { setError("API key required"); return; }
    setLoading(true); setError(null);
    try { await adminSetAnthropic(sessionUser.user_id, apiKey.trim()); onDone({ provider: "anthropic", api_key: "" }); }
    catch (e: unknown) { setError(String(e)); } finally { setLoading(false); }
  };

  const handleSaveOpenAI = async () => {
    if (!apiKey.trim()) { setError("API key required"); return; }
    setLoading(true); setError(null);
    try { await adminSetOpenai(sessionUser.user_id, baseUrl.trim(), apiKey.trim(), model); onDone({ provider: "openai", api_key: "", openai_base_url: baseUrl }); }
    catch (e: unknown) { setError(String(e)); } finally { setLoading(false); }
  };

  const handleSaveGemini = async () => {
    if (!apiKey.trim()) { setError("API key required"); return; }
    setLoading(true); setError(null);
    try { await adminSetGemini(sessionUser.user_id, apiKey.trim(), model || "gemini-2.0-flash"); onDone({ provider: "gemini", api_key: "" }); }
    catch (e: unknown) { setError(String(e)); } finally { setLoading(false); }
  };

  const handleValidateOpenAI = async () => {
    if (!apiKey.trim()) { setError("API key required"); return; }
    setLoading(true); setError(null);
    try { const r = await adminValidateOpenai(sessionUser.user_id, baseUrl.trim(), apiKey.trim()); setModelList(r.models ?? []); setStep("openai_pick_model"); }
    catch (e: unknown) { setError(String(e)); } finally { setLoading(false); }
  };

  const handleValidateGemini = async () => {
    if (!apiKey.trim()) { setError("API key required"); return; }
    setLoading(true); setError(null);
    try { const r = await adminValidateGemini(sessionUser.user_id, apiKey.trim()); setModelList(r.models ?? []); setStep("gemini_pick_model"); }
    catch (e: unknown) { setError(String(e)); } finally { setLoading(false); }
  };

  if (step === "pick_provider") return (
    <div className="setup-center">
      <div className="setup-card">
        <h2>Choose AI Provider</h2>
        <button className="btn-primary" onClick={handlePickAnthropic}>Claude (Anthropic)</button>
        <button className="btn-secondary" onClick={handlePickOpenAI}>OpenAI</button>
        <button className="btn-secondary" onClick={handlePickGemini}>Gemini</button>
        <button className="btn-secondary" onClick={onBack}>← Back</button>
        {error && <p className="setup-error">{error}</p>}
      </div>
    </div>
  );

  return (
    <div className="setup-center">
      <div className="setup-card">
        <h2>{step === "anthropic_key" ? "Anthropic API Key" : step.includes("openai") ? "OpenAI Setup" : "Gemini Setup"}</h2>
        {step === "openai_url_key" && <input className="field-input" placeholder="Base URL" value={baseUrl} onChange={e => setBaseUrl(e.target.value)} />}
        <input className="field-input" type="password" placeholder="API Key" value={apiKey} onChange={e => setApiKey(e.target.value)} />
        {step === "openai_url_key" && <button className="btn-primary" onClick={handleValidateOpenAI} disabled={loading}>Validate →</button>}
        {step === "gemini_key" && <button className="btn-primary" onClick={handleValidateGemini} disabled={loading}>Validate →</button>}
        {step === "anthropic_key" && <button className="btn-primary" onClick={handleSaveAnthropic} disabled={loading}>Save</button>}
        {(step === "openai_pick_model" || step === "gemini_pick_model") && (
          <>
            <select className="bo-select" value={model} onChange={e => setModel(e.target.value)}>
              {modelList.map(m => <option key={m} value={m}>{m}</option>)}
            </select>
            <button className="btn-primary" onClick={step === "openai_pick_model" ? handleSaveOpenAI : handleSaveGemini} disabled={loading}>Save</button>
          </>
        )}
        <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={loading}>← Back</button>
        {error && <p className="setup-error">{error}</p>}
      </div>
    </div>
  );
}
