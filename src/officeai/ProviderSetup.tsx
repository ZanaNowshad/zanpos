import type { ProviderConfig } from "../types";
import { useLanguage } from "../hooks/useLanguage";
import { officeAiTranslator } from "../i18n/officeAiStrings";
import type { SetupStep } from "./officeAiTypes";
import { useState } from "react";
import { adminSetAnthropic, adminSetOpenai, adminSetGemini, adminValidateOpenai, adminValidateGemini } from "../tauri/commands";

interface Props { sessionToken: string; onDone: (cfg: ProviderConfig) => void; onBack: () => void; }

function PickProvider({ onPick, onBack }: { onPick: (s: SetupStep) => void; onBack: () => void }) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <div className="setup-center">
      <div className="setup-card">
        <h2>{t("chooseAiProvider")}</h2>
        <button className="btn-primary" onClick={() => onPick("anthropic_key")}>Claude (Anthropic)</button>
        <button className="btn-secondary" onClick={() => onPick("openai_url_key")}>OpenAI</button>
        <button className="btn-secondary" onClick={() => onPick("gemini_key")}>Gemini</button>
        <button className="btn-secondary" onClick={onBack}><span className="icon-directional" aria-hidden="true">←</span> {t("back")}</button>
      </div>
    </div>
  );
}

const OPENAI_BASE_URLS = [
  "https://api.openai.com/v1",
  "https://api.groq.com/openai/v1",
  "https://openrouter.ai/api/v1",
  "https://api.deepseek.com/v1",
  "http://localhost:11434/v1",
];

