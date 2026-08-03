import { useEffect, useState, type FormEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  Cloud,
  ExternalLink,
  KeyRound,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Store,
  Unplug,
} from "lucide-react";
import type { CloudflareConnection } from "../../storefront/types";

export const CLOUDFLARE_TOKEN_TEMPLATE_URL =
  "https://dash.cloudflare.com/profile/api-tokens?permissionGroupKeys=%5B%7B%22key%22%3A%22workers_scripts%22%2C%22type%22%3A%22edit%22%7D%2C%7B%22key%22%3A%22workers_r2%22%2C%22type%22%3A%22edit%22%7D%2C%7B%22key%22%3A%22account_settings%22%2C%22type%22%3A%22read%22%7D%5D&accountId=%2A&zoneId=all&name=ZANPOS%20Storefront";

interface Props {
  connection: CloudflareConnection;
  busy: boolean;
  onConnect: (token: string) => Promise<void> | void;
  onSelectAccount: (accountId: string) => Promise<void> | void;
  onDisconnect: () => Promise<void> | void;
}

export default function CloudflareConnectionPanel({
  connection,
  busy,
  onConnect,
  onSelectAccount,
  onDisconnect,
}: Props) {
  const [token, setToken] = useState("");
  const [accountId, setAccountId] = useState(connection.account_id ?? "");
  const isConnected = connection.state === "connected";

  useEffect(() => {
    setAccountId(connection.account_id ?? connection.accounts[0]?.id ?? "");
  }, [connection.account_id, connection.accounts]);

  useEffect(() => () => setToken(""), []);

  async function connect(event: FormEvent) {
    event.preventDefault();
    const submitted = token.trim();
    if (!submitted) return;
    setToken("");
    await onConnect(submitted);
  }

  return (
    <section className={`cf-connect ${isConnected ? "is-connected" : ""}`} aria-labelledby="cf-connect-title">
      <div className="cf-route" aria-label="Storefront delivery route">
        <span className="cf-route-node is-ready"><Store aria-hidden="true" /><strong>ZANPOS shelf</strong><small>Catalogue source</small></span>
        <span className="cf-route-line" aria-hidden="true"><i /></span>
        <span className={`cf-route-node ${connection.credential_stored ? "is-ready" : "is-current"}`}><Cloud aria-hidden="true" /><strong>Cloudflare edge</strong><small>{isConnected ? "Account connected" : "Secure delivery"}</small></span>
        <span className="cf-route-line" aria-hidden="true"><i /></span>
        <span className={`cf-route-node ${isConnected ? "is-ready" : ""}`}><Check aria-hidden="true" /><strong>Customer link</strong><small>{isConnected ? "Ready for setup" : "Your public address"}</small></span>
      </div>

      {isConnected ? (
        <div className="cf-receipt">
          <span className="cf-provider-mark"><ShieldCheck aria-hidden="true" /></span>
          <div>
            <span className="cf-kicker">Connected securely</span>
            <h4 id="cf-connect-title">{connection.account_name}</h4>
            <p>The connection key is protected by your operating system and is never shown in ZANPOS.</p>
          </div>
          <button className="cf-quiet-button" type="button" onClick={() => void onDisconnect()} disabled={busy}>
            <Unplug aria-hidden="true" /> Disconnect
          </button>
        </div>
      ) : connection.state === "account_required" ? (
        <div className="cf-account-choice">
          <div>
            <span className="cf-kicker">Choose the shop account</span>
            <h4 id="cf-connect-title">Which Cloudflare account should host this storefront?</h4>
          </div>
          <select aria-label="Cloudflare account" value={accountId} onChange={event => setAccountId(event.target.value)}>
            {connection.accounts.map(account => <option key={account.id} value={account.id}>{account.name}</option>)}
          </select>
          <button className="btn-primary" type="button" disabled={!accountId || busy} onClick={() => void onSelectAccount(accountId)}>
            {busy ? <LoaderCircle aria-hidden="true" /> : <Check aria-hidden="true" />} Use this account
          </button>
        </div>
      ) : (
        <div className="cf-onboarding">
          <div className="cf-onboarding-copy">
            <span className="cf-kicker">{connection.state === "degraded" ? "Reconnect required" : "Built-in Cloudflare connection"}</span>
            <h4 id="cf-connect-title">{connection.state === "degraded" ? "Refresh the secure connection." : "Put your storefront on Cloudflare."}</h4>
            <p>Create a limited connection key, then paste it once. ZANPOS verifies it directly with Cloudflare and stores it in your operating system credential vault.</p>
            {connection.issue && <p className="cf-issue" role="alert">{connection.issue}</p>}
          </div>
          <ol className="cf-steps">
            <li><span>1</span><div><strong>Create the key</strong><small>Cloudflare opens with the required permissions prepared.</small></div></li>
            <li><span>2</span><div><strong>Paste it once</strong><small>ZANPOS verifies your account before saving anything.</small></div></li>
          </ol>
          <form className="cf-token-form" onSubmit={connect}>
            <button className="cf-template-button" type="button" onClick={() => void openUrl(CLOUDFLARE_TOKEN_TEMPLATE_URL)}>
              <ExternalLink aria-hidden="true" /> Create connection key
            </button>
            <label htmlFor="a11y-input-1">
              <span>Cloudflare connection key</span>
              <span className="cf-token-input"><KeyRound aria-hidden="true" /><input id="a11y-input-1" type="password" autoComplete="new-password" spellCheck={false} value={token} onChange={event => setToken(event.target.value)} placeholder="Paste the key from Cloudflare" /></span>
            </label>
            <button className="btn-primary" type="submit" disabled={!token.trim() || busy}>
              {busy ? <LoaderCircle aria-hidden="true" /> : connection.state === "degraded" ? <RefreshCw aria-hidden="true" /> : <Cloud aria-hidden="true" />}
              {busy ? "Verifying…" : connection.state === "degraded" ? "Reconnect Cloudflare" : "Connect Cloudflare"}
            </button>
          </form>
          <p className="cf-safety"><ShieldCheck aria-hidden="true" /> Limited to Workers, storefront storage, and account discovery. Your Cloudflare password is never requested.</p>
        </div>
      )}
    </section>
  );
}
