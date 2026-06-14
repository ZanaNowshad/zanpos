import type { ProviderConfig } from "../types";
import type { SetupStep } from "./officeAiTypes";
import { useState } from "react";
import { adminSetAnthropic, adminSetOpenai, adminSetGemini, adminValidateOpenai, adminValidateGemini } from "../tauri/commands";

interface Props { actorUserId: string; onDone: (cfg: ProviderConfig) => void; onBack: () => void; }

export default function ProviderSetup({ actorUserId, onDone, onBack }: Props) {
  const [step, setStep] = useState<SetupStep>("pick_provider");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("");
  const [models, setModels] = useState<string[]>([]);

  const handleSaveAnthropic = async () => { setLoading(true); setError(null); try { await adminSetAnthropic(actorUserId, apiKey.trim()); onDone({ provider: "anthropic", anthropic_key_set: true, openai_base_url: "", openai_key_set: false, openai_model: "", gemini_key_set: false, gemini_model: "" }); } catch (e: unknown) { setError(String(e)); } finally { setLoading(false); } };
  const handleSaveOpenAI = async () => { setLoading(true); setError(null); try { await adminSetOpenai(actorUserId, baseUrl.trim(), apiKey.trim(), model); onDone({ provider: "openai", anthropic_key_set: false, openai_base_url: baseUrl, openai_key_set: true, openai_model: model, gemini_key_set: false, gemini_model: "" }); } catch (e: unknown) { setError(String(e)); } finally { setLoading(false); } };
  const handleSaveGemini = async () => { setLoading(true); setError(null); try { await adminSetGemini(actorUserId, apiKey.trim(), model || "gemini-2.0-flash"); onDone({ provider: "gemini", anthropic_key_set: false, openai_base_url: "", openai_key_set: false, openai_model: "", gemini_key_set: true, gemini_model: model }); } catch (e: unknown) { setError(String(e)); } finally { setLoading(false); } };
  const handleValidateOpenAI = async () => { setLoading(true); setError(null); try { const r: { models?: { id: string }[] } = await adminValidateOpenai(actorUserId, baseUrl.trim(), apiKey.trim()); setModels(r.models?.map((m: { id: string }) => m.id) ?? []); setStep("openai_pick_model"); } catch (e: unknown) { setError(String(e)); } finally { setLoading(false); } };
  const handleValidateGemini = async () => { setLoading(true); setError(null); try { const r: { models?: { id: string }[] } = await adminValidateGemini(actorUserId, apiKey.trim()); setModels(r.models?.map((m: { id: string }) => m.id) ?? []); setStep("gemini_pick_model"); } catch (e: unknown) { setError(String(e)); } finally { setLoading(false); } };

  if (step === "pick_provider") return <div className="setup-center"><div className="setup-card"><h2>Choose AI Provider</h2><button className="btn-primary" onClick={() => setStep("anthropic_key")}>Claude (Anthropic)</button><button className="btn-secondary" onClick={() => setStep("openai_url_key")}>OpenAI</button><button className="btn-secondary" onClick={() => setStep("gemini_key")}>Gemini</button><button className="btn-secondary" onClick={onBack}>← Back</button>{error && <p className="setup-error">{error}</p>}</div></div>;

  return <div className="setup-center"><div className="setup-card"><h2>{step === "anthropic_key" ? "Anthropic API Key" : "Setup"}</h2>
    {step === "openai_url_key" && <input className="field-input" placeholder="Base URL" value={baseUrl} onChange={e => setBaseUrl(e.target.value)} />}
    {step === "openai_url_key" && <div className="setup-url-examples">
      {["https://api.openai.com/v1", "https://api.groq.com/openai/v1", "https://openrouter.ai/api/v1", "https://api.deepseek.com/v1", "http://localhost:11434/v1"].map(u => (
        <button key={u} className="setup-url-chip" onClick={() => setBaseUrl(u)}>
          {u.replace(/https?:\/\//, "").split("/")[0]}
        </button>
      ))}
    </div>}
    <input className="field-input" type="password" placeholder="API Key" value={apiKey} onChange={e => setApiKey(e.target.value)} />
    {step === "openai_url_key" && <button className="btn-primary" onClick={handleValidateOpenAI} disabled={loading}>Validate →</button>}
    {step === "gemini_key" && <button className="btn-primary" onClick={handleValidateGemini} disabled={loading}>Validate →</button>}
    {step === "anthropic_key" && <button className="btn-primary" onClick={handleSaveAnthropic} disabled={loading}>Save</button>}
    {(step === "openai_pick_model" || step === "gemini_pick_model") && <><select className="bo-select" value={model} onChange={e => setModel(e.target.value)}>{models.map(m => <option key={m}>{m}</option>)}</select>
      <button className="btn-primary" onClick={step === "openai_pick_model" ? handleSaveOpenAI : handleSaveGemini} disabled={loading}>Save</button></>}
    <button className="btn-secondary" onClick={() => setStep("pick_provider")} disabled={loading}>← Back</button>
    {error && <p className="setup-error">{error}</p>}
  </div></div>;
}
