import { describe, expect, it, vi } from "vitest";
import { clearAdminChat } from "../adminChatClear";
import { asSessionToken } from "../types";

const TOKEN = asSessionToken("session-secret");

describe("clearAdminChat", () => {
  it("clears local chat state, rotates the session, and clears persisted history", async () => {
    const calls: string[] = [];
    const clearHistory = vi.fn(() => Promise.resolve());

    clearAdminChat({
      sessionToken: TOKEN,
      branchId: "branch-1",
      newSessionId: () => "next-session",
      clearHistory,
      setMessages: () => calls.push("messages"),
      setHistory: () => calls.push("history"),
      setSessionId: value => calls.push(`session:${value}`),
    });

    await vi.waitFor(() => {
      expect(clearHistory).toHaveBeenCalledWith("session-secret", "branch-1");
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
      sessionToken: TOKEN,
      branchId: "branch-1",
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
