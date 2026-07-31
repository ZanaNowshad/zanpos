import { describe, expect, it } from "vitest";
import { canImportCatalogFromMessage } from "../components/NotificationModal";

describe("WhatsApp notification actions", () => {
  it("allows catalog import from business group images", () => {
    expect(canImportCatalogFromMessage({ media_type: "image" })).toBe(true);
  });

  it("does not offer catalog import for text-only messages", () => {
    expect(canImportCatalogFromMessage({ media_type: null })).toBe(false);
  });
});
