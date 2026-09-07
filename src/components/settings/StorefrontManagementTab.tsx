import { useEffect, useState } from "react";
import { Globe2, PackageSearch, Settings2 } from "lucide-react";
import type { StorefrontSettings } from "../../storefront/types";
import * as storefront from "../../tauri/storefront";
import StorefrontCataloguePage from "./StorefrontCataloguePage";
import StorefrontSetupPage from "./StorefrontSetupPage";
import "./StorefrontManagement.css";
import type { SessionToken } from "../../types";

interface Props {
  sessionToken: SessionToken;
  sessionRole: string;
  setupOnly?: boolean;
}

type StorefrontPage = "setup" | "catalogue";

function messageFrom(error: unknown) {
  return typeof error === "string"
    ? error
    : error instanceof Error ? error.message : "Storefront settings could not be loaded.";
}

export default function StorefrontManagementTab({
  sessionToken,
  sessionRole,
  setupOnly = false,
}: Props) {
  const [page, setPage] = useState<StorefrontPage>("setup");
  const [settings, setSettings] = useState<StorefrontSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    storefront.storefrontSettingsGet(sessionToken)
      .then(nextSettings => {
        if (!cancelled) setSettings(nextSettings);
      })
      .catch(loadError => {
        if (!cancelled) setError(messageFrom(loadError));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [sessionToken]);

  if (loading) {
    return <div className="sf-loading"><Globe2 aria-hidden="true" /> Loading ZanShop setup…</div>;
  }
  if (!settings) {
    return (
      <div className="sf-unavailable" role="alert">
        <Globe2 aria-hidden="true" />
        <strong>ZanShop setup is unavailable</strong>
        <span>{error ?? "The storefront service did not return its configuration."}</span>
      </div>
    );
  }

  return (
    <div className="sf-shell">
      <header className="sf-hero">
        <div>
          <span className="sf-eyebrow"><Globe2 aria-hidden="true" /> ZanShop</span>
          <h2>{page === "setup" ? "Set up the customer shop." : "Choose what customers can browse."}</h2>
          <p>
            {page === "setup"
              ? "Manage the customer link, language, ordering and publishing destination."
              : "Work through the catalogue one page at a time, then publish one clean release."}
          </p>
        </div>
      </header>

      {!setupOnly && (
        <div className="settings-command-nav" role="tablist" aria-label="ZanShop sections">
          <button
            type="button"
            role="tab"
            aria-selected={page === "setup"}
            className={`settings-command-tab${page === "setup" ? " active" : ""}`}
            onClick={() => setPage("setup")}
          >
            <Settings2 aria-hidden="true" /><strong>Setup</strong>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={page === "catalogue"}
            className={`settings-command-tab${page === "catalogue" ? " active" : ""}`}
            onClick={() => setPage("catalogue")}
          >
            <PackageSearch aria-hidden="true" /><strong>Catalogue</strong>
          </button>
        </div>
      )}

      {page === "setup" || setupOnly ? (
        <StorefrontSetupPage
          sessionToken={sessionToken}
          settings={settings}
          onSettingsChange={setSettings}
        />
      ) : (
        <StorefrontCataloguePage
          sessionToken={sessionToken}
          settings={settings}
        />
      )}

      <footer className="sf-role-note">
        {sessionRole === "owner" ? "Owner workspace" : "Manager workspace"}
        {" · "}Changes are recorded against this signed-in user.
      </footer>
    </div>
  );
}
