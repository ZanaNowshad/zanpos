import type {
  StorefrontReadiness,
  StorefrontSettings,
  StorefrontStatus,
} from "./types";

export const PUBLISH_NUDGE_WEEK_MS = 7 * 24 * 60 * 60 * 1000;
export const PUBLISH_NUDGE_STORAGE_KEY = "zanpos_storefront_publish_nudge_dismissed_at_v1";

export interface PublishNudgeInput {
  enabled: boolean;
  dirtyProductCount: number;
  dismissedAtMs: number | null;
  nowMs: number;
}

export interface NudgeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function getPublishNudgeStorage(): NudgeStorage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function publishNudgeWakeDelay(
  dismissedAtMs: number | null,
  nowMs: number,
): number | null {
  if (
    dismissedAtMs == null
    || !Number.isFinite(dismissedAtMs)
    || dismissedAtMs < 0
    || dismissedAtMs > nowMs
  ) {
    return null;
  }
  return Math.max(0, PUBLISH_NUDGE_WEEK_MS - (nowMs - dismissedAtMs));
}

export function shouldShowPublishNudge({
  enabled,
  dirtyProductCount,
  dismissedAtMs,
  nowMs,
}: PublishNudgeInput): boolean {
  if (!enabled || dirtyProductCount <= 0) return false;
  if (dismissedAtMs == null || !Number.isFinite(dismissedAtMs)) return true;
  if (dismissedAtMs < 0 || dismissedAtMs > nowMs) return true;
  return nowMs - dismissedAtMs >= PUBLISH_NUDGE_WEEK_MS;
}

export function readPublishNudgeDismissal(storage: NudgeStorage | null): number | null {
  if (!storage) return null;
  try {
    const stored = storage.getItem(PUBLISH_NUDGE_STORAGE_KEY);
    if (stored == null || stored.trim() === "") return null;
    const value = Number(stored);
    return Number.isFinite(value) && value >= 0 ? value : null;
  } catch {
    return null;
  }
}

export function writePublishNudgeDismissal(storage: NudgeStorage | null, dismissedAtMs: number): void {
  if (!storage) return;
  try {
    storage.setItem(PUBLISH_NUDGE_STORAGE_KEY, String(dismissedAtMs));
  } catch {
    // Advisory reminder persistence must never affect storefront operations.
  }
}

export function clearPublishNudgeDismissal(storage: NudgeStorage | null): void {
  if (!storage) return;
  try {
    storage.removeItem(PUBLISH_NUDGE_STORAGE_KEY);
  } catch {
    // A stale reminder is harmless; publishing must still complete normally.
  }
}

export function getStorefrontReadiness(
  settings: StorefrontSettings,
  status: StorefrontStatus,
): StorefrontReadiness {
  const issues: string[] = [];
  if (!settings.enabled) issues.push("Enable the customer storefront");
  if (!settings.public_url.trim()) issues.push("Add the customer-facing URL");
  if (!settings.whatsapp_number.trim()) issues.push("Add the WhatsApp ordering number");
  if (!status.connected) issues.push("Connect the publishing destination");
  if (status.published_product_count <= 0) {
    issues.push("Choose at least one product to publish");
  }
  return { ready: issues.length === 0, issues };
}

export function storefrontPageRange(
  offset: number,
  itemCount: number,
  total: number,
): { start: number; end: number } {
  if (total <= 0 || itemCount <= 0) return { start: 0, end: 0 };
  const safeOffset = Math.max(0, Math.trunc(offset));
  return {
    start: safeOffset + 1,
    end: Math.min(safeOffset + Math.max(0, Math.trunc(itemCount)), total),
  };
}

export function previousStorefrontOffset(offset: number, pageSize: number): number {
  return Math.max(0, Math.trunc(offset) - Math.max(1, Math.trunc(pageSize)));
}

export function nextStorefrontOffset(
  offset: number,
  pageSize: number,
  total: number,
): number {
  const safePageSize = Math.max(1, Math.trunc(pageSize));
  const candidate = Math.max(0, Math.trunc(offset)) + safePageSize;
  return candidate < total ? candidate : Math.max(0, Math.trunc(offset));
}

export function formatStorefrontMoney(
  amountMinor: number,
  currency: string,
): string {
  const exponent = currency === "BHD" ? 3 : 2;
  return `${currency} ${(amountMinor / (10 ** exponent)).toFixed(exponent)}`;
}
