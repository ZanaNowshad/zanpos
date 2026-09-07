import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Copy,
  ExternalLink,
  Globe2,
  LoaderCircle,
  QrCode,
  Wifi,
} from "lucide-react";
import type {
  CloudflareConnection,
  StorefrontConnectionResult,
  StorefrontSettings,
} from "../../storefront/types";
import * as storefront from "../../tauri/storefront";
import CloudflareConnectionPanel from "./CloudflareConnectionPanel";
import StorefrontOrderCapture from "./StorefrontOrderCapture";
import type { SessionToken } from "../../types";

interface Props {
  sessionToken: SessionToken;
  settings: StorefrontSettings;
  onSettingsChange: (settings: StorefrontSettings) => void;
}

const LOCALES: Array<{ value: StorefrontSettings["locale"]; label: string }> = [
  { value: "en-ar", label: "English + العربية" },
  { value: "en", label: "English" },
  { value: "ar", label: "العربية" },
];

function messageFrom(error: unknown, fallback: string) {
  return typeof error === "string"
    ? error
    : error instanceof Error ? error.message : fallback;
}

/**
 * Some Cloudflare failures are not faults — they are an account setting the
 * owner has not switched on yet. Presenting "R2 bucket creation failed" as a
 * generic error leaves the user with a dead end, when the fix is two clicks in
 * a dashboard we can link to directly.
 */
interface Guidance {
  headline: string;
  what: string;
  stillWorks: string;
  link?: { label: string; href: string };
}

function guidanceFor(message: string): Guidance | null {
  const m = message.toLowerCase();
  if (m.includes("10042") || (m.includes("r2") && m.includes("enable"))) {
    return {
      headline: "Cloudflare R2 storage is not enabled yet",
      what: "Your online shop needs R2 storage to publish its pages and images. R2 is switched on once, per Cloudflare account.",
      stillWorks: "Selling, printing and everything else in Command are unaffected. Nothing has been lost — publish again once R2 is on.",
      link: { label: "Open Cloudflare R2 settings", href: "https://dash.cloudflare.com/?to=/:account/r2" },
    };
  }
  if (m.includes("unauthorized") || m.includes("403") || m.includes("invalid api token")) {
    return {
      headline: "Cloudflare rejected the API token",
      what: "The token is missing, expired, or lacks R2 and Workers permissions.",
      stillWorks: "In-store selling is unaffected.",
      link: { label: "Open Cloudflare API tokens", href: "https://dash.cloudflare.com/profile/api-tokens" },
    };
  }
  return null;
}

