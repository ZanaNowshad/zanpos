import { describe, expect, it } from "vitest";
import {
  reduceZanAiUiState,
  sameRuntimeIdentity,
  selectSendContext,
  serializeSurfaceContext,
} from "../zanai/zanAiState";

describe("shared ZanAI runtime identity", () => {
  it("keeps one runtime only for the same token, user, and branch", () => {
    const identity = { sessionToken: "t1", userId: "u1", branchId: "b1" };

    expect(sameRuntimeIdentity(identity, { ...identity })).toBe(true);
    expect(sameRuntimeIdentity(identity, { ...identity, sessionToken: "t2" })).toBe(false);
    expect(sameRuntimeIdentity(identity, { ...identity, userId: "u2" })).toBe(false);
    expect(sameRuntimeIdentity(identity, { ...identity, branchId: "b2" })).toBe(false);
  });
});

describe("shared ZanAI UI state", () => {
  it("counts a completed reply only when its POS surface is not visible", () => {
    const initial = {
      activeSurface: "pos" as const,
      widgetOpen: false,
      widgetExpanded: false,
      unreadCount: 0,
    };

    expect(reduceZanAiUiState(initial, { type: "assistant_result" }).unreadCount).toBe(1);
    expect(
      reduceZanAiUiState({ ...initial, widgetOpen: true }, { type: "assistant_result" }).unreadCount,
    ).toBe(0);
    expect(
      reduceZanAiUiState({ ...initial, activeSurface: "office" }, { type: "assistant_result" }).unreadCount,
    ).toBe(0);
  });

  it("opening the widget clears unread without altering unrelated UI state", () => {
    const next = reduceZanAiUiState(
      { activeSurface: "pos", widgetOpen: false, widgetExpanded: true, unreadCount: 3 },
      { type: "open_widget" },
    );

    expect(next).toEqual({
      activeSurface: "pos",
      widgetOpen: true,
      widgetExpanded: true,
      unreadCount: 0,
    });
  });
});

describe("surface-scoped sends", () => {
  it("uses the explicit context supplied by the sending surface", () => {
    const registered = { surface: "office" as const, summary: "Catalogue / Products" };
    const explicit = { surface: "pos" as const, summary: "Till · 2 items" };

    expect(selectSendContext(registered, explicit)).toBe(explicit);
    expect(selectSendContext(registered)).toBe(registered);
  });

  it("preserves OfficeAI summaries and serializes structured POS context", () => {
    expect(serializeSurfaceContext({ surface: "office", summary: "Catalogue / Products" })).toBe(
      "Catalogue / Products",
    );
    expect(
      serializeSurfaceContext({
        surface: "pos",
        summary: "Till · 2 items",
        structured: {
          surface: "pos",
          captured_at: "2026-08-14T12:00:00Z",
          branch: { id: "b1", name: "Main" },
          device: { id: "d1" },
          operator: { id: "u1", display_name: "Cashier" },
          shift: { id: "s1", opened_at: "2026-08-14T08:00:00Z" },
          cart: {
            item_count: 2,
            subtotal_minor: 1000,
            discount_minor: 0,
            tax_minor: 0,
            total_minor: 1000,
            lines: [],
          },
          connection: { online: true, pending_sync_count: 0 },
        },
      }),
    ).toContain('"item_count":2');
  });
});
