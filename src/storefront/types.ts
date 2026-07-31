export type StorefrontLocale = "en" | "ar" | "en-ar";

export interface StorefrontSettings {
  enabled: boolean;
  public_url: string;
  publish_url: string;
  whatsapp_number: string;
  locale: StorefrontLocale;
  auto_publish: boolean;
  /** Write-only. The backend never serializes the stored secret back. */
  publish_secret?: string;
}

export interface StorefrontStatus {
  enabled: boolean;
  connected: boolean;
  last_release_at: string | null;
  last_release_id: string | null;
  published_product_count: number;
  eligible_product_count: number;
  dirty_product_count: number;
  failed_product_count: number;
  last_error: string | null;
}

export interface StorefrontProduct {
  product_id: string;
  name: string;
  name_ar: string | null;
  description: string | null;
  description_ar: string | null;
  price_minor: number;
  currency: string;
  image_url: string | null;
  published: boolean;
  featured: boolean;
  sort_order: number;
  dirty: boolean;
  publish_error: string | null;
}

export interface StorefrontProductPage {
  items: StorefrontProduct[];
  total: number;
  offset: number;
  limit: number;
}

export interface StorefrontProductUpdate {
  published?: boolean;
  featured?: boolean;
  sort_order?: number;
  name_ar?: string | null;
  description_ar?: string | null;
}

export interface StorefrontPublishFailure {
  product_id: string;
  name: string;
  message: string;
}

export interface StorefrontPublishResult {
  success: boolean;
  release_id: string | null;
  public_url: string;
  publish_url: string;
  published_count: number;
  failed_count: number;
  failures: StorefrontPublishFailure[];
  published_at: string | null;
}

export interface StorefrontDeployReport {
  public_url: string;
  bucket_created: boolean;
  script_uploaded: boolean;
  assets_uploaded: number;
  subdomain: string;
}

export interface StorefrontConnectionResult {
  ok: boolean;
  message: string;
  latency_ms: number | null;
}

export interface CloudflareAccount {
  id: string;
  name: string;
}

export type CloudflareConnectionState =
  | "unconfigured"
  | "account_required"
  | "connected"
  | "degraded";

export interface CloudflareConnection {
  state: CloudflareConnectionState;
  account_id: string | null;
  account_name: string | null;
  accounts: CloudflareAccount[];
  credential_stored: boolean;
  last_verified_at: string | null;
  issue: string | null;
}

export interface StorefrontReadiness {
  ready: boolean;
  issues: string[];
}
