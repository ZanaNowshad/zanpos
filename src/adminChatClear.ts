export type ClearAdminChatOptions = {
  branchId: string;
  userId: string;
  newSessionId: () => string;
  clearHistory: (branchId: string, userId: string) => Promise<unknown>;
  setMessages: () => void;
  setHistory: () => void;
  setSessionId: (value: string) => void;
};

export function clearAdminChat(options: ClearAdminChatOptions): void {
  options.setMessages();
  options.setHistory();
  options.setSessionId(options.newSessionId());
  options.clearHistory(options.branchId, options.userId).catch((e: unknown) => console.warn("Failed to clear AI chat history:", e));
}
