import { useRef, useState } from "react";
import type { ProviderConfig, SessionUser } from "../../types";
import { authLoginPin, authLogout, adminGetProviderConfig } from "../../tauri/commands";
import { useChatController } from "../../officeai/useChatController";
import ChatPanel from "../../officeai/ChatPanel";

interface PanelProps {
  ownerUsername: string;
  onClose: () => void;
}

interface ChatProps {
  sessionUser: SessionUser;
  onClose: () => void;
}

/** Owns the chat controller once a real session exists — kept in its own
 * component so useChatController (a hook) is never called conditionally. */
function ProductsZanAiChat({ sessionUser, onClose }: ChatProps) {
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const [mutationCount, setMutationCount] = useState(0);
  const ctrl = useChatController({
    sessionUser,
    getUiContext: () => "Onboarding — Load Products step. The owner is about to paste a list of products (name, price, category) to create in bulk.",
    onNavigate: () => { /* no tabs during onboarding */ },
    onMutationApplied: () => setMutationCount(n => n + 1),
  });

  return (
    <div className="setup-zanai-panel">
      <p className="setup-field-hint">
        Paste your product list below — for example one per line with a name and price.
        ZanAI will create them for you and tell you about anything it skipped.
        {mutationCount > 0 && ` (${mutationCount} change${mutationCount === 1 ? "" : "s"} applied so far.)`}
      </p>
      <div className="setup-zanai-chat">
        <ChatPanel ctrl={ctrl} variant="docked" userName={sessionUser.display_name} businessName="" composerRef={composerRef} />
      </div>
      <div className="setup-actions">
        <button className="setup-btn-secondary" onClick={onClose}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
      </div>
    </div>
  );
}

/** Step 4 sub-panel — "paste a list". Reuses the real ZanAI chat + bulk
 * product-create tool path (the same one the catalog import flow uses). It
 * needs an authenticated session (AI endpoints are session-token gated,
 * unlike the plain actor-id commands used elsewhere in this wizard), so this
 * performs a short-lived login with the PIN just created in step 2 and logs
 * back out when the user leaves this panel. */
export default function ProductsZanAiPanel({ ownerUsername, onClose }: PanelProps) {
  const [pin, setPin] = useState("");
  const [sessionUser, setSessionUser] = useState<SessionUser | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [provider, setProvider] = useState<ProviderConfig | null>(null);
  const [providerChecked, setProviderChecked] = useState(false);

  const handleClose = () => {
    if (sessionUser) authLogout(sessionUser.session_token).catch(() => {});
    onClose();
  };

  const handleLogin = async () => {
    setLoggingIn(true);
    setLoginError(null);
    try {
      const user = await authLoginPin(ownerUsername, pin);
      setSessionUser(user);
      try {
        setProvider(await adminGetProviderConfig(user.session_token));
      } catch {
        setProvider(null);
      } finally {
        setProviderChecked(true);
      }
    } catch (e: unknown) {
      setLoginError(typeof e === "string" ? e : "Incorrect PIN");
    } finally {
      setLoggingIn(false);
    }
  };

  if (sessionUser && providerChecked && provider?.provider) {
    return <ProductsZanAiChat sessionUser={sessionUser} onClose={handleClose} />;
  }

  if (sessionUser && providerChecked) {
    return (
      <div className="setup-zanai-panel">
        <p className="setup-body">
          ZanAI needs an AI provider configured before it can create products from text.
          You can set this up later in Settings → AI, or use CSV import / start empty for now.
        </p>
        <div className="setup-actions">
          <button className="setup-btn-secondary" onClick={handleClose}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
        </div>
      </div>
    );
  }

  return (
    <div className="setup-zanai-panel">
      <p className="setup-field-hint">Confirm the owner PIN you just created to use ZanAI here.</p>
      <input
        className="field-input setup-pin-input"
        type="password" inputMode="numeric" pattern="[0-9]*" maxLength={6}
        placeholder="Owner PIN"
        value={pin}
        onChange={e => { setPin(e.target.value.replace(/\D/g, "")); setLoginError(null); }}
      />
      {loginError && <div className="modal-error">{loginError}</div>}
      <div className="setup-actions">
        <button className="setup-btn-secondary" onClick={onClose} disabled={loggingIn}><span className="icon-directional" aria-hidden="true">←</span> Back</button>
        <button className="setup-btn-primary" onClick={handleLogin} disabled={loggingIn || pin.length < 4}>
          {loggingIn ? "Checking…" : "Continue"}
        </button>
      </div>
    </div>
  );
}
