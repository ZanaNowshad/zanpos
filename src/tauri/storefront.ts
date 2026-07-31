import { invoke } from "@tauri-apps/api/core";
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
  actorUserId: string,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_connection_get", { actorUserId });

export const storefrontCloudflareConnect = (
  actorUserId: string,
  apiToken: string,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_connect", { actorUserId, apiToken });

export const storefrontCloudflareSelectAccount = (
  actorUserId: string,
  accountId: string,
): Promise<CloudflareConnection> =>
  invoke("storefront_cloudflare_select_account", { actorUserId, accountId });

export const storefrontCloudflareDisconnect = (
  actorUserId: string,
): Promise<void> =>
  invoke("storefront_cloudflare_disconnect", { actorUserId });

export const storefrontStatus = (actorUserId: string): Promise<StorefrontStatus> =>
  invoke("storefront_status", { actorUserId });

export const storefrontSettingsGet = (actorUserId: string): Promise<StorefrontSettings> =>
  invoke("storefront_settings_get", { actorUserId });

export const storefrontSettingsSave = (
  actorUserId: string,
  settings: StorefrontSettings,
): Promise<StorefrontSettings> =>
  invoke("storefront_settings_save", { actorUserId, settings });

export const storefrontProductsList = (
  actorUserId: string,
  options: {
    search?: string;
    offset?: number;
    limit?: number;
    publishedOnly?: boolean;
  } = {},
): Promise<StorefrontProductPage> =>
  invoke("storefront_products_list", {
    actorUserId,
    search: options.search,
    offset: options.offset,
    limit: options.limit,
    publishedOnly: options.publishedOnly,
  });

export const storefrontProductUpdate = (
  actorUserId: string,
  productId: string,
  update: StorefrontProductUpdate,
): Promise<StorefrontProduct> =>
  invoke("storefront_product_update", { actorUserId, productId, update });

export const storefrontPublish = (actorUserId: string): Promise<StorefrontPublishResult> =>
  invoke("storefront_publish", { actorUserId });

/** One-click GO LIVE: provisions Cloudflare (bucket, worker, secret, public URL)
 *  and uploads the storefront web app. Idempotent — re-run to update. */
export const storefrontCloudflareDeploy = (
  actorUserId: string,
): Promise<StorefrontDeployReport> =>
  invoke("storefront_cloudflare_deploy", { actorUserId });

/** QR PNG data-URL for the public storefront link (for the shop counter). */
export const storefrontQr = (actorUserId: string): Promise<string> =>
  invoke("storefront_qr", { actorUserId });

export const storefrontConnectionTest = (
  actorUserId: string,
): Promise<StorefrontConnectionResult> =>
  invoke("storefront_connection_test", { actorUserId });
