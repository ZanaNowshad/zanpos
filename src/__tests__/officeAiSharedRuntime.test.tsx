import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ChatController } from "../officeai/useChatController";
import type { SessionUser } from "../types";

const sharedController = {
  messages: [{
    id: "shared-message",
    role: "assistant",
    text: "Shared conversation is still available",
    timestamp: new Date("2026-08-14T12:00:00Z"),
  }],
  input: "",
  setInput: () => {},
  imageAttachment: null,
  setImageAttachment: () => {},
  chatState: "idle",
  pendingAction: null,
  pendingBatchActions: null,
  liveToolCalls: [],
  streamingMsgId: null,
  tokenCount: 0,
  streamStartTime: null,
  canStop: false,
  kpi: {
    loading: false,
    error: null,
    today: null,
    lowStockCount: 0,
    outOfStockCount: 0,
    sync: null,
    alerts: [],
  },
  runState: null,
  bulkProgress: null,
  errorMessage: null,
  dismissError: () => {},
  fetchKpi: async () => {},
  dismissAlert: async () => {},
  handleSend: async () => {},
  handleStop: async () => {},
  handleConfirm: async () => {},
  handleCancel: async () => {},
  handleUndo: async () => {},
  handleFeedback: async () => {},
  dismissForm: () => {}, conversationId: null, conversations: [], conversationsLoading: false,
  refreshConversations: async () => {}, startNewConversation: () => {},
  openConversation: async () => {}, deleteConversation: async () => {}, renameConversation: async () => {}, launcherOpen: false, setLauncherOpen: () => {}, handleClearChat: () => {},
  handleRunExecute: async () => {},
  handleRunCancel: () => {},
  handleRunUndo: async () => {},
} satisfies ChatController;

vi.mock("../officeai/useChatController", () => ({
  useChatController: () => {
    throw new Error("OfficeAI created a second chat controller");
  },
}));

vi.mock("../zanai/useZanAi", () => ({
  useZanAi: () => ({
    ctrl: sharedController,
    registerSurfaceContext: () => {},
    navigationRequest: null,
    consumeNavigationRequest: () => {},
    dataEpoch: 0,
    uiState: {
      activeSurface: "office",
      widgetOpen: false,
      widgetExpanded: false,
      unreadCount: 0,
    },
    dispatchUi: () => {},
  }),
}));

vi.mock("../hooks/useLanguage", () => ({
  useLanguage: () => ({ language: "en", toggle: () => {} }),
}));

vi.mock("../hooks/useTheme", () => ({
  useTheme: () => ({ theme: "light", toggle: () => {} }),
}));

import OfficeAIPage from "../officeai/OfficeAIPage";

const user: SessionUser = {
  user_id: "U1",
  branch_id: "B1",
  display_name: "Store Owner",
  username: "owner",
  role_id: "ROLE_OWNER",
  role_name: "owner",
  session_token: "session-secret",
  session_expires_at: "2099-01-01T00:00:00Z",
};

describe("OfficeAI shared runtime", () => {
  it("renders from the provider without constructing a second controller", () => {
    const html = renderToStaticMarkup(
      <OfficeAIPage sessionUser={user} onBackToPOS={() => {}} initialTab="assistant" />,
    );

    expect(html).toContain("ZANPOS");
  });
});