export default function StorefrontSetupPage({
  sessionToken,
  settings,
  onSettingsChange,
}: Props) {
  const [cloudflare, setCloudflare] = useState<CloudflareConnection | null>(null);
  const [cloudflareBusy, setCloudflareBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [deployNote, setDeployNote] = useState<string | null>(null);
  const [connection, setConnection] = useState<StorefrontConnectionResult | null>(null);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    storefront.storefrontCloudflareConnectionGet(sessionToken)
      .then(value => { if (!cancelled) setCloudflare(value); })
      .catch(loadError => {
        if (!cancelled) setError(messageFrom(loadError, "Cloudflare status could not be loaded."));
      });
    return () => { cancelled = true; };
  }, [sessionToken]);

  useEffect(() => {
    if (!settings.public_url) {
      setQrDataUrl(null);
      return;
    }
    let cancelled = false;
    storefront.storefrontQr(sessionToken)
      .then(value => { if (!cancelled) setQrDataUrl(value); })
      .catch(() => { if (!cancelled) setQrDataUrl(null); });
    return () => { cancelled = true; };
  }, [sessionToken, settings.public_url]);

  function patchSettings(update: Partial<StorefrontSettings>) {
    onSettingsChange({ ...settings, ...update });
    setSaved(false);
  }

  async function saveSettings() {
    if (settings.public_url && !/^https:\/\//i.test(settings.public_url)) {
      setError("Public URL must start with https://");
      return;
    }
    if (
      settings.publish_url
      && !/^https:\/\/|^http:\/\/(?:localhost|127\.0\.0\.1)(?::\d+)?$/i.test(settings.publish_url)
    ) {
      setError("Publish URL must use HTTPS, except when testing on localhost.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      onSettingsChange(await storefront.storefrontSettingsSave(sessionToken, settings));
      setSaved(true);
    } catch (saveError) {
      setError(messageFrom(saveError, "ZanShop settings could not be saved."));
    } finally {
      setSaving(false);
    }
  }

  async function testConnection() {
    setTesting(true);
    setConnection(null);
    setError(null);
    try {
      setConnection(await storefront.storefrontConnectionTest(sessionToken));
    } catch (testError) {
      setError(messageFrom(testError, "Connection test failed."));
    } finally {
      setTesting(false);
    }
  }

  async function deployStorefront() {
    setDeploying(true);
    setDeployNote(null);
    setError(null);
    try {
      const report = await storefront.storefrontCloudflareDeploy(sessionToken);
      setDeployNote(
        `Live at ${report.public_url} — ${report.assets_uploaded} file(s) uploaded`
        + (report.bucket_created ? ", storage created" : ""),
      );
      onSettingsChange(await storefront.storefrontSettingsGet(sessionToken));
    } catch (deployError) {
      setError(messageFrom(deployError, "Deployment failed."));
    } finally {
      setDeploying(false);
    }
  }

  async function updateCloudflare(action: () => Promise<CloudflareConnection>) {
    setCloudflareBusy(true);
    setError(null);
    try {
      setCloudflare(await action());
    } catch (cloudflareError) {
      setError(messageFrom(cloudflareError, "Cloudflare could not be updated."));
    } finally {
      setCloudflareBusy(false);
    }
  }

  async function copyPublicUrl() {
    if (!settings.public_url) return;
    try {
      await navigator.clipboard.writeText(settings.public_url);
      setConnection({ ok: true, message: "Customer link copied.", latency_ms: null });
    } catch {
      setError("The customer link could not be copied.");
    }
  }

  return (
    <>
      <label className="sf-master-toggle sf-setup-master-toggle">
        <span>
          <strong>Customer storefront</strong>
          <small>{settings.enabled ? "Visible when published" : "Currently switched off"}</small>
        </span>
        <input
          type="checkbox"
          checked={settings.enabled}
          onChange={event => patchSettings({ enabled: event.target.checked })}
        />
        <span className="sf-toggle-track" aria-hidden="true" />
      </label>

      <section className="sf-section" aria-labelledby="sf-destination-title">
        <div className="sf-section-heading">
          <div><span>Setup</span><h3 id="sf-destination-title">Where the shop lives</h3></div>
          <button className="sf-text-action" onClick={testConnection} disabled={testing}>
            <Wifi aria-hidden="true" /> {testing ? "Testing…" : "Test connection"}
          </button>
        </div>
        {cloudflare && (
          <CloudflareConnectionPanel
            connection={cloudflare}
            busy={cloudflareBusy}
            onConnect={apiToken => updateCloudflare(
              () => storefront.storefrontCloudflareConnect(sessionToken, apiToken),
            )}
            onSelectAccount={accountId => updateCloudflare(
              () => storefront.storefrontCloudflareSelectAccount(sessionToken, accountId),
            )}
            onDisconnect={() => updateCloudflare(async () => {
              await storefront.storefrontCloudflareDisconnect(sessionToken);
              return storefront.storefrontCloudflareConnectionGet(sessionToken);
            })}
          />
        )}
        {cloudflare?.state === "connected" && (
          <div className="sf-golive">
            <button className="sf-primary-action" onClick={deployStorefront} disabled={deploying}>
              {deploying
                ? <><LoaderCircle className="sf-spin" aria-hidden="true" /> Going live…</>
                : <><Globe2 aria-hidden="true" /> {settings.public_url ? "Update live site" : "Go live"}</>}
            </button>
            {deployNote && <span className="sf-deploy-note">{deployNote}</span>}
            {!settings.public_url && !deploying && (
              <span className="sf-deploy-hint">
                One click creates your free store link on Cloudflare — nothing to install.
              </span>
            )}
          </div>
        )}
        {qrDataUrl && settings.public_url && (
          <div className="sf-qr">
            <img src={qrDataUrl} alt={`QR code for ${settings.public_url}`} width={132} height={132} />
            <div>
              <strong><QrCode size={14} aria-hidden="true" /> Counter QR</strong>
              <p>Customers scan this to open your store and order on WhatsApp.</p>
            </div>
          </div>
        )}
        <div className="sf-field-grid">
          <label>
            Public customer URL
            <span className="sf-url-input">
              <input value={settings.public_url} onChange={event => patchSettings({ public_url: event.target.value })} placeholder="https://shop.example.com" />
              <button type="button" aria-label="Copy customer URL" onClick={copyPublicUrl} disabled={!settings.public_url}><Copy /></button>
              <button type="button" aria-label="Open customer URL" onClick={() => void openUrl(settings.public_url)} disabled={!settings.public_url}><ExternalLink /></button>
            </span>
          </label>
          <label htmlFor="a11y-input-1">
            WhatsApp ordering number
            <input id="a11y-input-1" value={settings.whatsapp_number} onChange={event => patchSettings({ whatsapp_number: event.target.value })} placeholder="+973 3300 1234" inputMode="tel" />
          </label>
          <label htmlFor="a11y-input-2">
            Customer language
            <select id="a11y-input-2" value={settings.locale} onChange={event => patchSettings({ locale: event.target.value as StorefrontSettings["locale"] })}>
              {LOCALES.map(locale => <option key={locale.value} value={locale.value}>{locale.label}</option>)}
            </select>
          </label>
        </div>
        <details className="sf-advanced-endpoint">
          <summary>Advanced · Custom publishing endpoint</summary>
          <div className="sf-field-grid">
            <label htmlFor="a11y-input-3">CDN / publish destination<input value={settings.publish_url} onChange={event => patchSettings({ publish_url: event.target.value })} placeholder="https://shop.example.com" /></label>
            <label htmlFor="a11y-fix-StorefrontSetupPage">
              Publishing secret
              <input id="a11y-input-3" type="password" autoComplete="new-password" value={settings.publish_secret ?? ""} onChange={event => patchSettings({ publish_secret: event.target.value || undefined })} placeholder="Leave blank to keep the stored secret" />
              <small>Use this only for an existing custom deployment.</small>
            </label>
          </div>
        </details>
        <div className="sf-settings-footer">
          <p className="sf-publish-guidance">Catalogue changes stay queued until you publish one atomic release.</p>
          <button className="btn-primary" onClick={saveSettings} disabled={saving}>
            {saving ? "Saving…" : saved ? "Saved" : "Save setup"}
          </button>
        </div>
        {connection && <p className={`sf-inline-result ${connection.ok ? "is-success" : "is-error"}`} role="status">{connection.message}{connection.latency_ms != null ? ` · ${connection.latency_ms} ms` : ""}</p>}
        {error && (() => {
          const g = guidanceFor(error);
          if (!g) {
            return (
              <div className="sf-error-banner" role="alert">
                <strong>Storefront setup needs attention</strong>
                <span>{error}</span>
              </div>
            );
          }
          return (
            <div className="sf-error-banner zp-sf-guidance" role="alert">
              <strong>{g.headline}</strong>
              <span>{g.what}</span>
              <span className="zp-sf-stillworks">{g.stillWorks}</span>
              {g.link && (
                <a className="zp-sf-link" href={g.link.href} target="_blank" rel="noreferrer noopener">
                  {g.link.label}
                </a>
              )}
              <details className="zp-sf-detail">
                <summary>Technical detail</summary>
                <code>{error}</code>
              </details>
            </div>
          );
        })()}
      </section>

      <StorefrontOrderCapture sessionToken={sessionToken} />
    </>
  );
}
