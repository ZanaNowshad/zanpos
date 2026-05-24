import { describe, expect, it, vi } from "vitest";
import { clearAdminChat } from "../adminChatClear";

describe("clearAdminChat", () => {
  it("clears local chat state, rotates the session, and clears persisted history", async () => {
    const calls: string[] = [];
    const clearHistory = vi.fn(() => Promise.resolve());

    clearAdminChat({
      branchId: "branch-1",
      userId: "user-1",
      newSessionId: () => "next-session",
      clearHistory,
      setMessages: () => calls.push("messages"),
      setHistory: () => calls.push("history"),
      setSessionId: value => calls.push(`session:${value}`),
    });

    await vi.waitFor(() => {
      expect(clearHistory).toHaveBeenCalledWith("branch-1", "user-1");
    });
    expect(calls).toEqual([
      "messages",
      "history",
      "session:next-session",
    ]);
  });

  it("swallows persistence failures", async () => {
    const clearHistory = vi.fn(() => Promise.reject(new Error("offline")));

    clearAdminChat({
      branchId: "branch-1",
      userId: "user-1",
      newSessionId: () => "next-session",
      clearHistory,
      setMessages: () => {},
      setHistory: () => {},
      setSessionId: () => {},
    });

    await vi.waitFor(() => {
      expect(clearHistory).toHaveBeenCalledOnce();
    });
  });
});
