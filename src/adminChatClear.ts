export type ClearAdminChatOptions = {
  sessionToken: string;
  branchId: string;
  newSessionId: () => string;
  clearHistory: (sessionToken: string, branchId: string) => Promise<unknown>;
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
