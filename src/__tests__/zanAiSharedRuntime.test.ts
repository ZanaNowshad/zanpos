import { describe, expect, it } from "vitest";
import { reduceZanAiUiState, sameRuntimeIdentity } from "../zanai/zanAiState";

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