function KeyEntry({
  title, showBaseUrl, apiKey, baseUrl,
  onApiKey, onBaseUrl, onValidate, onBack, loading, error,
}: {
  title: string;
  showBaseUrl?: boolean;
  apiKey: string;
  baseUrl?: string;
  onApiKey: (v: string) => void;
  onBaseUrl?: (v: string) => void;
  onValidate: () => void;
  onBack: () => void;
  loading: boolean;
  error: string | null;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <div className="setup-center">
      <div className="setup-card">
        <h2>{title}</h2>
        {showBaseUrl && onBaseUrl && (
          <>
            <input className="field-input" placeholder={t("baseUrl")} value={baseUrl} onChange={e => onBaseUrl(e.target.value)} />
            <div className="setup-url-examples">
              {OPENAI_BASE_URLS.map(u => (
                <button key={u} className="setup-url-chip" onClick={() => onBaseUrl(u)}>
                  {u.replace(/https?:\/\//, "").split("/")[0]}
                </button>
              ))}
            </div>
          </>
        )}
        <input className="field-input" type="password" placeholder={t("apiKey")} value={apiKey} onChange={e => onApiKey(e.target.value)} />
        <button className="btn-primary" onClick={onValidate} disabled={loading}>{t("validate")} <span className="icon-directional" aria-hidden="true">→</span></button>
        <button className="btn-secondary" onClick={onBack} disabled={loading}><span className="icon-directional" aria-hidden="true">←</span> {t("back")}</button>
        {error && <p className="setup-error">{error}</p>}
      </div>
    </div>
  );
}

function PickModel({
  models, model, onSelect, onSave, onBack, loading, error,
}: {
  models: string[];
  model: string;
  onSelect: (v: string) => void;
  onSave: () => void;
  onBack: () => void;
  loading: boolean;
  error: string | null;
}) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  return (
    <div className="setup-center">
      <div className="setup-card">
        <h2>{t("chooseModel")}</h2>
        <select className="bo-select" value={model} onChange={e => onSelect(e.target.value)}>
          {models.map(m => <option key={m}>{m}</option>)}
        </select>
        <button className="btn-primary" onClick={onSave} disabled={loading}>{t("save")}</button>
        <button className="btn-secondary" onClick={onBack} disabled={loading}><span className="icon-directional" aria-hidden="true">←</span> {t("back")}</button>
        {error && <p className="setup-error">{error}</p>}
      </div>
    </div>
  );
}

export default function ProviderSetup({ sessionToken, onDone, onBack }: Props) {
  const { language } = useLanguage();
  const t = officeAiTranslator(language);
  const [step, setStep] = useState<SetupStep>("pick_provider");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("");
  const [models, setModels] = useState<string[]>([]);

  if (step === "pick_provider") {
    return <PickProvider onPick={setStep} onBack={onBack} />;
  }

  if (step === "anthropic_key") {
    return (
      <KeyEntry
        title={`Anthropic — ${t("apiKey")}`}
        apiKey={apiKey}
        onApiKey={setApiKey}
        onValidate={async () => {
          setLoading(true); setError(null);
          try {
            await adminSetAnthropic(sessionToken, apiKey.trim());
            onDone({ provider: "anthropic", anthropic_key_set: true, anthropic_model: "", openai_base_url: "", openai_key_set: false, openai_model: "", gemini_key_set: false, gemini_model: "" });
          } catch (e: unknown) { setError(String(e)); }
          setLoading(false);
        }}
        onBack={() => { setStep("pick_provider"); setError(null); }}
        loading={loading}
        error={error}
      />
    );
  }

  if (step === "openai_url_key") {
    return (
      <KeyEntry
        title={`OpenAI — ${t("setup")}`}
        showBaseUrl
        apiKey={apiKey}
        baseUrl={baseUrl}
        onApiKey={setApiKey}
        onBaseUrl={setBaseUrl}
        onValidate={async () => {
          setLoading(true); setError(null);
          try {
            const r: { models?: { id: string }[] } = await adminValidateOpenai(sessionToken, baseUrl.trim(), apiKey.trim());
            setModels(r.models?.map((m: { id: string }) => m.id) ?? []);
            setStep("openai_pick_model");
          } catch (e: unknown) { setError(String(e)); }
          setLoading(false);
        }}
        onBack={() => { setStep("pick_provider"); setError(null); }}
        loading={loading}
        error={error}
      />
    );
  }

  if (step === "gemini_key") {
    return (
      <KeyEntry
        title={`Gemini — ${t("apiKey")}`}
        apiKey={apiKey}
        onApiKey={setApiKey}
        onValidate={async () => {
          setLoading(true); setError(null);
          try {
            const r: { models?: { id: string }[] } = await adminValidateGemini(sessionToken, apiKey.trim());
            setModels(r.models?.map((m: { id: string }) => m.id) ?? []);
            setStep("gemini_pick_model");
          } catch (e: unknown) { setError(String(e)); }
          setLoading(false);
        }}
        onBack={() => { setStep("pick_provider"); setError(null); }}
        loading={loading}
        error={error}
      />
    );
  }

  if (step === "openai_pick_model") {
    return (
      <PickModel
        models={models}
        model={model}
        onSelect={setModel}
        onSave={async () => {
          setLoading(true); setError(null);
          try {
            await adminSetOpenai(sessionToken, baseUrl.trim(), apiKey.trim(), model);
            onDone({ provider: "openai", anthropic_key_set: false, anthropic_model: "", openai_base_url: baseUrl, openai_key_set: true, openai_model: model, gemini_key_set: false, gemini_model: "" });
          } catch (e: unknown) { setError(String(e)); }
          setLoading(false);
        }}
        onBack={() => setStep("openai_url_key")}
        loading={loading}
        error={error}
      />
    );
  }

  if (step === "gemini_pick_model") {
    return (
      <PickModel
        models={models}
        model={model}
        onSelect={setModel}
        onSave={async () => {
          setLoading(true); setError(null);
          try {
            await adminSetGemini(sessionToken, apiKey.trim(), model || "gemini-2.0-flash");
            onDone({ provider: "gemini", anthropic_key_set: false, anthropic_model: "", openai_base_url: "", openai_key_set: false, openai_model: "", gemini_key_set: true, gemini_model: model });
          } catch (e: unknown) { setError(String(e)); }
          setLoading(false);
        }}
        onBack={() => setStep("gemini_key")}
        loading={loading}
        error={error}
      />
    );
  }

  return null;
}
