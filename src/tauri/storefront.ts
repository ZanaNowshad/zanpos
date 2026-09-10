import { invoke } from "@tauri-apps/api/core";
import type { SessionToken } from "../types";
import type {
  CloudflareConnection,
  StorefrontConnectionResult,
  StorefrontDeployReport,
  StorefrontProduct,
  StorefrontProductPage,
  StorefrontProductUpdate,
  StorefrontPublishResult,
  StorefrontSettings,
  StorefrontStatus,
} from "../storefront/types";

export const storefrontCloudflareConnectionGet = (
  sessionToken: SessionToken,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_connection_get", { sessionToken });

export const storefrontCloudflareConnect = (
  sessionToken: SessionToken,
  apiToken: string,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_connect", { sessionToken, apiToken });

export const storefrontCloudflareSelectAccount = (
  sessionToken: SessionToken,
  accountId: string,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_select_account", { sessionToken, accountId });

export const storefrontCloudflareDisconnect = (
  sessionToken: SessionToken,
): Promise<void> =>
  invoke("storefront_cloudflare_disconnect", { sessionToken });

export const storefrontStatus = (sessionToken: SessionToken): Promise<StorefrontStatus> =>
  invoke("storefront_status", { sessionToken });

export const storefrontSettingsGet = (sessionToken: SessionToken): Promise<StorefrontSettings> =>
  invoke("storefront_settings_get", { sessionToken });

export const storefrontSettingsSave = (
  sessionToken: SessionToken,
  settings: StorefrontSettings,
): Promise<StorefrontSettings> =>
  invoke("storefront_settings_save", { sessionToken, settings });

export const storefrontProductsList = (
  sessionToken: SessionToken,
  options: {
    search?: string;
    offset?: number;
    limit?: number;
    publishedOnly?: boolean;
  } = {},
): Promise<StorefrontProductPage> =>
  invoke("storefront_products_list", {
    sessionToken,
    search: options.search,
    offset: options.offset,
    limit: options.limit,
    publishedOnly: options.publishedOnly,
  });

export const storefrontProductUpdate = (
  sessionToken: SessionToken,
  productId: string,
  update: StorefrontProductUpdate,
): Promise<StorefrontProduct> =>
  invoke("storefront_product_update", { sessionToken, productId, update });

export const storefrontPublish = (sessionToken: SessionToken): Promise<StorefrontPublishResult> =>
  invoke("storefront_publish", { sessionToken });

/** One-click GO LIVE: provisions Cloudflare (bucket, worker, secret, public URL)
 *  and uploads the storefront web app. Idempotent — re-run to update. */
export const storefrontCloudflareDeploy = (
  sessionToken: SessionToken,
): Promise<StorefrontDeployReport> =>
  invoke("storefront_cloudflare_deploy", { sessionToken });

/** QR PNG data-URL for the public storefront link (for the shop counter). */
export const storefrontQr = (sessionToken: SessionToken): Promise<string> =>
  invoke("storefront_qr", { sessionToken });

export const storefrontConnectionTest = (
  sessionToken: SessionToken,
): Promise<StorefrontConnectionResult> =>
  invoke("storefront_connection_test", { sessionToken });
