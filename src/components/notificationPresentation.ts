import type { WaMessage } from "../types";

export function relativeMessageTime(timestampSeconds: number): string {
  const difference = Math.max(0, Date.now() / 1000 - timestampSeconds);
  if (difference < 60) return "now";
  if (difference < 3600) return `${Math.floor(difference / 60)}m ago`;
  if (difference < 86400) return `${Math.floor(difference / 3600)}h ago`;
  return `${Math.floor(difference / 86400)}d ago`;
}

export function canImportCatalogFromMessage(message: Pick<WaMessage, "media_type">): boolean {
  return message.media_type === "image";
}
