import { useCallback, useEffect, useState } from "react";
import { Globe, ExternalLink, ShoppingBag, AlertTriangle, QrCode } from "lucide-react";
import type { StorefrontSettings, StorefrontStatus } from "../../storefront/types";
import * as storefront from "../../tauri/storefront";
import { PageTemplate, EmptyState, LoadingSkeleton, DegradedBanner } from "../../components/templates";
import type { SessionToken } from "../../types";

interface Props {
  sessionToken: SessionToken;
  onOpenSettings?: () => void;
}

type PageState = "loading" | "setup" | "connected" | "error";

export default function ZanShopPage({ sessionToken, onOpenSettings }: Props) {
  const [pageState, setPageState] = useState<PageState>("loading");
  const [settings, setSettings] = useState<StorefrontSettings | null>(null);
  const [status, setStatus] = useState<StorefrontStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setPageState("loading");
    setError(null);
    try {
      const [s, st] = await Promise.all([
        storefront.storefrontSettingsGet(sessionToken),
        storefront.storefrontStatus(sessionToken),
      ]);
      setSettings(s);
      setStatus(st);
      setPageState(s.enabled ? "connected" : "setup");
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "Failed to load storefront");
      setPageState("error");
    }
  }, [sessionToken]);

  useEffect(() => { load(); }, [load]);

  const publishStatusLabel = () => {
    if (!status) return "";
    if (status.last_error) return "Attention";
    if (status.dirty_product_count > 0) return "Changes pending";
    if (status.published_product_count > 0) return "Live";
    return "No products published";
  };

  const publishStatusColor = () => {
    if (!status) return "";
    if (status.last_error) return "warning";
    if (status.published_product_count > 0 && status.dirty_product_count === 0) return "ok";
    return "info";
  };

  if (pageState === "loading") {
    return (
      <PageTemplate
        header={{ title: "ZANSHOP", icon: <Globe size={18} strokeWidth={1.7} aria-hidden="true" />, subtitle: "Customer online shop" }}
      >
        <LoadingSkeleton variant="card" count={3} />
      </PageTemplate>
    );
  }

  if (pageState === "error") {
    return (
      <PageTemplate
        header={{ title: "ZANSHOP", icon: <Globe size={18} strokeWidth={1.7} aria-hidden="true" /> }}
      >
        <DegradedBanner severity="warning" message={error ?? "Unable to load storefront"} />
        <EmptyState
          icon={<AlertTriangle size={36} strokeWidth={1.5} />}
          title="Could not load storefront"
          actions={[{ label: "Retry", onClick: load, primary: true }]}
        />
      </PageTemplate>
    );
  }

  if (pageState === "setup") {
    return (
      <PageTemplate
        header={{
          title: "ZANSHOP",
          icon: <Globe size={18} strokeWidth={1.7} aria-hidden="true" />,
          subtitle: "Customer online shop",
          primaryAction: onOpenSettings
            ? { label: "Setup ZANSHOP", onClick: onOpenSettings }
            : undefined,
        }}
      >
        <EmptyState
          icon={<ShoppingBag size={40} strokeWidth={1.5} />}
          title="Your online shop is not set up yet"
          description="Publish your catalogue online so customers can browse and order via WhatsApp. Setup takes just a few minutes."
          actions={[
            onOpenSettings
              ? { label: "Start Setup", onClick: onOpenSettings, primary: true }
              : { label: "Refresh", onClick: load },
          ].filter(Boolean) as { label: string; onClick: () => void; primary?: boolean }[]}
        />
      </PageTemplate>
    );
  }

  return (
    <PageTemplate
      header={{
        title: "ZANSHOP",
        icon: <Globe size={18} strokeWidth={1.7} aria-hidden="true" />,
        subtitle: settings?.public_url ?? "Customer online shop",
        primaryAction: settings?.public_url
          ? { label: "Open Shop", onClick: () => window.open(settings!.public_url, "_blank") }
          : undefined,
        secondaryActions: onOpenSettings
          ? [{ label: "Settings", onClick: onOpenSettings }]
          : [],
      }}
    >
      {status?.last_error && (
        <DegradedBanner
          severity="warning"
          message={`Publish issue: ${status.last_error}`}
          action={{ label: "Open Settings", onClick: onOpenSettings ?? (() => {}) }}
        />
      )}

      <div className="zanshop-grid">
        {/* Status card */}
        <div className="zanshop-card">
          <div className="zanshop-card-header">
            <span className={`oa-pulse-chip oa-pulse-${publishStatusColor()}`}>
              {publishStatusLabel()}
            </span>
          </div>
          <div className="zanshop-card-body">
            <div className="zanshop-stat">
              <span className="zanshop-stat-label">Published products</span>
              <span className="zanshop-stat-value">{status?.published_product_count ?? 0} / {status?.eligible_product_count ?? 0}</span>
            </div>
            {status && status.dirty_product_count > 0 && (
              <div className="zanshop-stat zanshop-stat-attention">
                <span className="zanshop-stat-label">Changes to publish</span>
                <span className="zanshop-stat-value">{status.dirty_product_count} products</span>
              </div>
            )}
            {status && status.failed_product_count > 0 && (
              <div className="zanshop-stat zanshop-stat-error">
                <span className="zanshop-stat-label">Failed to publish</span>
                <span className="zanshop-stat-value">{status.failed_product_count} products</span>
              </div>
            )}
          </div>
        </div>

        {/* Public URL card */}
        {settings?.public_url && (
          <div className="zanshop-card">
            <div className="zanshop-card-header">
              <h3>Public shop URL</h3>
            </div>
            <div className="zanshop-card-body">
              <div className="zanshop-url-row">
                <code className="zanshop-url">{settings.public_url}</code>
                <div className="zanshop-url-actions">
                  <button
                    className="btn-secondary"
                    onClick={() => window.open(settings.public_url, "_blank")}
                    title="Open shop in browser"
                  >
                    <ExternalLink size={14} /> Open
                  </button>
                </div>
              </div>
              <div className="zanshop-qr-hint">
                <QrCode size={16} strokeWidth={1.5} />
                <span>Share this link with customers. They can browse and order via WhatsApp.</span>
              </div>
            </div>
          </div>
        )}

        {/* WhatsApp card */}
        {settings?.whatsapp_number && (
          <div className="zanshop-card">
            <div className="zanshop-card-header">
              <h3>WhatsApp Orders</h3>
            </div>
            <div className="zanshop-card-body">
              <div className="zanshop-stat">
                <span className="zanshop-stat-label">WhatsApp number</span>
                <span className="zanshop-stat-value">{settings.whatsapp_number}</span>
              </div>
            </div>
          </div>
        )}
      </div>
    </PageTemplate>
  );
}
