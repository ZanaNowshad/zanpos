import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import StorefrontReadinessSummary from "../components/settings/StorefrontReadinessSummary";
import CloudflareConnectionPanel, {
  CLOUDFLARE_TOKEN_TEMPLATE_URL,
} from "../components/settings/CloudflareConnectionPanel";
import {
  PUBLISH_NUDGE_STORAGE_KEY,
  PUBLISH_NUDGE_WEEK_MS,
  clearPublishNudgeDismissal,
  getPublishNudgeStorage,
  publishNudgeWakeDelay,
  getStorefrontReadiness,
  nextStorefrontOffset,
  previousStorefrontOffset,
  readPublishNudgeDismissal,
  shouldShowPublishNudge,
  storefrontPageRange,
  writePublishNudgeDismissal,
} from "../storefront/storefrontUtils";
import {
  storefrontCloudflareConnect,
  storefrontCloudflareConnectionGet,
  storefrontCloudflareDisconnect,
  storefrontCloudflareSelectAccount,
  storefrontProductUpdate,
  storefrontProductsList,
  storefrontSettingsSave,
} from "../tauri/storefront";
import type {
  StorefrontProduct,
  StorefrontSettings,
  StorefrontStatus,
} from "../storefront/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const settings: StorefrontSettings = {
  enabled: true,
  public_url: "https://shop.zanpos.app/pearl-market",
  publish_url: "https://cdn.zanpos.app/pearl-market",
  whatsapp_number: "+973 3300 1234",
  locale: "en-ar",
  auto_publish: false,
};

const status: StorefrontStatus = {
  enabled: true,
  connected: true,
  last_release_at: "2026-07-23T12:00:00Z",
  last_release_id: "rel_23",
  published_product_count: 2,
  eligible_product_count: 3,
  dirty_product_count: 1,
  failed_product_count: 0,
  last_error: null,
};

const product = (overrides: Partial<StorefrontProduct>): StorefrontProduct => ({
  product_id: "p-1",
  name: "Rose Water",
  name_ar: "ماء ورد",
  description: "Bahraini rose water",
  description_ar: "ماء ورد بحريني",
  price_minor: 1250,
  currency: "BHD",
  image_url: null,
  published: true,
  featured: false,
  sort_order: 1,
  dirty: false,
  publish_error: null,
  ...overrides,
});

describe("storefront command contract", () => {
  beforeEach(() => invoke.mockReset());

  it("saves settings using camelCase invoke arguments", async () => {
    invoke.mockResolvedValue(settings);
    await storefrontSettingsSave("owner-1", settings);
    expect(invoke).toHaveBeenCalledWith("storefront_settings_save", {
      actorUserId: "owner-1",
      settings,
    });
  });

  it("provisions a write-only publishing secret inside the settings payload", async () => {
    const provisioned: StorefrontSettings = {
      ...settings,
      publish_secret: "a-secure-publish-secret",
    };
    invoke.mockResolvedValue(settings);
    await storefrontSettingsSave("owner-1", provisioned);
    expect(invoke).toHaveBeenCalledWith("storefront_settings_save", {
      actorUserId: "owner-1",
      settings: provisioned,
    });
  });

  it("updates publication controls with an explicit product id", async () => {
    const update = { published: false, featured: true, sort_order: 4 };
    invoke.mockResolvedValue(product(update));
    await storefrontProductUpdate("manager-1", "p-1", update);
    expect(invoke).toHaveBeenCalledWith("storefront_product_update", {
      actorUserId: "manager-1",
      productId: "p-1",
      update,
    });
  });

  it("requests a bounded catalogue page with server-side search", async () => {
    invoke.mockResolvedValue({ items: [product({})], total: 72, offset: 25, limit: 25 });
    await storefrontProductsList("manager-1", {
      search: "rose",
      offset: 25,
      limit: 25,
      publishedOnly: false,
    });
    expect(invoke).toHaveBeenCalledWith("storefront_products_list", {
      actorUserId: "manager-1",
      search: "rose",
      offset: 25,
      limit: 25,
      publishedOnly: false,
    });
  });

  it("connects Cloudflare without persisting the token in frontend state", async () => {
    const connection = {
      state: "account_required" as const,
      account_id: null,
      account_name: null,
      accounts: [{ id: "acc-1", name: "Pearl Market" }],
      credential_stored: true,
      last_verified_at: "2026-07-23T12:00:00Z",
      issue: null,
    };
    invoke.mockResolvedValue(connection);

    await storefrontCloudflareConnect("owner-1", "secret-token");

    expect(invoke).toHaveBeenCalledWith("storefront_cloudflare_connect", {
      actorUserId: "owner-1",
      apiToken: "secret-token",
    });
    expect(connection).not.toHaveProperty("api_token");
  });

  it("loads, selects, and disconnects a Cloudflare account with explicit commands", async () => {
    invoke.mockResolvedValue({});

    await storefrontCloudflareConnectionGet("owner-1");
    await storefrontCloudflareSelectAccount("owner-1", "acc-1");
    await storefrontCloudflareDisconnect("owner-1");

    expect(invoke).toHaveBeenNthCalledWith(1, "storefront_cloudflare_connection_get", {
      actorUserId: "owner-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "storefront_cloudflare_select_account", {
      actorUserId: "owner-1",
      accountId: "acc-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "storefront_cloudflare_disconnect", {
      actorUserId: "owner-1",
    });
  });
});

