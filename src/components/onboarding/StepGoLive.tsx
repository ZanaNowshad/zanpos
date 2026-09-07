import { useEffect, useState } from "react";
import * as storefront from "../../tauri/storefront";
import StorefrontManagementTab from "../settings/StorefrontManagementTab";
import type { SessionToken } from "../../types";

interface Props {
  sessionToken: SessionToken;
  language: "en" | "ar" | "en-ar";
  onDone: () => void;
}

/** Step 6 (optional) — Cloudflare connect, one-click deploy, public URL +
 * counter QR. Reuses StorefrontManagementTab wholesale; the only thing this
 * step does on its own is seed the customer language chosen in step 1 before
 * the tab loads, and provide the wizard's own "finish" action since the tab
 * has no completion callback of its own. */
export default function StepGoLive({ sessionToken, language, onDone }: Props) {
  const [seeded, setSeeded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const settings = await storefront.storefrontSettingsGet(sessionToken);
        if (!cancelled && settings.locale !== language) {
          await storefront.storefrontSettingsSave(sessionToken, { ...settings, locale: language });
        }
      } catch {
        // Non-blocking — the storefront tab loads and manages its own state either way.
      } finally {
        if (!cancelled) setSeeded(true);
      }
    })();
    return () => { cancelled = true; };
  }, [sessionToken, language]);

  return (
    <div className="setup-content setup-golive">
      <h2 className="setup-title">Go Live (optional)</h2>
      <p className="setup-body">
        Connect Cloudflare, deploy your public store, and get a counter QR customers can scan.
        You can also do this later in Settings → Storefront.
      </p>

      {seeded
        ? <StorefrontManagementTab sessionToken={sessionToken} sessionRole="owner" setupOnly />
        : <div className="setup-body-dim">Loading storefront…</div>}

      <div className="setup-actions">
        <button className="setup-btn-primary setup-btn-finish" onClick={onDone}>
          Finish Setup ✓
        </button>
      </div>
    </div>
  );
}
