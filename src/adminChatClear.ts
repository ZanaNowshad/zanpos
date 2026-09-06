import type { SessionToken } from "./types";

export type ClearAdminChatOptions = {
  sessionToken: SessionToken;
  branchId: string;
  newSessionId: () => string;
  clearHistory: (sessionToken: SessionToken, branchId: string) => Promise<unknown>;
  setMessages: () => void;
  setHistory: () => void;
  setSessionId: (value: string) => void;
};

export function clearAdminChat(options: ClearAdminChatOptions): void {
  options.setMessages();
  options.setHistory();
  options.setSessionId(options.newSessionId());
  options.clearHistory(options.sessionToken, options.branchId).catch((e: unknown) => console.warn("Failed to clear AI chat history:", e));
}