describe("storefront publish readiness", () => {
  it("is ready when connection, customer path, and products are complete", () => {
    expect(getStorefrontReadiness(settings, status)).toEqual({
      ready: true,
      issues: [],
    });
  });

  it("explains every blocking customer-journey issue", () => {
    const result = getStorefrontReadiness(
      { ...settings, public_url: "", whatsapp_number: "" },
      { ...status, connected: false, published_product_count: 0 },
    );
    expect(result.ready).toBe(false);
    expect(result.issues).toEqual([
      "Add the customer-facing URL",
      "Add the WhatsApp ordering number",
      "Connect the publishing destination",
      "Choose at least one product to publish",
    ]);
  });

  it("does not call a switched-off storefront ready", () => {
    const result = getStorefrontReadiness(
      { ...settings, enabled: false },
      status,
    );
    expect(result.issues).toContain("Enable the customer storefront");
  });

  it("keeps readiness global when the current page has no published products", () => {
    expect(getStorefrontReadiness(settings, {
      ...status,
      published_product_count: 12,
    }).ready).toBe(true);
  });

  it("renders a concise release and dirty-state summary", () => {
    const html = renderToStaticMarkup(
      <StorefrontReadinessSummary
        readiness={{ ready: true, issues: [] }}
        status={status}
      />,
    );
    expect(html).toContain("Ready to publish");
    expect(html).toContain("1 change waiting");
    expect(html).toContain("2 products live");
  });
});

describe("storefront catalogue pagination", () => {
  it("reports first, middle, last, and empty page ranges", () => {
    expect(storefrontPageRange(0, 25, 72)).toEqual({ start: 1, end: 25 });
    expect(storefrontPageRange(25, 25, 72)).toEqual({ start: 26, end: 50 });
    expect(storefrontPageRange(50, 22, 72)).toEqual({ start: 51, end: 72 });
    expect(storefrontPageRange(0, 0, 0)).toEqual({ start: 0, end: 0 });
  });

  it("clamps previous and refuses to advance past the final page", () => {
    expect(previousStorefrontOffset(0, 25)).toBe(0);
    expect(previousStorefrontOffset(25, 25)).toBe(0);
    expect(nextStorefrontOffset(25, 25, 72)).toBe(50);
    expect(nextStorefrontOffset(50, 25, 72)).toBe(50);
  });
});

describe("weekly publish nudge", () => {
  const nowMs = 2_000_000_000_000;
  const input = {
    enabled: true,
    dirtyProductCount: 1,
    dismissedAtMs: null,
    nowMs,
  };

  it("shows for existing drift and stays advisory when clean or disabled", () => {
    expect(shouldShowPublishNudge(input)).toBe(true);
    expect(shouldShowPublishNudge({ ...input, dirtyProductCount: 0 })).toBe(false);
    expect(shouldShowPublishNudge({ ...input, enabled: false })).toBe(false);
  });

  it("stays dismissed for one week and returns at the boundary", () => {
    expect(shouldShowPublishNudge({
      ...input,
      dismissedAtMs: nowMs - PUBLISH_NUDGE_WEEK_MS + 1,
    })).toBe(false);
    expect(shouldShowPublishNudge({
      ...input,
      dismissedAtMs: nowMs - PUBLISH_NUDGE_WEEK_MS,
    })).toBe(true);
    expect(publishNudgeWakeDelay(nowMs, nowMs)).toBe(PUBLISH_NUDGE_WEEK_MS);
    expect(publishNudgeWakeDelay(
      nowMs,
      nowMs + PUBLISH_NUDGE_WEEK_MS - 1,
    )).toBe(1);
    expect(publishNudgeWakeDelay(
      nowMs,
      nowMs + PUBLISH_NUDGE_WEEK_MS,
    )).toBe(0);
  });

  it("fails toward showing for invalid or future dismissal times", () => {
    expect(shouldShowPublishNudge({ ...input, dismissedAtMs: Number.NaN })).toBe(true);
    expect(shouldShowPublishNudge({ ...input, dismissedAtMs: nowMs + 1 })).toBe(true);
    expect(shouldShowPublishNudge({ ...input, dismissedAtMs: -1 })).toBe(true);
  });

  it("reads, writes, and clears the reminder timestamp", () => {
    const values = new Map<string, string>();
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value); },
      removeItem: (key: string) => { values.delete(key); },
    };
    writePublishNudgeDismissal(storage, nowMs);
    expect(values.get(PUBLISH_NUDGE_STORAGE_KEY)).toBe(String(nowMs));
    expect(readPublishNudgeDismissal(storage)).toBe(nowMs);
    clearPublishNudgeDismissal(storage);
    expect(readPublishNudgeDismissal(storage)).toBeNull();
  });

  it("swallows unavailable storage so the reminder cannot affect operations", () => {
    const storage = {
      getItem: () => { throw new Error("unavailable"); },
      setItem: () => { throw new Error("unavailable"); },
      removeItem: () => { throw new Error("unavailable"); },
    };
    expect(readPublishNudgeDismissal(storage)).toBeNull();
    expect(() => writePublishNudgeDismissal(storage, nowMs)).not.toThrow();
    expect(() => clearPublishNudgeDismissal(storage)).not.toThrow();
    expect(getPublishNudgeStorage()).toBeNull();
  });
});

describe("Cloudflare connection experience", () => {
  it("opens a token template with every required permission and account scope", () => {
    const url = new URL(CLOUDFLARE_TOKEN_TEMPLATE_URL);
    const permissions = JSON.parse(url.searchParams.get("permissionGroupKeys") ?? "[]");

    expect(permissions).toEqual(expect.arrayContaining([
      { key: "workers_scripts", type: "edit" },
      { key: "workers_r2", type: "edit" },
      { key: "account_settings", type: "read" },
    ]));
    expect(url.searchParams.get("accountId")).toBe("*");
    expect(url.searchParams.get("zoneId")).toBe("all");
  });

  it("presents a guided route and secure connection field", () => {
    const html = renderToStaticMarkup(
      <CloudflareConnectionPanel
        connection={{
          state: "unconfigured",
          account_id: null,
          account_name: null,
          accounts: [],
          credential_stored: false,
          last_verified_at: null,
          issue: null,
        }}
        busy={false}
        onConnect={vi.fn()}
        onSelectAccount={vi.fn()}
        onDisconnect={vi.fn()}
      />,
    );

    expect(html).toContain("ZANPOS shelf");
    expect(html).toContain("Cloudflare edge");
    expect(html).toContain("Customer link");
    expect(html).toContain('type="password"');
    expect(html).toContain("Create connection key");
  });

  it("never renders a credential after Cloudflare is connected", () => {
    const html = renderToStaticMarkup(
      <CloudflareConnectionPanel
        connection={{
          state: "connected",
          account_id: "acc-1",
          account_name: "Pearl Market",
          accounts: [{ id: "acc-1", name: "Pearl Market" }],
          credential_stored: true,
          last_verified_at: "2026-07-23T12:00:00Z",
          issue: null,
        }}
        busy={false}
        onConnect={vi.fn()}
        onSelectAccount={vi.fn()}
        onDisconnect={vi.fn()}
      />,
    );

    expect(html).toContain("Pearl Market");
    expect(html).toContain("Connected securely");
    expect(html).not.toContain('type="password"');
  });
});
